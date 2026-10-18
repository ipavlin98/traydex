use super::Storage;
use crate::i18n::{fill, tr};
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

pub fn is_image(path: &Path) -> bool {
    path.extension().is_some_and(|s| {
        matches!(s.to_string_lossy().to_lowercase().as_str(), "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp" | "ico")
    })
}

pub fn is_archive(path: &Path) -> bool {
    path.extension().is_some_and(|s| {
        matches!(
            s.to_string_lossy().to_lowercase().as_str(),
            "zip" | "rar" | "7z" | "tar" | "gz" | "tgz" | "bz2" | "xz" | "zst" | "cab" | "lz" | "lzma"
        )
    })
}

pub fn save_note(storage: &Storage, text: &str) -> Result<PathBuf> {
    let path = storage.unique("note", "txt");
    if let Err(error) = fs::write(&path, text) {
        storage.discard(&path);
        return Err(error.into());
    }
    Ok(path)
}

pub fn save_virtual(storage: &Storage, name: &str, bytes: &[u8]) -> Result<PathBuf> {
    let path = storage.allocate(name)?;
    if let Err(error) = fs::write(&path, bytes) {
        storage.release(&path);
        return Err(error.into());
    }
    Ok(path)
}

pub fn set_aside(path: &Path) -> Result<PathBuf> {
    let broken = path.with_extension("broken.json");
    fs::rename(path, &broken).with_context(|| broken.display().to_string())?;
    Ok(broken)
}

pub fn restore_note(path: &Path, text: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let name = path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
    let temp = path.with_file_name(format!(".{name}.tmp"));
    if let Err(error) = fs::write(&temp, text).and_then(|_| fs::rename(&temp, path)) {
        let _ = fs::remove_file(&temp);
        return Err(error).with_context(|| path.display().to_string());
    }
    Ok(())
}

pub fn vacant(path: PathBuf) -> PathBuf {
    let stem = path.file_stem().unwrap_or_default().to_string_lossy().into_owned();
    let ext = path.extension().map(|s| format!(".{}", s.to_string_lossy())).unwrap_or_default();
    let mut candidate = path.clone();
    let mut n = 2;
    while candidate.exists() {
        candidate = path.with_file_name(format!("{stem} ({n}){ext}"));
        n += 1;
    }
    candidate
}

pub(super) fn copy_path(source: &Path, dest: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(source)?;
    if metadata.file_type().is_symlink() {
        anyhow::bail!(fill(tr().symlink_skipped, source.display()));
    }
    if metadata.is_dir() {
        let source_real = fs::canonicalize(source)?;
        let parent_real = fs::canonicalize(dest.parent().context(tr().no_target)?)?;
        if parent_real.starts_with(&source_real) {
            anyhow::bail!(tr().into_itself);
        }
        fs::create_dir(dest)?;
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            copy_path(&entry.path(), &dest.join(entry.file_name()))?;
        }
    } else {
        fs::copy(source, dest).with_context(|| fill(tr().copy_file_failed, source.display()))?;
    }
    Ok(())
}
