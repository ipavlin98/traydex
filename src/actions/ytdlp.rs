use super::{Outcome, Progress, Show, Tool, by_name, extension, last_line, readable_name, textual};
use crate::i18n::{self, fill, tr};
use crate::store::{self, Storage};
use crate::system::{AppData, Services};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

pub const HEIGHTS: [u32; 6] = [2160, 1440, 1080, 720, 480, 360];
const LINK_FILE_LIMIT: u64 = 64 * 1024;
const PROGRESS_MARK: &str = "[traydex]";
const PROGRESS_TEMPLATE: &str = "download:[traydex] %(progress.downloaded_bytes)s \
    %(progress.total_bytes,progress.total_bytes_estimate)s %(info.playlist_index)s %(info.n_entries)s";
const NAME_TEMPLATE: &str = "%(playlist_index&{} - |)s%(title).150B.%(ext)s";
const PREFERRED: &str = "ext:mp4:m4a";
const CONTAINERS: &str = "mp4/mkv";
const AUDIO_FORMAT: &str = "mp3";
const SUBTITLE_FORMAT: &str = "srt";
const SPONSORS: &str = "sponsor";
const LOGIN_HINT: &str = "--cookies";
const BOT_HINT: &str = "not a bot";
const ERROR_PREFIX: &str = "ERROR:";
const LIVE_CHAT: &str = "live_chat";
const BROWSERS: [(&str, AppData, &str); 7] = [
    ("firefox", AppData::Roaming, r"Mozilla\Firefox\Profiles"),
    ("edge", AppData::Local, r"Microsoft\Edge\User Data"),
    ("chrome", AppData::Local, r"Google\Chrome\User Data"),
    ("brave", AppData::Local, r"BraveSoftware\Brave-Browser\User Data"),
    ("vivaldi", AppData::Local, r"Vivaldi\User Data"),
    ("opera", AppData::Roaming, r"Opera Software\Opera Stable"),
    ("chromium", AppData::Local, r"Chromium\User Data"),
];

#[derive(Clone, Copy, Default, PartialEq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    #[default]
    Video,
    Audio,
}

#[derive(Clone, Copy, Default, PartialEq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Quality {
    #[default]
    Best,
    Height(u32),
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Extra {
    Playlist,
    Subtitles,
    Cover,
    Sponsors,
}

impl Extra {
    pub const ALL: [Extra; 4] = [Extra::Playlist, Extra::Subtitles, Extra::Cover, Extra::Sponsors];

    pub fn title(self) -> &'static str {
        match self {
            Extra::Playlist => tr().media_playlist,
            Extra::Subtitles => tr().media_subtitles,
            Extra::Cover => tr().media_cover,
            Extra::Sponsors => tr().media_sponsors,
        }
    }
}

#[derive(Clone, Copy, Default, PartialEq, Debug, Serialize, Deserialize)]
pub struct Fetch {
    #[serde(default)]
    pub mode: Mode,
    #[serde(default)]
    pub quality: Quality,
    #[serde(default)]
    pub subtitles: bool,
    #[serde(default)]
    pub cover: bool,
    #[serde(default)]
    pub sponsors: bool,
    #[serde(skip)]
    pub playlist: bool,
}

impl Fetch {
    pub fn get(&self, extra: Extra) -> bool {
        match extra {
            Extra::Playlist => self.playlist,
            Extra::Subtitles => self.subtitles,
            Extra::Cover => self.cover,
            Extra::Sponsors => self.sponsors,
        }
    }

    pub fn set(&mut self, extra: Extra, on: bool) {
        let flag = match extra {
            Extra::Playlist => &mut self.playlist,
            Extra::Subtitles => &mut self.subtitles,
            Extra::Cover => &mut self.cover,
            Extra::Sponsors => &mut self.sponsors,
        };
        *flag = on;
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Media {
    pub url: String,
    pub title: String,
    entries: Option<usize>,
    in_playlist: bool,
    top: Option<u32>,
    subtitles: Vec<String>,
    youtube: bool,
    browser: Option<&'static str>,
}

impl Media {
    pub fn detail(&self) -> String {
        match self.entries {
            Some(count) => format!("{} · {}", store::host(&self.url), fill(tr().media_entries, count)),
            None => store::host(&self.url).to_string(),
        }
    }

    pub fn allows(&self, height: u32) -> bool {
        self.top.is_none_or(|top| top >= height)
    }

    pub fn quality(&self, fetch: &Fetch) -> Quality {
        match fetch.quality {
            Quality::Height(height) if self.allows(height) => Quality::Height(height),
            Quality::Best | Quality::Height(_) => Quality::Best,
        }
    }

    pub fn offers(&self, extra: Extra) -> bool {
        match extra {
            Extra::Playlist => self.in_playlist,
            Extra::Subtitles => !self.subtitles.is_empty(),
            Extra::Cover => true,
            Extra::Sponsors => self.youtube,
        }
    }

    pub fn applies(&self, fetch: &Fetch, extra: Extra) -> bool {
        (extra == Extra::Playlist && self.entries.is_some()) || (self.offers(extra) && fetch.get(extra))
    }

    pub fn hint(&self, extra: Extra) -> String {
        match extra {
            Extra::Playlist => match self.entries {
                Some(count) => fill(tr().media_entries, count),
                None if self.in_playlist => tr().media_in_playlist.into(),
                None => tr().media_single.into(),
            },
            Extra::Subtitles if self.subtitles.is_empty() => tr().media_no_subtitles.into(),
            Extra::Subtitles => self.subtitles.join(", "),
            Extra::Cover => tr().media_cover_hint.into(),
            Extra::Sponsors => tr().media_sponsors_hint.into(),
        }
    }
}

struct Kit {
    ytdlp: PathBuf,
    ffmpeg: PathBuf,
    deno: PathBuf,
}

impl Kit {
    fn require(storage: &Storage, system: &dyn Services) -> Result<Self> {
        Ok(Self {
            ytdlp: Tool::YtDlp.require(storage, system)?,
            ffmpeg: Tool::Ffmpeg.require(storage, system)?,
            deno: Tool::Deno.require(storage, system)?,
        })
    }

    fn command(&self, browser: Option<&str>) -> Command {
        let mut runtime = OsString::from("deno:");
        runtime.push(&self.deno);
        let mut command = Command::new(&self.ytdlp);
        command
            .env("PYTHONUTF8", "1")
            .env("PYTHONIOENCODING", "utf-8")
            .arg("--ignore-config")
            .arg("--ffmpeg-location")
            .arg(&self.ffmpeg)
            .arg("--js-runtimes")
            .arg(runtime);
        if let Some(browser) = browser {
            command.args(["--cookies-from-browser", browser]);
        }
        command
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Barrier {
    Bot,
    Account,
}

enum Attempt<T> {
    Done(T),
    Login(Barrier),
    Failed(String),
}

fn attempt<T>(output: &Output) -> Attempt<T> {
    let log = String::from_utf8_lossy(&output.stderr);
    if log.contains(BOT_HINT) {
        Attempt::Login(Barrier::Bot)
    } else if log.contains(LOGIN_HINT) {
        Attempt::Login(Barrier::Account)
    } else {
        Attempt::Failed(failure(&output.stderr))
    }
}

fn failure(stderr: &[u8]) -> String {
    let text = String::from_utf8_lossy(stderr);
    let error = text.lines().rev().find_map(|line| line.trim().strip_prefix(ERROR_PREFIX)).map(str::trim);
    let message = error.map(str::to_string).unwrap_or_else(|| last_line(stderr));
    if message.is_empty() { tr().unsupported.into() } else { message }
}

fn browsers(system: &dyn Services) -> Vec<&'static str> {
    BROWSERS
        .iter()
        .filter(|(_, place, folder)| system.app_data(*place).is_some_and(|root| root.join(folder).is_dir()))
        .map(|(name, _, _)| *name)
        .collect()
}

fn signed_in<T>(
    url: &str,
    known: Option<&'static str>,
    system: &dyn Services,
    mut run: impl FnMut(Option<&'static str>) -> Result<Attempt<T>>,
) -> Result<(T, Option<&'static str>)> {
    let candidates: Vec<Option<&'static str>> = match known {
        Some(browser) => vec![Some(browser)],
        None => std::iter::once(None).chain(browsers(system).into_iter().map(Some)).collect(),
    };
    let (mut signed_out, mut barrier) = (false, Barrier::Account);
    for browser in candidates {
        match (run(browser)?, browser) {
            (Attempt::Done(value), _) => return Ok((value, browser)),
            (Attempt::Login(found), Some(_)) => {
                signed_out = true;
                if found == Barrier::Bot {
                    barrier = found;
                }
            }
            (Attempt::Login(found), None) => {
                barrier = found;
                log::info!("{url} needs a sign-in, trying the browser cookies");
            }
            (Attempt::Failed(message), None) => bail!(message),
            (Attempt::Failed(message), Some(name)) => log::warn!("The {name} cookies didn't work: {message}"),
        }
    }
    let template = match (barrier, signed_out) {
        (Barrier::Bot, _) => tr().ytdlp_bot,
        (Barrier::Account, true) => tr().ytdlp_sign_in,
        (Barrier::Account, false) => tr().ytdlp_no_cookies,
    };
    bail!(fill(template, store::host(url)))
}

fn find_link(text: &str) -> Option<String> {
    text.split(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | '<' | '>'))
        .map(|word| word.trim_start_matches("URL="))
        .find(|word| word.starts_with("https://") || word.starts_with("http://"))
        .map(str::to_string)
}

fn link(files: &[PathBuf]) -> Result<String> {
    for file in files {
        let readable =
            textual(file) && fs::metadata(file).is_ok_and(|meta| meta.is_file() && meta.len() <= LINK_FILE_LIMIT);
        if readable && let Some(url) = find_link(&String::from_utf8_lossy(&fs::read(file)?)) {
            return Ok(url);
        }
    }
    bail!(tr().ytdlp_needs_link)
}

fn has_list(url: &str) -> bool {
    let query = url.split_once('?').map_or("", |(_, rest)| rest.split('#').next().unwrap_or_default());
    query.split('&').any(|pair| pair.split('=').next() == Some("list"))
}

fn wanted() -> Vec<&'static str> {
    let own = i18n::current().code();
    if own == "en" { vec![own] } else { vec![own, "en"] }
}

fn pick(manual: &[String], automatic: &[String]) -> Vec<String> {
    let exact = |keys: &[String], lang: &str| keys.iter().find(|key| *key == lang).cloned();
    let close = |keys: &[String], lang: &str| keys.iter().find(|key| key.starts_with(&format!("{lang}-"))).cloned();
    let found: Vec<String> = wanted()
        .into_iter()
        .filter_map(|lang| {
            exact(manual, lang)
                .or_else(|| close(manual, lang))
                .or_else(|| exact(automatic, lang))
                .or_else(|| close(automatic, lang))
        })
        .collect();
    if !found.is_empty() {
        return found;
    }
    manual.iter().find(|key| *key != LIVE_CHAT).cloned().into_iter().collect()
}

fn parse_info(json: &[u8], url: &str, browser: Option<&'static str>) -> Result<Media> {
    let info: Value = serde_json::from_slice(json).context("yt-dlp")?;
    let text = |key: &str| info.get(key).and_then(Value::as_str);
    let keys = |key: &str| -> Vec<String> {
        info.get(key).and_then(Value::as_object).map(|map| map.keys().cloned().collect()).unwrap_or_default()
    };
    let playlist = text("_type") == Some("playlist");
    let entries = playlist.then(|| {
        let listed = info.get("entries").and_then(Value::as_array).map_or(0, Vec::len);
        info.get("playlist_count").and_then(Value::as_u64).map_or(listed, |count| count as usize)
    });
    let top = info.get("formats").and_then(Value::as_array).and_then(|formats| {
        formats
            .iter()
            .filter(|format| format.get("vcodec").and_then(Value::as_str) != Some("none"))
            .filter_map(|format| format.get("height")?.as_u64())
            .max()
            .map(|height| height as u32)
    });
    let subtitles = if playlist {
        wanted().into_iter().map(String::from).collect()
    } else {
        pick(&keys("subtitles"), &keys("automatic_captions"))
    };
    Ok(Media {
        url: url.to_string(),
        title: text("title").filter(|title| !title.trim().is_empty()).unwrap_or(url).to_string(),
        entries,
        in_playlist: !playlist && has_list(url),
        top,
        subtitles,
        youtube: text("extractor_key").is_some_and(|key| key.starts_with("Youtube")),
        browser,
    })
}

pub(super) fn probe(
    files: &[PathBuf],
    progress: &Progress,
    storage: &Storage,
    system: &dyn Services,
) -> Result<Outcome> {
    let url = link(files)?;
    let kit = Kit::require(storage, system)?;
    let (media, _) = signed_in(&url, None, system, |browser| {
        let mut command = kit.command(browser);
        command.args(["-J", "--flat-playlist", "--no-playlist"]).arg(&url);
        let output = system.output(&mut command, None, progress)?;
        Ok(if output.status.success() {
            Attempt::Done(parse_info(&output.stdout, &url, browser)?)
        } else {
            attempt(&output)
        })
    })?;
    Ok(Outcome::showing(Show::Media(media)))
}

fn arguments(media: &Media, fetch: &Fetch) -> Vec<String> {
    let mut args = vec!["--newline", "--no-mtime", "--progress-template", PROGRESS_TEMPLATE, "-o", NAME_TEMPLATE];
    if media.applies(fetch, Extra::Playlist) {
        args.extend(["--yes-playlist", "--ignore-errors"]);
    } else {
        args.push("--no-playlist");
    }
    let mut args: Vec<String> = args.into_iter().map(String::from).collect();
    match fetch.mode {
        Mode::Video => {
            let sort = match media.quality(fetch) {
                Quality::Best => format!("res,{PREFERRED}"),
                Quality::Height(height) => format!("res:{height},{PREFERRED}"),
            };
            args.extend(["-S".into(), sort, "--merge-output-format".into(), CONTAINERS.into()]);
        }
        Mode::Audio => {
            args.extend(["-f", "ba/b", "-x", "--audio-format", AUDIO_FORMAT, "--audio-quality", "0"].map(String::from))
        }
    }
    if media.applies(fetch, Extra::Subtitles) {
        args.extend(["--write-subs", "--write-auto-subs", "--sub-langs"].map(String::from));
        args.push(media.subtitles.join(","));
        args.extend(["--convert-subs", SUBTITLE_FORMAT].map(String::from));
    }
    if media.applies(fetch, Extra::Cover) {
        args.extend(["--embed-thumbnail", "--embed-metadata"].map(String::from));
    }
    if media.applies(fetch, Extra::Sponsors) {
        args.extend(["--sponsorblock-remove", SPONSORS].map(String::from));
    }
    args
}

fn fraction(line: &str) -> Option<f32> {
    let mut numbers = line.strip_prefix(PROGRESS_MARK)?.split_whitespace().map(|part| part.parse::<f64>().ok());
    let (done, total) = (numbers.next()??, numbers.next()??);
    if total <= 0. {
        return None;
    }
    let part = (done / total).clamp(0., 1.);
    let overall = match (numbers.next().flatten(), numbers.next().flatten()) {
        (Some(index), Some(count)) if index >= 1. && count >= index => (index - 1. + part) / count,
        _ => part,
    };
    Some(overall as f32)
}

fn finished(folder: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in fs::read_dir(folder)? {
        let path = entry?.path();
        let kind = extension(&path);
        if path.is_file() && !kind.starts_with("part") && kind != "ytdl" {
            files.push(path);
        }
    }
    Ok(files)
}

pub fn download_media(
    media: &Media,
    fetch: &Fetch,
    progress: &Progress,
    storage: &Storage,
    system: &dyn Services,
) -> Result<Outcome> {
    let kit = Kit::require(storage, system)?;
    let folder = storage.allocate(&readable_name(&media.title, "media"))?;
    let args = arguments(media, fetch);
    let saved = fs::create_dir_all(&folder).map_err(anyhow::Error::from).and_then(|()| {
        signed_in(&media.url, media.browser, system, |browser| {
            let mut reached = 0f32;
            let mut command = kit.command(browser);
            command.args(&args).arg("-P").arg(&folder).arg(&media.url);
            let output = system.stream(&mut command, None, progress, &mut |line| {
                if let Some(fraction) = fraction(line) {
                    reached = reached.max(fraction);
                    progress.update(|stage| stage.fraction = Some(reached));
                }
            })?;
            let files = finished(&folder)?;
            Ok(if files.is_empty() { attempt(&output) } else { Attempt::Done(files) })
        })
    });
    let mut files = match saved {
        Ok((files, _)) => files,
        Err(error) => {
            storage.release(&folder);
            return Err(error);
        }
    };
    by_name(&mut files);
    Ok(Outcome { message: fill(tr().converted, files.len()), created: files, clipboard: None, show: None })
}

#[cfg(test)]
mod tests;
