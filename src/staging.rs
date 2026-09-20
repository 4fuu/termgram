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

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Configuration {
    /// Plain terminal paste remains text unless explicitly enabled.
    pub auto_attach_paths: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct Attachment {
    pub path: PathBuf,
    pub size: u64,
    pub digest: [u8; 32],
    pub photo_supported: bool,
    pub as_photo: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Request {
    pub key: Key,
    pub id: u64,
    pub paths: String,
    pub available_files: usize,
    pub available_bytes: u64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Prepared {
    pub attachments: Vec<Attachment>,
    pub errors: Vec<String>,
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
    let paths = paths(&request.paths)?;
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
                as_photo: photo_supported,
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
    Ok(result)
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

#[derive(Default)]
pub(crate) struct Worker {
    task: Option<(Request, tokio::task::JoinHandle<Result<Prepared>>)>,
}

impl Worker {
    pub fn start(&mut self, request: Request) -> Option<NetworkEvent> {
        let permit = PREPARATION.try_acquire();
        if self.task.is_some() || permit.is_err() {
            return Some(NetworkEvent::AttachmentsPrepared {
                key: request.key,
                request_id: request.id,
                result: Err("Another attachment is still being prepared".to_owned()),
            });
        }
        let work = request.clone();
        self.task = Some((
            request,
            tokio::task::spawn_blocking(move || {
                let _permit = permit;
                prepare(&work)
            }),
        ));
        None
    }

    pub async fn next(&mut self) -> NetworkEvent {
        let result = if let Some((_, task)) = &mut self.task {
            task.await
        } else {
            return std::future::pending().await;
        };
        let (request, _) = self.task.take().expect("active preparation");
        NetworkEvent::AttachmentsPrepared {
            key: request.key,
            request_id: request.id,
            result: result
                .map_err(anyhow::Error::from)
                .and_then(std::convert::identity)
                .map_err(|error| format!("{error:#}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            paths: url::Url::from_file_path(&original).unwrap().to_string(),
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
