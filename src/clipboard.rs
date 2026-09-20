//! Native clipboard adapter. File lists keep their originals; bitmaps become
//! draft-owned PNG assets. Clipboard reads never run on the terminal thread.
mod wsl;

pub mod copy;

use crate::staging::{Attachment, Prepared, Request};
use anyhow::{Context, Result, ensure};
use image::{ImageDecoder, ImageEncoder};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, OnceLock, Weak,
        atomic::{AtomicBool, Ordering},
    },
};

// Arboard operations must not overlap on Windows. Both background adapters
// share this guard; no clipboard access or lock wait runs on the terminal thread.
static NATIVE_ACCESS: Mutex<()> = Mutex::new(());

static LIVE_ASSETS: OnceLock<Mutex<HashMap<PathBuf, Weak<Asset>>>> = OnceLock::new();

fn live_assets() -> std::sync::MutexGuard<'static, HashMap<PathBuf, Weak<Asset>>> {
    LIVE_ASSETS
        .get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

const MAX_RGBA: usize = 128 * 1024 * 1024;
const MAX_ENCODED: u64 = 16 * 1024 * 1024;
const MAX_TEXT: usize = 1024 * 1024;

#[derive(Clone, Eq, PartialEq)]
pub struct Payload {
    pub mime: String,
    pub bytes: Arc<[u8]>,
    pub remote: bool,
}

impl std::fmt::Debug for Payload {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ClipboardPayload")
            .field("mime", &self.mime)
            .field("bytes", &self.bytes.len())
            .field("remote", &self.remote)
            .finish()
    }
}

pub(crate) fn prepare_payload(
    state: &Path,
    request: &Request,
    payload: &Payload,
) -> Result<Prepared> {
    ensure!(
        u64::try_from(payload.bytes.len())? <= MAX_ENCODED,
        "Terminal clipboard exceeds 16 MiB"
    );
    if matches!(
        payload.mime.as_str(),
        "text/plain" | "text/plain;charset=utf-8"
    ) {
        ensure!(
            payload.bytes.len() <= MAX_TEXT,
            "Clipboard text exceeds 1 MiB"
        );
        return Ok(Prepared {
            text: Some(std::str::from_utf8(&payload.bytes)?.to_owned()),
            ..Prepared::default()
        });
    }
    if payload.mime == "text/uri-list" {
        ensure!(
            !payload.remote,
            "Clipboard file paths belong to the terminal host; use :attach with a file readable on this machine"
        );
        ensure!(
            payload.bytes.len() <= MAX_TEXT,
            "Clipboard file list exceeds 1 MiB"
        );
        let text = std::str::from_utf8(&payload.bytes)?;
        let mut paths = Vec::new();
        let mut errors = Vec::new();
        for line in text
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
        {
            ensure!(
                paths.len() + errors.len() < crate::staging::MAX_FILES,
                "Clipboard contains more than eight files"
            );
            match url::Url::parse(line)
                .ok()
                .filter(|url| url.scheme() == "file")
                .and_then(|url| url.to_file_path().ok())
            {
                Some(path) => paths.push(path),
                None => errors.push(
                    "Clipboard contains a file URL that is not local to this machine".to_owned(),
                ),
            }
        }
        ensure!(
            !paths.is_empty() || !errors.is_empty(),
            "Clipboard file list is empty"
        );
        let mut prepared = crate::staging::prepare_paths(paths, request);
        prepared.errors.extend(errors);
        return Ok(prepared);
    }
    let (format, suffix) = match payload.mime.as_str() {
        "image/png" => (image::ImageFormat::Png, ".png"),
        "image/jpeg" => (image::ImageFormat::Jpeg, ".jpg"),
        "image/webp" => (image::ImageFormat::WebP, ".webp"),
        "image/gif" => (image::ImageFormat::Gif, ".gif"),
        _ => anyhow::bail!("Unsupported clipboard MIME type"),
    };
    ensure!(
        image::guess_format(&payload.bytes)? == format,
        "Clipboard image does not match its MIME type"
    );
    ensure!(
        request.available_files > 0,
        "Remove an attachment before adding another"
    );
    let mut reader = image::ImageReader::with_format(std::io::Cursor::new(&payload.bytes), format);
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(MAX_RGBA as u64);
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    reader.limits(limits);
    let decoder = reader.into_decoder()?;
    let (width, height) = decoder.dimensions();
    ensure!(
        width > 0 && height > 0 && decoder.total_bytes() <= MAX_RGBA as u64,
        "Clipboard image exceeds the 128 MiB decoded size limit"
    );
    let directory = prepare_root(state, request.key.account)?;
    let mut file = tempfile::Builder::new()
        .prefix("clipboard-")
        .suffix(suffix)
        .tempfile_in(directory)?;
    std::io::Write::write_all(file.as_file_mut(), &payload.bytes)?;
    finish_asset(file, request)
}

#[derive(Debug)]
pub(crate) struct Asset {
    path: PathBuf,
    retained: AtomicBool,
}

impl PartialEq for Asset {
    fn eq(&self, other: &Self) -> bool {
        self.path == other.path
    }
}
impl Eq for Asset {}

impl Asset {
    fn new(path: PathBuf, retained: bool) -> Arc<Self> {
        let mut live = live_assets();
        live.retain(|_, lease| lease.strong_count() > 0);
        if let Some(existing) = live.get(&path).and_then(Weak::upgrade) {
            return existing;
        }
        let lease = Arc::new(Self {
            path: path.clone(),
            retained: AtomicBool::new(retained),
        });
        live.insert(path, Arc::downgrade(&lease));
        lease
    }
    pub(crate) fn retain(&self) {
        self.retained.store(true, Ordering::Release);
    }
    pub(crate) fn discard(&self) {
        self.retained.store(false, Ordering::Release);
    }
}

impl Drop for Asset {
    fn drop(&mut self) {
        if !self.retained.load(Ordering::Acquire) {
            // Only paths created here or validated under the account's managed
            // root acquire a lease. Original files can never enter this branch.
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

fn root(state: &Path, account: i64) -> PathBuf {
    let mut path = state.as_os_str().to_owned();
    path.push(".attachments");
    PathBuf::from(path).join(account.to_string())
}

fn prepare_root(state: &Path, account: i64) -> Result<PathBuf> {
    ensure!(
        account > 0,
        "Wait for the Telegram account identity before pasting media"
    );
    let path = root(state, account);
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(&path)?;
    ensure!(
        std::fs::symlink_metadata(&path)?.is_dir(),
        "draft asset storage must be a directory"
    );
    std::fs::canonicalize(path).context("cannot resolve draft asset storage")
}

/// Called while loading this account's drafts, under its existing cache lock.
/// Unreferenced crash remnants are separate from the downloaded-media cache.
pub(crate) fn restore(
    state: &Path,
    account: i64,
    drafts: &mut [crate::drafts::Stored],
) -> Result<()> {
    let path = root(state, account);
    if !path.exists() {
        return Ok(());
    }
    ensure!(
        std::fs::symlink_metadata(&path)?.is_dir(),
        "draft asset storage must be a directory"
    );
    let directory = std::fs::canonicalize(path)?;
    let mut referenced: HashSet<_> = drafts
        .iter()
        .flat_map(|draft| &draft.attachments)
        .map(|file| file.path.clone())
        .collect();
    // A quick account switch can precede the coalesced draft commit. Protect
    // every in-process lease as well as persisted references during recovery.
    referenced.extend(
        live_assets()
            .iter()
            .filter(|(_, lease)| lease.strong_count() > 0)
            .map(|(path, _)| path.clone()),
    );
    for draft in drafts {
        for file in &mut draft.attachments {
            if file.owned && managed_path(&directory, &file.path) {
                file.lease = Some(Asset::new(file.path.clone(), true));
            }
        }
    }
    for entry in std::fs::read_dir(&directory)? {
        let entry = entry?;
        let path = entry.path();
        if managed_path(&directory, &path)
            && entry.file_type()?.is_file()
            && !referenced.contains(&path)
        {
            let _ = std::fs::remove_file(path);
        }
    }
    Ok(())
}

// Managed filenames are minted here with an exact lowercase suffix.
#[allow(clippy::case_sensitive_file_extension_comparisons)]
fn managed_path(directory: &Path, path: &Path) -> bool {
    path.parent() == Some(directory)
        && path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| {
                name.starts_with("clipboard-")
                    && [".png", ".jpg", ".webp", ".gif"]
                        .iter()
                        .any(|suffix| name.ends_with(suffix))
            })
}

pub(crate) fn read(state: &Path, request: &Request) -> Result<Prepared> {
    match read_native(state, request) {
        Ok(prepared) => Ok(prepared),
        Err(native_error) if cfg!(target_os = "linux") && wsl::available() => {
            wsl::read(state, request).with_context(|| {
                format!("Native clipboard: {native_error:#}; Windows clipboard fallback failed")
            })
        }
        Err(error) => Err(error),
    }
}

fn read_native(state: &Path, request: &Request) -> Result<Prepared> {
    let _access = NATIVE_ACCESS
        .lock()
        .map_err(|_| anyhow::anyhow!("Native clipboard lock failed"))?;
    let mut clipboard = arboard::Clipboard::new()
        .context("System clipboard unavailable; use :attach for a local file")?;
    if let Ok(files) = clipboard.get().file_list()
        && !files.is_empty()
    {
        return Ok(crate::staging::prepare_paths(files, request));
    }
    if let Ok(image) = clipboard.get_image() {
        return bitmap(state, request, &image);
    }
    let text = clipboard
        .get_text()
        .context("Clipboard has no supported files, image or text")?;
    ensure!(text.len() <= MAX_TEXT, "Clipboard text exceeds 1 MiB");
    Ok(Prepared {
        text: Some(text),
        ..Prepared::default()
    })
}

fn bitmap(state: &Path, request: &Request, image: &arboard::ImageData<'_>) -> Result<Prepared> {
    ensure!(
        request.available_files > 0,
        "Remove an attachment before adding another"
    );
    let expected = image
        .width
        .checked_mul(image.height)
        .and_then(|size| size.checked_mul(4));
    ensure!(
        image.width > 0
            && image.height > 0
            && expected == Some(image.bytes.len())
            && image.bytes.len() <= MAX_RGBA,
        "Clipboard bitmap is invalid or exceeds 128 MiB RGBA"
    );
    let width = u32::try_from(image.width)?;
    let height = u32::try_from(image.height)?;
    let directory = prepare_root(state, request.key.account)?;
    let mut file = tempfile::Builder::new()
        .prefix("clipboard-")
        .suffix(".png")
        .tempfile_in(directory)?;
    image::codecs::png::PngEncoder::new(file.as_file_mut()).write_image(
        &image.bytes,
        width,
        height,
        image::ExtendedColorType::Rgba8,
    )?;
    finish_asset(file, request)
}

fn finish_asset(file: tempfile::NamedTempFile, request: &Request) -> Result<Prepared> {
    file.as_file().sync_all()?;
    ensure!(
        file.as_file().metadata()?.len() <= MAX_ENCODED.min(request.available_bytes),
        "Clipboard image exceeds 16 MiB or the draft budget"
    );
    let mut prepared = crate::staging::prepare_paths(vec![file.path().to_owned()], request);
    let attachment: &mut Attachment = prepared
        .attachments
        .first_mut()
        .context("could not prepare clipboard image")?;
    let (_, path) = file.keep()?;
    attachment.owned = true;
    attachment.lease = Some(Asset::new(path, false));
    Ok(prepared)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        drafts::{Draft, Key},
        staging::{Input, MAX_BYTES, MAX_FILES},
    };
    use std::borrow::Cow;

    #[test]
    fn terminal_images_keep_encoded_bytes_and_reject_remote_file_paths() {
        let directory = tempfile::tempdir().unwrap();
        let state = directory.path().join("state.sqlite3");
        let request = Request {
            key: Key {
                account: 100,
                chat: 7,
                topic: 0,
            },
            id: 1,
            input: Input::Clipboard,
            as_photo: false,
            available_files: MAX_FILES,
            available_bytes: MAX_BYTES,
        };
        let mut encoded = Vec::new();
        image::codecs::png::PngEncoder::new(&mut encoded)
            .write_image(&[120_u8; 16], 2, 2, image::ExtendedColorType::Rgba8)
            .unwrap();
        let mut payload = Payload {
            mime: "image/png".to_owned(),
            bytes: encoded.clone().into(),
            remote: true,
        };
        let prepared = prepare_payload(&state, &request, &payload).unwrap();
        let path = prepared.attachments[0].path.clone();
        assert_eq!(std::fs::read(&path).unwrap(), encoded);
        assert!(!prepared.attachments[0].as_photo);
        drop(prepared);
        assert!(!path.exists());
        payload.mime = "image/jpeg".to_owned();
        assert!(prepare_payload(&state, &request, &payload).is_err());
        payload.mime = "text/uri-list".to_owned();
        payload.bytes = b"file:///a/local/desktop/file".as_slice().into();
        assert!(
            prepare_payload(&state, &request, &payload)
                .unwrap_err()
                .to_string()
                .contains("terminal host")
        );
        payload.mime = "text/plain".to_owned();
        assert_eq!(
            prepare_payload(&state, &request, &payload)
                .unwrap()
                .text
                .as_deref(),
            Some("file:///a/local/desktop/file")
        );
    }

    #[test]
    fn bitmap_assets_survive_draft_restore_and_clean_up_only_when_released() {
        let directory = tempfile::tempdir().unwrap();
        let state = directory.path().join("state.sqlite3");
        let request = Request {
            key: Key {
                account: 100,
                chat: 7,
                topic: 0,
            },
            id: 1,
            input: Input::Clipboard,
            as_photo: false,
            available_files: MAX_FILES,
            available_bytes: MAX_BYTES,
        };
        let pixels = [120_u8; 16];
        let image = arboard::ImageData {
            width: 2,
            height: 2,
            bytes: Cow::Borrowed(&pixels),
        };
        let pending = bitmap(&state, &request, &image).unwrap();
        let abandoned = pending.attachments[0].path.clone();
        drop(pending);
        assert!(
            !abandoned.exists(),
            "unaccepted clipboard results clean up their own files"
        );
        let prepared = bitmap(&state, &request, &image).unwrap();
        let file = &prepared.attachments[0];
        assert!(!file.as_photo);
        file.retain_owned();
        let path = file.path.clone();
        restore(&state, 100, &mut []).unwrap();
        assert!(
            path.exists(),
            "a quick account switch cannot collect an unflushed draft"
        );
        let draft = Draft {
            attachments: prepared.attachments,
            ..Draft::default()
        };
        let serialized = serde_json::to_string(&draft.stored(7, 0)).unwrap();
        drop(draft);
        assert!(path.exists());
        let orphan = path.parent().unwrap().join("clipboard-orphan.png");
        let unrelated = path.parent().unwrap().join("personal.txt");
        std::fs::write(&orphan, b"orphan").unwrap();
        std::fs::write(&unrelated, b"keep").unwrap();
        let mut restored = vec![serde_json::from_str(&serialized).unwrap()];
        restore(&state, 100, &mut restored).unwrap();
        assert!(path.exists());
        assert!(!orphan.exists());
        assert!(unrelated.exists());
        let held_by_transfer = restored[0].attachments[0].clone();
        restored[0].attachments[0].discard_owned();
        drop(restored);
        assert!(
            path.exists(),
            "a transfer keeps its file until its final lease drops"
        );
        drop(held_by_transfer);
        assert!(!path.exists());
        let invalid = arboard::ImageData {
            width: usize::MAX,
            height: 2,
            bytes: Cow::Borrowed(&pixels),
        };
        assert!(bitmap(&state, &request, &invalid).is_err());
    }
}
