use super::Progress;
use crate::i18n::{fill, tr};
use crate::store::Storage;
use crate::system::Services;
use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use std::{fs, io};
use zip::ZipArchive;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tool {
    Ffmpeg,
    YtDlp,
    Deno,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Presence {
    Missing,
    System,
    Bundled,
}

impl Presence {
    pub fn ready(self) -> bool {
        self != Presence::Missing
    }
}

impl Tool {
    pub const ALL: [Tool; 3] = [Tool::Ffmpeg, Tool::YtDlp, Tool::Deno];

    pub fn name(self) -> &'static str {
        match self {
            Tool::Ffmpeg => "ffmpeg",
            Tool::YtDlp => "yt-dlp",
            Tool::Deno => "Deno",
        }
    }

    fn file(self) -> &'static str {
        match self {
            Tool::Ffmpeg => "ffmpeg.exe",
            Tool::YtDlp => "yt-dlp.exe",
            Tool::Deno => "deno.exe",
        }
    }

    fn url(self) -> &'static str {
        match self {
            Tool::Ffmpeg => "https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip",
            Tool::YtDlp => "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe",
            Tool::Deno => "https://github.com/denoland/deno/releases/latest/download/deno-x86_64-pc-windows-msvc.zip",
        }
    }

    fn entry(self) -> Option<&'static str> {
        match self {
            Tool::Ffmpeg => Some("bin/ffmpeg.exe"),
            Tool::YtDlp => None,
            Tool::Deno => Some("deno.exe"),
        }
    }

    fn bundled(self, storage: &Storage) -> PathBuf {
        folder(storage).join(self.file())
    }

    fn system(self, system: &dyn Services) -> Option<PathBuf> {
        let paths = system.search_path()?;
        std::env::split_paths(&paths).map(|folder| folder.join(self.file())).find(|path| path.is_file())
    }

    pub fn locate(self, storage: &Storage, system: &dyn Services) -> Option<PathBuf> {
        let bundled = self.bundled(storage);
        if bundled.is_file() {
            return Some(bundled);
        }
        self.system(system)
    }

    pub fn presence(self, storage: &Storage, system: &dyn Services) -> Presence {
        if self.bundled(storage).is_file() {
            Presence::Bundled
        } else if self.system(system).is_some() {
            Presence::System
        } else {
            Presence::Missing
        }
    }

    pub fn download_label(self) -> String {
        fill(tr().tool_download, self.name())
    }

    pub fn require(self, storage: &Storage, system: &dyn Services) -> Result<PathBuf> {
        self.locate(storage, system).with_context(|| fill(tr().tool_missing, self.name()))
    }

    pub fn install(self, storage: &Storage, system: &dyn Services, progress: &Progress) -> Result<PathBuf> {
        static INSTALL: Mutex<()> = Mutex::new(());
        let _guard = INSTALL.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(path) = self.locate(storage, system) {
            return Ok(path);
        }
        let folder = folder(storage);
        fs::create_dir_all(&folder)?;
        let target = self.bundled(storage);
        let result = match self.entry() {
            Some(entry) => {
                let archive = target.with_extension("zip");
                let result = system
                    .download(self.url(), &archive, progress)
                    .and_then(|()| unpack(&archive, entry, &target, progress));
                let _ = fs::remove_file(&archive);
                result
            }
            None => {
                let temp = target.with_extension("tmp");
                let result =
                    system.download(self.url(), &temp, progress).and_then(|()| Ok(fs::rename(&temp, &target)?));
                let _ = fs::remove_file(&temp);
                result
            }
        };
        if result.is_err() {
            let _ = fs::remove_dir(&folder);
        }
        result.with_context(|| fill(tr().tool_failed, self.name()))?;
        Ok(target)
    }

    pub fn sweep(storage: &Storage) {
        let Ok(entries) = fs::read_dir(folder(storage)) else {
            return;
        };
        for path in entries.filter_map(|entry| entry.ok().map(|entry| entry.path())) {
            let partial = path.extension().is_some_and(|extension| extension == "zip" || extension == "tmp");
            if partial && fs::remove_file(&path).is_ok() {
                log::info!("Removed an unfinished download: {}", path.display());
            }
        }
        let _ = fs::remove_dir(folder(storage));
    }

    pub fn remove(self, storage: &Storage) -> io::Result<()> {
        fs::remove_file(self.bundled(storage))?;
        let _ = fs::remove_dir(folder(storage));
        Ok(())
    }
}

fn folder(storage: &Storage) -> PathBuf {
    storage.root().join("tools")
}

fn unpack(archive: &Path, entry: &str, target: &Path, progress: &Progress) -> Result<()> {
    let mut zip = ZipArchive::new(fs::File::open(archive)?)?;
    let name = zip
        .file_names()
        .filter_map(Result::ok)
        .find(|name| name.ends_with(entry))
        .map(|name| name.into_owned())
        .with_context(|| entry.to_string())?;
    let temp = target.with_extension("tmp");
    let copied = io::copy(&mut zip.by_name(&name)?, &mut fs::File::create(&temp)?);
    if copied.is_err() || progress.cancelled() {
        let _ = fs::remove_file(&temp);
        copied?;
        bail!(tr().operation_cancelled);
    }
    fs::rename(temp, target)?;
    Ok(())
}

#[cfg(test)]
mod tests;
