//! File preparation uses Yazi's platform argument parser and bounded background
//! work. Original files stay references until an explicit send is confirmed.

use crate::{drafts::Key, event::NetworkEvent, model::sanitize_terminal_line};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub const MAX_FILES: usize = 8;
pub const MAX_BYTES: u64 = 2 * 1024 * 1024 * 1024;
static PREPARATION: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);

#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
// Independent Lua preferences, not states of a single operation.
#[allow(clippy::struct_excessive_bools)]
pub struct Configuration {
    /// General file paths require opt-in; decodable images attach by default.
    pub auto_attach_paths: bool,
    pub auto_attach_images: bool,
    pub clipboard_as_photo: bool,
    pub terminal_clipboard: bool,
}

impl Default for Configuration {
    fn default() -> Self {
        Self {
            auto_attach_paths: false,
            auto_attach_images: true,
            clipboard_as_photo: true,
            terminal_clipboard: true,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct Attachment {
    pub path: PathBuf,
    pub size: u64,
    pub digest: [u8; 32],
    pub photo_supported: bool,
    pub as_photo: bool,
    #[serde(default)]
    pub owned: bool,
    #[serde(skip)]
    pub(crate) lease: Option<std::sync::Arc<crate::clipboard::Asset>>,
}

impl Attachment {
    pub(crate) fn retain_owned(&self) {
        if let Some(lease) = &self.lease {
            lease.retain();
        }
    }

    pub(crate) fn discard_owned(&self) {
        if let Some(lease) = &self.lease {
            lease.discard();
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Input {
    Paths(String),
    ImagePaths(String),
    Clipboard,
    Terminal(crate::clipboard::Payload),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Request {
    pub key: Key,
    pub id: u64,
    pub input: Input,
    pub as_photo: bool,
    pub auto_attach_images: bool,
    pub available_files: usize,
    pub available_bytes: u64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Prepared {
    pub attachments: Vec<Attachment>,
    pub errors: Vec<String>,
    pub text: Option<String>,
}

/// Parse native file arguments; no shell expansion or execution takes place.
/// # Errors
/// Returns an argument or file-URL error.
pub fn paths(value: &str) -> Result<Vec<PathBuf>> {
    // A single existing path may contain spaces without shell quoting. This
    // check runs on the preparation thread, never during a UI redraw.
    let direct = Path::new(value.trim());
    if direct.is_file() {
        return Ok(vec![direct.to_owned()]);
    }
    // A complete local URL has URL escaping, not shell escaping. In
    // particular an apostrophe can remain literal inside a file URL.
    if let Ok(url) = url::Url::parse(value.trim())
        && url.scheme() == "file"
        && let Ok(path) = url.to_file_path()
        && path.is_file()
    {
        return Ok(vec![path]);
    }
    #[cfg(unix)]
    let words = yazi_shared::shell::unix::split(value, false)?.0;
    #[cfg(windows)]
    let words = yazi_shared::shell::windows::split(&format!("termgram {value}"))?
        .into_iter()
        .skip(1)
        .collect::<Vec<_>>();
    ensure!(!words.is_empty(), "Choose at least one file");
    words
        .into_iter()
        .map(|word| {
            if word.starts_with("file:") {
                url::Url::parse(&word)?
                    .to_file_path()
                    .map_err(|()| anyhow::anyhow!("File URL is not local to this machine"))
            } else {
                Ok(PathBuf::from(word))
            }
        })
        .collect()
}

fn hash_copy(
    reader: &mut File,
    mut output: Option<&mut File>,
    limit: u64,
) -> Result<(u64, [u8; 32])> {
    let mut hash = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024];
    let mut total = 0_u64;
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        total = total.saturating_add(count as u64);
        ensure!(
            total <= limit,
            "Attachment exceeds the available staging budget"
        );
        hash.update(&buffer[..count]);
        if let Some(file) = &mut output {
            file.write_all(&buffer[..count])?;
        }
    }
    Ok((total, hash.finalize().into()))
}

pub(crate) fn prepare(request: &Request) -> Result<Prepared> {
    if let Input::ImagePaths(value) = &request.input {
        return Ok(prepare_image_paste(value, request));
    }
    let Input::Paths(value) = &request.input else {
        anyhow::bail!("native clipboard requires the preparation worker");
    };
    Ok(prepare_paths(paths(value)?, request))
}

/// A cheap hint keeps ordinary prose on the synchronous text path. Actual
/// regular-file and image-header checks belong to the preparation worker.
#[must_use]
pub fn image_path_candidate(value: &str) -> bool {
    image::ImageFormat::from_path(value.trim().trim_matches(['\'', '"'])).is_ok()
}

/// Follow Codex's image-dimensions check after Yazi parses quoted/escaped paths.
/// All paths must be images, otherwise the original paste stays intact as text.
pub(crate) fn prepare_image_paste(value: &str, request: &Request) -> Prepared {
    if let Ok(paths) = paths(value)
        && !paths.is_empty()
        && paths.len() <= MAX_FILES
        && paths.iter().all(|path| {
            path.is_file()
                && image::ImageReader::open(path)
                    .and_then(image::ImageReader::with_guessed_format)
                    .ok()
                    .and_then(|reader| reader.into_dimensions().ok())
                    .is_some_and(|(width, height)| width > 0 && height > 0)
        })
    {
        return prepare_paths(paths, request);
    }
    Prepared {
        text: Some(value.to_owned()),
        ..Prepared::default()
    }
}

pub(crate) fn prepare_paths(paths: Vec<PathBuf>, request: &Request) -> Prepared {
    let mut result = Prepared::default();
    let mut remaining = request.available_bytes;
    for path in paths {
        let item = (|| -> Result<Attachment> {
            ensure!(
                result.attachments.len() < request.available_files,
                "At most {MAX_FILES} files per draft"
            );
            let path = std::fs::canonicalize(&path)?;
            ensure!(path.to_str().is_some(), "Choose a file with a Unicode name");
            let metadata = std::fs::metadata(&path)?;
            ensure!(metadata.is_file(), "Only regular files can be attached");
            let mut file = File::open(&path)?;
            let metadata = file.metadata()?;
            ensure!(metadata.is_file(), "Only regular files can be attached");
            ensure!(
                metadata.len() <= remaining,
                "Draft attachments exceed the 2 GiB budget"
            );
            let (size, digest) = hash_copy(&mut file, None, remaining)?;
            let photo_supported = image::ImageReader::open(&path)?
                .with_guessed_format()?
                .format()
                .is_some_and(|format| {
                    matches!(
                        format,
                        image::ImageFormat::Png
                            | image::ImageFormat::Jpeg
                            | image::ImageFormat::WebP
                    )
                });
            Ok(Attachment {
                path,
                size,
                digest,
                photo_supported,
                as_photo: photo_supported && request.as_photo,
                owned: false,
                lease: None,
            })
        })();
        match item {
            Ok(attachment) => {
                remaining = remaining.saturating_sub(attachment.size);
                result.attachments.push(attachment);
            }
            Err(error) => result.errors.push(format!(
                "{}: {error:#}",
                sanitize_terminal_line(&path.display().to_string())
            )),
        }
    }
    result
}

/// A private immutable upload copy retains the reviewed bytes even if the
/// original file is replaced after confirmation. Its directory owns cleanup.
pub struct Upload {
    _directory: tempfile::TempDir,
    pub path: PathBuf,
}

/// # Errors
/// Rejects missing/changed files before Telegram receives any bytes.
pub fn upload_copy(path: &Path, expected: [u8; 32]) -> Result<Upload> {
    ensure!(
        std::fs::metadata(path)
            .context("attachment is no longer available")?
            .is_file(),
        "attachment is no longer a regular file"
    );
    let mut source = File::open(path).context("attachment is no longer available")?;
    ensure!(
        source.metadata()?.is_file(),
        "attachment is no longer a regular file"
    );
    let directory = tempfile::Builder::new()
        .prefix("termgram-upload-")
        .tempdir()?;
    let destination = directory
        .path()
        .join(path.file_name().context("missing attachment name")?);
    crate::config::prepare_private_file(&destination)?;
    let mut output = File::options().write(true).open(&destination)?;
    let (_, actual) = hash_copy(&mut source, Some(&mut output), MAX_BYTES)?;
    ensure!(
        actual == expected,
        "Attachment changed since it was added; remove it and attach it again"
    );
    output.flush()?;
    Ok(Upload {
        _directory: directory,
        path: destination,
    })
}

struct Work {
    request: Request,
    receiver: tokio::sync::oneshot::Receiver<Result<Prepared>>,
    deadline: Option<tokio::time::Instant>,
    expired: bool,
}

pub(crate) struct Worker {
    task: Option<Work>,
    state: PathBuf,
    owner: std::sync::Arc<File>,
}

impl Worker {
    pub fn new(state: PathBuf, owner: std::sync::Arc<File>) -> Self {
        Self {
            task: None,
            state,
            owner,
        }
    }

    pub fn start(&mut self, request: Request) -> Option<NetworkEvent> {
        let permit = PREPARATION.try_acquire();
        if self.task.is_some() || permit.is_err() {
            return Some(request.failure(
                "Another attachment or clipboard read is still being prepared".to_owned(),
            ));
        }
        let state = self.state.clone();
        let owner = self.owner.clone();
        let work = request.clone();
        let (sender, receiver) = tokio::sync::oneshot::channel();
        // A hung OS clipboard call must neither accumulate workers nor keep
        // Tokio's runtime shutdown waiting after the terminal has been restored.
        if let Err(error) = std::thread::Builder::new()
            .name("termgram-attachments".to_owned())
            .spawn(move || {
                let _permit = permit;
                let _owner = owner;
                let result = match &work.input {
                    Input::Paths(_) | Input::ImagePaths(_) => prepare(&work),
                    Input::Clipboard => crate::clipboard::read(&state, &work),
                    Input::Terminal(payload) => {
                        crate::clipboard::prepare_payload(&state, &work, payload)
                    }
                };
                let _ = sender.send(result);
            })
        {
            return Some(request.failure(error.to_string()));
        }
        self.task = Some(Work {
            deadline: matches!(request.input, Input::Clipboard | Input::ImagePaths(_))
                .then(|| tokio::time::Instant::now() + std::time::Duration::from_secs(10)),
            expired: false,
            request,
            receiver,
        });
        None
    }

    pub async fn next(&mut self) -> Option<NetworkEvent> {
        let Some(work) = &mut self.task else {
            return std::future::pending().await;
        };
        let result = tokio::select! {
            result = &mut work.receiver => result,
            () = async {
                if let Some(deadline) = work.deadline { tokio::time::sleep_until(deadline).await; }
                else { std::future::pending::<()>().await; }
            } => {
                work.expired = true;
                work.deadline = None;
                return Some(work.request.failure("Attachment preparation timed out; its backend must finish before another read can start".to_owned()));
            }
        };
        let work = self.task.take().expect("active preparation");
        if work.expired {
            return None;
        }
        Some(NetworkEvent::AttachmentsPrepared {
            key: work.request.key,
            request_id: work.request.id,
            result: result
                .map_err(anyhow::Error::from)
                .and_then(std::convert::identity)
                .map_err(|error| format!("{error:#}")),
        })
    }
}

impl Request {
    pub(crate) fn failure(&self, error: String) -> NetworkEvent {
        NetworkEvent::AttachmentsPrepared {
            key: self.key,
            request_id: self.id,
            result: Err(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn expired_preparation_stays_bounded_and_discards_its_late_result() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let mut worker = Worker::new(
            file.path().to_owned(),
            std::sync::Arc::new(file.reopen().unwrap()),
        );
        let request = Request {
            key: Key {
                account: 1,
                chat: 2,
                topic: 0,
            },
            id: 1,
            input: Input::Clipboard,
            as_photo: true,
            auto_attach_images: true,
            available_files: MAX_FILES,
            available_bytes: MAX_BYTES,
        };
        let (sender, receiver) = tokio::sync::oneshot::channel();
        worker.task = Some(Work {
            request: request.clone(),
            receiver,
            deadline: Some(tokio::time::Instant::now()),
            expired: false,
        });
        assert!(matches!(
            worker.next().await,
            Some(NetworkEvent::AttachmentsPrepared { result: Err(_), .. })
        ));
        assert!(
            worker.start(request).is_some(),
            "timeout does not free a still-running backend"
        );
        sender.send(Ok(Prepared::default())).unwrap();
        assert!(
            worker.next().await.is_none(),
            "late completion never reaches the UI"
        );
        assert!(worker.task.is_none());
    }

    #[test]
    fn reviewed_bytes_are_copied_privately_and_changes_rejected() {
        let root = tempfile::tempdir().unwrap();
        let original = root.path().join("a '猫' file.txt");
        std::fs::write(&original, b"reviewed bytes").unwrap();
        let prepared = prepare(&Request {
            key: Key {
                account: 1,
                chat: 2,
                topic: 0,
            },
            id: 1,
            input: Input::Paths(url::Url::from_file_path(&original).unwrap().to_string()),
            as_photo: true,
            auto_attach_images: true,
            available_files: MAX_FILES,
            available_bytes: MAX_BYTES,
        })
        .unwrap();
        assert!(prepared.errors.is_empty(), "{:?}", prepared.errors);
        let file = &prepared.attachments[0];
        let upload = upload_copy(&file.path, file.digest).unwrap();
        let copy = upload.path.clone();
        std::fs::write(&original, b"changed bytes").unwrap();
        assert_eq!(std::fs::read(&copy).unwrap(), b"reviewed bytes");
        assert!(upload_copy(&file.path, file.digest).is_err());
        drop(upload);
        assert!(!copy.exists());
        assert_eq!(std::fs::read(&original).unwrap(), b"changed bytes");
    }
}
