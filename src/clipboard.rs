//! Native clipboard adapter. File lists keep their originals; bitmaps become
//! draft-owned PNG assets. Clipboard reads never run on the terminal thread.
mod wsl;

use crate::staging::{Attachment, Prepared, Request};
use anyhow::{Context, Result, ensure};
use image::ImageEncoder;
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, OnceLock, Weak,
        atomic::{AtomicBool, Ordering},
    },
};

static LIVE_ASSETS: OnceLock<Mutex<HashMap<PathBuf, Weak<Asset>>>> = OnceLock::new();

fn live_assets() -> std::sync::MutexGuard<'static, HashMap<PathBuf, Weak<Asset>>> {
    LIVE_ASSETS
        .get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

const MAX_RGBA: usize = 128 * 1024 * 1024;
const MAX_PNG: u64 = 16 * 1024 * 1024;
const MAX_TEXT: usize = 1024 * 1024;

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
            .is_some_and(|name| name.starts_with("clipboard-") && name.ends_with(".png"))
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
    finish_png(file, request)
}

fn finish_png(file: tempfile::NamedTempFile, request: &Request) -> Result<Prepared> {
    file.as_file().sync_all()?;
    ensure!(
        file.as_file().metadata()?.len() <= MAX_PNG.min(request.available_bytes),
        "Clipboard PNG exceeds 16 MiB or the draft budget"
    );
    let mut prepared = crate::staging::prepare_paths(vec![file.path().to_owned()], request);
    let attachment: &mut Attachment = prepared
        .attachments
        .first_mut()
        .context("could not prepare clipboard PNG")?;
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
