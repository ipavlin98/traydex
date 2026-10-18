use super::{Outcome, Progress, Report, Tool, extension, last_line, output, stem};
use crate::i18n::tr;
use crate::store::Storage;
use crate::system::Services;
use anyhow::{Result, bail};
use std::path::{Path, PathBuf};
use std::process::Command;

const VIDEO: [&str; 11] = ["mp4", "mov", "mkv", "webm", "avi", "m4v", "wmv", "flv", "mpg", "mpeg", "3gp"];
const GIF_FILTER: &str = "fps=12,scale='min(640,iw)':-1:flags=lanczos,split[a][b];[a]palettegen[p];[b][p]paletteuse";

pub(super) fn is_video(path: &Path) -> bool {
    VIDEO.contains(&extension(path).as_str())
}

pub(super) fn gif(files: &[PathBuf], progress: &Progress, storage: &Storage, system: &dyn Services) -> Result<Outcome> {
    let videos: Vec<&PathBuf> = files.iter().filter(|file| file.is_file() && is_video(file)).collect();
    if videos.is_empty() {
        bail!(tr().needs_video);
    }
    let ffmpeg = Tool::Ffmpeg.require(storage, system)?;
    let mut report = Report { skipped: files.len() - videos.len(), ..Report::default() };
    for (done, file) in videos.iter().enumerate() {
        if !progress.step(done, videos.len()) {
            break;
        }
        match gif_one(&ffmpeg, file, progress, storage, system) {
            Ok(target) => report.created.push(target),
            Err(error) => report.fail(file, error),
        }
    }
    report.finish(tr().needs_video.to_string())
}

fn gif_one(
    ffmpeg: &Path,
    file: &Path,
    progress: &Progress,
    storage: &Storage,
    system: &dyn Services,
) -> Result<PathBuf> {
    let target = output(file, &format!("{}.gif", stem(file)), storage)?;
    let mut command = Command::new(ffmpeg);
    command.args(["-v", "error", "-y", "-i"]).arg(file).args(["-vf", GIF_FILTER, "-loop", "0"]).arg(&target);
    match system.output(&mut command, None, progress) {
        Ok(done) if done.status.success() => Ok(target),
        Ok(done) => {
            storage.discard(&target);
            bail!(last_line(&done.stderr))
        }
        Err(error) => {
            storage.discard(&target);
            Err(error.into())
        }
    }
}
