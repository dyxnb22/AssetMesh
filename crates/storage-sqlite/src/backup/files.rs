//! Private filesystem primitives, bounded metadata reads, and backup-folder locks.
use assetmesh_core::{AppError, AppResult};
use fs2::FileExt;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::Path,
};

const MANIFEST_MAX_BYTES: u64 = 262_144;

pub(super) fn directory_bytes(path: &Path) -> AppResult<u64> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(storage_error(error)),
    };
    if metadata.is_file() {
        return Ok(metadata.len());
    }
    if !metadata.is_dir() {
        return Ok(0);
    }
    let mut total = 0_u64;
    for item in fs::read_dir(path).map_err(storage_error)? {
        total = total.saturating_add(directory_bytes(&item.map_err(storage_error)?.path())?);
    }
    Ok(total)
}

pub(super) fn backup_lock(root: &Path) -> AppResult<File> {
    private_dir(root)?;
    let path = root.join(".lock");
    if !path.exists() {
        match private_file(&path) {
            Ok(_) => {}
            Err(_) if path.exists() => {}
            Err(error) => return Err(error),
        }
    }
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .map_err(storage_error)?;
    file.lock_exclusive().map_err(storage_error)?;
    Ok(file)
}

pub(super) fn storage_error(_: impl std::fmt::Display) -> AppError {
    AppError::storage("could not read or write the backup folder")
}

pub(super) fn corrupt() -> AppError {
    AppError::corrupt_data("backup is incomplete, damaged, or unsupported")
}

pub(super) fn private_dir(path: &Path) -> AppResult<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(path)
            .map_err(storage_error)?;
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(storage_error)?;
    }
    #[cfg(not(unix))]
    fs::create_dir_all(path).map_err(storage_error)?;
    Ok(())
}

pub(super) fn private_file(path: &Path) -> AppResult<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path).map_err(storage_error)
}

pub(super) fn atomic_json<T: Serialize>(path: &Path, value: &T) -> AppResult<()> {
    let tmp = path.with_extension(format!("partial-{}", uuid::Uuid::now_v7()));
    let result = (|| {
        let mut file = private_file(&tmp)?;
        file.write_all(&serde_json::to_vec_pretty(value).map_err(storage_error)?)
            .map_err(storage_error)?;
        file.sync_all().map_err(storage_error)?;
        fs::rename(&tmp, path).map_err(storage_error)?;
        File::open(path.parent().ok_or_else(corrupt)?)
            .and_then(|f| f.sync_all())
            .map_err(storage_error)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(tmp);
    }
    result
}

pub(super) fn checksum(path: &Path) -> AppResult<String> {
    if !fs::metadata(path).map_err(storage_error)?.is_file() {
        return Err(corrupt());
    }
    let mut file = File::open(path).map_err(storage_error)?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65_536];
    loop {
        let n = file.read(&mut buffer).map_err(storage_error)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

pub(super) fn read_bounded_json<T: serde::de::DeserializeOwned>(path: &Path) -> AppResult<T> {
    let metadata = fs::metadata(path).map_err(storage_error)?;
    if !metadata.is_file() || metadata.len() > MANIFEST_MAX_BYTES {
        return Err(corrupt());
    }
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(storage_error)?
        .take(MANIFEST_MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(storage_error)?;
    if bytes.len() as u64 > MANIFEST_MAX_BYTES {
        return Err(corrupt());
    }
    serde_json::from_slice(&bytes).map_err(|_| corrupt())
}
