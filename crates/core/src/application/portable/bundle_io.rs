//! Bounded filesystem reads and locked, recoverable directory replacement.
use super::budget::{
    validate_bundle_budget, validate_manifest_budget, MAX_BUNDLE_BYTES, MAX_BUNDLE_FILE_BYTES,
    MAX_MANIFEST_BYTES,
};
use super::format::{
    json_error, ExportFile, PortableBundle, PortableManifest, EXPORT_FORMAT, EXPORT_VERSION,
    V1_FILE_PATHS,
};
use crate::{AppError, AppResult};
use std::fs;
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};

fn read_bounded_bundle_file(base: &Path, relative: &str, limit: usize) -> AppResult<String> {
    let base = base.canonicalize().map_err(fs_error)?;
    let path = base.join(relative).canonicalize().map_err(fs_error)?;
    if !path.starts_with(&base) {
        return Err(AppError::validation(
            "bundle file resolves outside its directory",
        ));
    }
    let metadata = fs::metadata(&path).map_err(fs_error)?;
    if !metadata.is_file() || metadata.len() > limit as u64 {
        return Err(AppError::validation(format!(
            "bundle file {relative} exceeds its size limit or is not a regular file"
        )));
    }
    let file = fs::File::open(path).map_err(fs_error)?;
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(fs_error)?;
    if bytes.len() > limit {
        return Err(AppError::validation(format!(
            "bundle file {relative} exceeds its size limit"
        )));
    }
    String::from_utf8(bytes)
        .map_err(|_| AppError::validation(format!("bundle file {relative} is not UTF-8")))
}

pub(super) fn private_directory(path: &Path) -> AppResult<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path).map_err(fs_error)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(fs_error)?;
    }
    Ok(())
}

pub(super) fn private_output_file(path: &Path) -> std::io::Result<fs::File> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

// ---------------------------------------------------------------------------
// Filesystem bundle I/O (reference adapter; std-only, reusable by adapters)
// ---------------------------------------------------------------------------

/// Filesystem bundle I/O (std-only reference adapter).
///
/// Replacement protocol — crash-recoverable with deterministic names:
///
/// 1. any interrupted swap left behind by a previous process is recovered
///    first (roll back to the previous bundle if the swap never completed);
/// 2. the new bundle is written completely into a sibling staging directory
///    `<target>.swap/new` and fsynced;
/// 3. the existing target is moved to `<target>.swap/previous`, then the new
///    bundle is moved into place; the swap directory is removed.
///
/// A process crash can therefore only leave the deterministic swap directory
/// behind — never a missing or half-written `target` — and the next read or
/// write restores a complete old or new bundle. Note std cannot fsync
/// directories; a power-loss (as opposed to process crash) may still lose
/// the rename durability, which is why recovery runs on every operation.
/// Writes a bundle to `target`, replacing any AssetMesh bundle already there.
///
/// The whole operation runs under an exclusive cross-process lock
/// ([`with_bundle_lock`]) and refuses to clobber a directory that holds
/// unrelated data: overwriting an existing destination is only permitted
/// when that destination is itself an AssetMesh bundle (or an empty
/// directory). See [`ensure_replaceable`].
pub fn write_bundle_to_directory(bundle: &PortableBundle, target: &Path) -> AppResult<()> {
    validate_bundle_budget(bundle)?;
    with_bundle_lock(target, || {
        recover_interrupted_swap(target)?;
        ensure_replaceable(target)?;

        let swap = swap_dir(target);
        private_directory(&swap)?;
        let new_dir = swap.join("new");
        private_directory(&new_dir)?;
        private_directory(&new_dir.join("modules"))?;
        for file in &bundle.files {
            let path = new_dir.join(&file.path);
            if let Some(dir) = path.parent() {
                private_directory(dir)?;
            }
            private_output_file(&path)
                .and_then(|mut f| {
                    f.write_all(file.content.as_bytes())?;
                    f.sync_all()
                })
                .map_err(fs_error)?;
        }
        let manifest_json = bundle.manifest_json()?;
        let manifest_path = new_dir.join("manifest.json");
        private_output_file(&manifest_path)
            .and_then(|mut f| {
                f.write_all(manifest_json.as_bytes())?;
                f.sync_all()
            })
            .map_err(fs_error)?;

        if target.exists() {
            fs::rename(target, swap.join("previous")).map_err(fs_error)?;
        }
        fs::rename(&new_dir, target).map_err(fs_error)?;
        let _ = fs::remove_dir_all(&swap);
        Ok(())
    })
}

/// Reads a bundle from a directory, recovering any interrupted swap first so
/// a crash between the two renames can never hide the last complete bundle.
///
/// Recovery is destructive (it deletes a leftover swap directory), so it
/// happens under the same exclusive lock a write takes: without it, a reader
/// in one process could delete the staging directory of a writer in another.
pub fn read_bundle_from_directory(source: &Path) -> AppResult<PortableBundle> {
    // A bundle on read-only media cannot create the sibling lock file. It
    // also cannot be replaced by a cooperating writer on that media. Never
    // attempt recovery here: an interrupted swap needs a writable parent.
    let parent = source.parent().unwrap_or_else(|| Path::new("."));
    if fs::metadata(parent)
        .map_err(fs_error)?
        .permissions()
        .readonly()
    {
        if swap_dir(source).exists() {
            return Err(AppError::storage_busy(
                "a read-only bundle has an interrupted export; copy it to writable storage to recover",
            ));
        }
        return read_bundle_files(source);
    }
    with_bundle_lock(source, || {
        recover_interrupted_swap(source)?;
        read_bundle_files(source)
    })
}

fn read_bundle_files(source: &Path) -> AppResult<PortableBundle> {
    let base = source.canonicalize().map_err(fs_error)?;
    let manifest_text = read_bounded_bundle_file(&base, "manifest.json", MAX_MANIFEST_BYTES)?;
    let manifest: PortableManifest = serde_json::from_str(&manifest_text).map_err(json_error)?;
    validate_manifest_budget(&manifest)?;

    let mut files = Vec::new();
    let mut total = manifest_text.len();
    for path in V1_FILE_PATHS {
        let path_buf = source.join(path);
        if path_buf.exists() {
            let content = read_bounded_bundle_file(
                &base,
                path,
                MAX_BUNDLE_FILE_BYTES.min(MAX_BUNDLE_BYTES.saturating_sub(total)),
            )?;
            total += content.len();
            files.push(ExportFile {
                path: path.into(),
                content,
            });
        }
    }

    Ok(PortableBundle { manifest, files })
}

/// Deterministic staging location: `<parent>/.<name>.swap/` with `new/` and
/// optionally `previous/` subdirectories.
pub(super) fn swap_dir(target: &Path) -> PathBuf {
    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    let file_name = target
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "assetmesh-export".to_string());
    parent.join(format!(".{file_name}.swap"))
}

/// Rolls an interrupted swap forward or back so `target` always ends up
/// pointing at a complete bundle:
/// - target missing, `previous` present → the crash happened between the two
///   renames; roll the previous (known-good) bundle back into place;
/// - target present, `previous` present → the swap completed but cleanup did
///   not; remove the swap directory;
/// - anything else → the crash happened while staging; discard the swap.
///
/// Callers must hold the bundle lock: this deletes a swap directory, and a
/// reader that raced an active writer would otherwise destroy the writer's
/// staging area.
pub(super) fn recover_interrupted_swap(target: &Path) -> AppResult<()> {
    let swap = swap_dir(target);
    if !swap.exists() {
        return Ok(());
    }
    let previous = swap.join("previous");
    if !target.exists() && previous.exists() {
        fs::rename(&previous, target).map_err(fs_error)?;
    }
    let _ = fs::remove_dir_all(&swap);
    Ok(())
}

pub(super) fn fs_error(e: std::io::Error) -> AppError {
    AppError::storage(format!("portable bundle I/O failed: {e}"))
}

// ---------------------------------------------------------------------------
// Cross-process bundle lock (OS advisory exclusive file lock)
// ---------------------------------------------------------------------------

use fs2::FileExt;

/// Runs `body` while holding an exclusive cross-process lock on `target`.
///
/// The lock is a sibling file (`<parent>/.<name>.lock`) using operating
/// system advisory exclusive file locks (`fs2::FileExt::try_lock_exclusive`).
/// When a process exits or crashes, the OS automatically releases the lock.
/// The lock file itself remains on disk.
pub fn with_bundle_lock<R>(target: &Path, body: impl FnOnce() -> AppResult<R>) -> AppResult<R> {
    let _lock = BundleLock::acquire(target)?;
    body()
}

/// Owned guard for the bundle lock file. Holding the open `File` keeps the
/// OS advisory lock active until dropped or until the process exits.
#[derive(Debug)]
pub struct BundleLock {
    _file: fs::File,
    path: PathBuf,
}

impl BundleLock {
    pub fn acquire(target: &Path) -> AppResult<Self> {
        let path = lock_path(target);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(fs_error)?;
        }
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(fs_error)?;

        match file.try_lock_exclusive() {
            Ok(()) => Ok(BundleLock { _file: file, path }),
            Err(e) if is_lock_contention(&e) => Err(AppError::storage_busy(format!(
                "another process is using the bundle at {}; retry later",
                target.display()
            ))),
            Err(e) => Err(fs_error(e)),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for BundleLock {
    fn drop(&mut self) {
        let _ = self._file.unlock();
    }
}

/// Lock location: `<parent>/.<name>.lock`, a sibling of the target bundle directory.
pub fn lock_path(target: &Path) -> PathBuf {
    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    let file_name = target
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "assetmesh-export".to_string());
    parent.join(format!(".{file_name}.lock"))
}

fn is_lock_contention(e: &std::io::Error) -> bool {
    if e.kind() == std::io::ErrorKind::WouldBlock {
        return true;
    }
    #[cfg(unix)]
    if let Some(code) = e.raw_os_error() {
        return code == 35 /* EWOULDBLOCK on macOS / BSD */ || code == 11 /* EAGAIN / EWOULDBLOCK on Linux */;
    }
    false
}

/// Guards the destructive rename in [`write_bundle_to_directory`]: a target
/// that already holds data is only replaced when that data is itself an
/// AssetMesh bundle. Silently `rename`-ing a user's existing directory into
/// the swap area deletes it, and the failure mode is not recoverable — the
/// swap directory is removed on success, so the old contents would be gone.
pub(super) fn ensure_replaceable(target: &Path) -> AppResult<()> {
    if !target.exists() {
        return Ok(());
    }
    if is_assetmesh_bundle(target)? {
        return Ok(());
    }
    // An empty directory destroys nothing, so it is an acceptable target.
    if is_empty_directory(target) {
        return Ok(());
    }
    Err(AppError::validation(format!(
        "refusing to overwrite {}: the destination exists but is not an AssetMesh export bundle. \
         Move its contents aside or export into an empty directory.",
        target.display()
    )))
}

/// True for an empty directory. Anything that cannot be enumerated — a file,
/// or an unreadable directory — is not empty, and therefore not replaceable.
fn is_empty_directory(path: &Path) -> bool {
    fs::read_dir(path)
        .map(|mut entries| entries.next().is_none())
        .unwrap_or(false)
}

/// True when `dir` contains a manifest that parses as this format. Parsing
/// the manifest (rather than only checking the file name) keeps an unrelated
/// directory that happens to contain `manifest.json` from being replaced.
fn is_assetmesh_bundle(dir: &Path) -> AppResult<bool> {
    let Ok(text) = read_bounded_bundle_file(dir, "manifest.json", MAX_MANIFEST_BYTES) else {
        return Ok(false);
    };
    match serde_json::from_str::<PortableManifest>(&text) {
        Ok(manifest) => Ok(manifest.format == EXPORT_FORMAT && manifest.version == EXPORT_VERSION),
        Err(_) => Ok(false),
    }
}
