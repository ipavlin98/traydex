mod action;
mod compress;
mod convert;
mod files;
mod hash;
mod ocr;
mod pdf;
mod phone;
mod plugin;
mod rename;
mod tiles;
mod tools;
mod unpack;
mod upload;
mod video;
mod ytdlp;

pub use action::{Action, Algorithm, Compression, ENGINES, Field, Kind, Single, Slot, Upload, engine_name};
pub use hash::{Digest, Lookup, Verdict};
pub use phone::{qr_png, serve};
pub use plugin::{
    BUNDLE, Custom, Input, Plugin, Setting, install as install_plugin, scan as scan_plugins,
    uninstall as uninstall_plugin,
};
pub use rename::{Batch, Rule, apply as rename_all};
pub use tiles::{MAX_TILES, Tile, TileId, Tiles};
pub use tools::{Presence, Tool};
pub use unpack::{Unlocked, unlock};
pub use upload::{curl_exe, curl_failure};
pub use ytdlp::{Extra, Fetch, HEIGHTS, Media, Mode, Quality, download_media};

use self::compress::compress;
use self::convert::convert;
use self::files::{copy_paths, data_uri, launch, transfer, zip};
use self::hash::hashes;
use self::ocr::recognize;
use self::pdf::pdf;
use self::phone::{qr, share};
use self::unpack::unpack;
use self::upload::{Accepts, find_image, publish};
use self::video::gif;
use crate::i18n::{fill, tr};
use crate::store::Payload;
use crate::store::{self, Storage};
use crate::system::{Offer, Progress, Server, Services};
use anyhow::{Result, anyhow, bail};
use image::ImageFormat;
use std::cmp::Ordering as Order;
use std::path::{Path, PathBuf};
use std::{fs, io};

const NAME_LIMIT: usize = 40;
const IMGUR_LIMIT: u64 = 20 * 1024 * 1024;
pub const CONNECT_TIMEOUT: &str = "15";
const IMGUR_URL: &str = "https://api.imgur.com/3/image";
const QUERY_LIMIT: usize = 2000;
const QUERY_FILE_LIMIT: u64 = 64 * 1024;
pub const VIRUSTOTAL_JOIN: &str = "https://www.virustotal.com/gui/join-us";

fn textual(path: &Path) -> bool {
    matches!(extension(path).as_str(), "txt" | "url")
}

fn offered(offer: &Offer, admits: impl Fn(&Path, bool) -> bool) -> bool {
    match offer {
        Offer::Files(entries) => entries.iter().any(|entry| admits(&entry.path, entry.folder)),
        Offer::Text => admits(Path::new("text.txt"), false),
        Offer::Image => admits(Path::new("image.png"), false),
        Offer::Unknown => true,
    }
}

fn normalized(path: &Path) -> String {
    path.to_string_lossy().replace('/', "\\").trim_end_matches('\\').to_lowercase()
}

pub enum Show {
    Share(Server),
    Hashes(Vec<Digest>, Option<Lookup>),
    Qr(String),
    Rename(Batch),
    Media(Media),
    Password(Vec<PathBuf>),
}

#[derive(Debug, PartialEq)]
pub enum Pending {
    Share(Option<Vec<PathBuf>>),
    Hashes { files: Option<Vec<PathBuf>>, checking: bool },
    Qr,
    Rename(Vec<PathBuf>),
    Media(Fetch),
}

pub fn pending(action: &Action, payload: &Payload) -> Option<Pending> {
    if !action.ready() {
        return None;
    }
    let links = || match payload {
        Payload::Paths(paths) => paths.iter().any(|path| textual(path)),
        Payload::Text(_) => true,
        Payload::Stored(_) | Payload::Image(_) => false,
    };
    let files = |keep: fn(&Path) -> bool| match payload {
        Payload::Paths(paths) => {
            let files: Vec<PathBuf> = paths.iter().filter(|path| keep(path)).cloned().collect();
            (!files.is_empty()).then_some(Some(files))
        }
        Payload::Stored(_) | Payload::Text(_) | Payload::Image(_) => Some(None),
    };
    match &action.kind {
        Kind::Phone => files(Path::is_file).map(Pending::Share),
        Kind::Hash { virustotal, .. } => {
            files(Path::is_file).map(|files| Pending::Hashes { files, checking: !virustotal.trim().is_empty() })
        }
        Kind::Qr => links().then_some(Pending::Qr),
        Kind::Ytdlp(fetch) => links().then_some(Pending::Media(*fetch)),
        Kind::Rename => {
            let mut files = files(Path::exists)??;
            by_name(&mut files);
            Some(Pending::Rename(files))
        }
        Kind::Move { .. }
        | Kind::Copy { .. }
        | Kind::Zip
        | Kind::Jpeg
        | Kind::Png
        | Kind::Gif
        | Kind::CopyPath
        | Kind::Imgur { .. }
        | Kind::Upload(_)
        | Kind::Run { .. }
        | Kind::Search { .. }
        | Kind::DataUri
        | Kind::Ocr { .. }
        | Kind::Compress { .. }
        | Kind::Pdf
        | Kind::Unpack
        | Kind::Custom(_)
        | Kind::Cancel => None,
    }
}

pub struct Outcome {
    pub message: String,
    pub created: Vec<PathBuf>,
    pub clipboard: Option<String>,
    pub show: Option<Show>,
}

impl Outcome {
    fn note(message: String) -> Self {
        Self { message, created: Vec::new(), clipboard: None, show: None }
    }

    fn showing(show: Show) -> Self {
        Self { show: Some(show), ..Self::note(String::new()) }
    }
}

impl Progress {
    fn ensure_running(&self) -> io::Result<()> {
        if self.cancelled() { Err(io::Error::other(tr().stopped)) } else { Ok(()) }
    }

    fn guard<T>(&self, inner: T) -> Guarded<'_, T> {
        Guarded { inner, progress: self }
    }

    fn fraction(&self, done: u64, total: u64) {
        if total > 0 {
            self.update(|stage| stage.fraction = Some(done.min(total) as f32 / total as f32));
        }
    }

    fn step(&self, done: usize, total: usize) -> bool {
        if total > 1 {
            self.update(|stage| stage.fraction = Some(done as f32 / total as f32));
        }
        !self.cancelled()
    }
}

fn base(path: &Path) -> String {
    path.file_name().unwrap_or(path.as_os_str()).to_string_lossy().into_owned()
}

fn extension(path: &Path) -> String {
    path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
}

fn natural(a: &str, b: &str) -> Order {
    let (mut a, mut b) = (a.chars().peekable(), b.chars().peekable());
    loop {
        let (x, y) = match (a.peek(), b.peek()) {
            (None, None) => return Order::Equal,
            (None, _) => return Order::Less,
            (_, None) => return Order::Greater,
            (Some(&x), Some(&y)) => (x, y),
        };
        let order = if x.is_ascii_digit() && y.is_ascii_digit() {
            let number = |chars: &mut std::iter::Peekable<std::str::Chars>| {
                let mut digits = String::new();
                while let Some(c) = chars.next_if(char::is_ascii_digit) {
                    digits.push(c);
                }
                digits.trim_start_matches('0').to_string()
            };
            let (x, y) = (number(&mut a), number(&mut b));
            x.len().cmp(&y.len()).then_with(|| x.cmp(&y))
        } else {
            a.next();
            b.next();
            x.to_lowercase().cmp(y.to_lowercase())
        };
        if order != Order::Equal {
            return order;
        }
    }
}

fn by_name(files: &mut [PathBuf]) {
    files.sort_by(|a, b| natural(&base(a), &base(b)));
}

pub fn run(
    action: &Action,
    payload: Payload,
    progress: &Progress,
    storage: &Storage,
    system: &dyn Services,
) -> Result<Outcome> {
    if !action.ready() {
        bail!(tr().not_configured);
    }
    if action.needs_disk() && !matches!(payload, Payload::Paths(_)) {
        bail!(tr().needs_disk_files);
    }
    let (files, scratch) = inputs(payload, storage)?;
    let result = execute(action, &files, progress, storage, system);
    if progress.cancelled() {
        if let Ok(outcome) = &result {
            outcome.created.iter().for_each(|file| storage.discard(file));
        }
        if progress.expired() {
            bail!(fill(tr().plugin_timeout, action.timeout().unwrap_or_default().as_secs()));
        }
        bail!(tr().stopped);
    }
    if matches!(action.kind, Kind::Run { .. } | Kind::Phone) && !scratch.is_empty() {
        log::info!("Temporary inputs stay for the launched program or the phone until the next start");
        scratch.keep();
    }
    result
}

fn execute(
    action: &Action,
    files: &[PathBuf],
    progress: &Progress,
    storage: &Storage,
    system: &dyn Services,
) -> Result<Outcome> {
    match &action.kind {
        Kind::Move { folder } => transfer(files, folder, false, &action.title(), system),
        Kind::Copy { folder } => transfer(files, folder, true, &action.title(), system),
        Kind::Zip => zip(files, progress, storage),
        Kind::Jpeg => convert(files, ImageFormat::Jpeg, progress, storage),
        Kind::Png => convert(files, ImageFormat::Png, progress, storage),
        Kind::Gif => gif(files, progress, storage, system),
        Kind::CopyPath => copy_paths(files),
        Kind::Phone => share(files, system),
        Kind::Imgur { client_id } => {
            let upload = Upload {
                url: IMGUR_URL.into(),
                field: "image".into(),
                header: format!("Authorization: Client-ID {}", client_id.trim()),
                link: "/data/link".into(),
                ..Upload::default()
            };
            let accepts = Accepts { kind: store::is_image, limit: IMGUR_LIMIT, wrong: tr().needs_images };
            publish(files, progress, accepts, |file| upload.send(file, progress, system))
        }
        Kind::Upload(upload) => publish(files, progress, Accepts::files(), |file| upload.send(file, progress, system)),
        Kind::Run { program } => launch(program, files, &action.title(), system),
        Kind::Search { url } => search(url, files, progress, storage, system),
        Kind::DataUri => data_uri(files),
        Kind::Hash { algorithm, virustotal } => hashes(files, *algorithm, virustotal, progress),
        Kind::Ocr { language } => recognize(files, language, progress, system),
        Kind::Compress { format } => compress(files, *format, progress, storage),
        Kind::Pdf => pdf(files, progress, storage, system),
        Kind::Unpack => unpack(files, progress, storage),
        Kind::Rename => rename::prepare(files),
        Kind::Qr => qr(files),
        Kind::Ytdlp(_) => ytdlp::probe(files, progress, storage, system),
        Kind::Custom(custom) => plugin::run(custom, &action.title(), files, progress, storage, system),
        Kind::Cancel => Ok(Outcome::note(String::new())),
    }
}

fn encode(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => char::from(b).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

fn query_part(file: &Path, storage: &Storage) -> Result<String> {
    let note = storage.owned(file)
        && extension(file) == "txt"
        && fs::metadata(file).is_ok_and(|m| m.is_file() && m.len() <= QUERY_FILE_LIMIT);
    if note {
        return Ok(String::from_utf8_lossy(&fs::read(file)?).into_owned());
    }
    let name = if file.is_dir() { base(file) } else { stem(file) };
    Ok(name.replace(['_', '.'], " "))
}

fn search(
    engine: &str,
    files: &[PathBuf],
    progress: &Progress,
    storage: &Storage,
    system: &dyn Services,
) -> Result<Outcome> {
    let url = match files.iter().find(|file| file.is_file() && store::is_image(file)) {
        Some(image) => find_image(engine, image, progress, system)?,
        None => {
            let parts = files.iter().map(|file| query_part(file, storage)).collect::<Result<Vec<_>>>()?;
            let text = parts.join(" ");
            let words: Vec<&str> = text.split_whitespace().collect();
            let query: String = words.join(" ").chars().take(QUERY_LIMIT).collect();
            if query.is_empty() {
                bail!(tr().needs_text);
            }
            let engine = engine.trim();
            if engine.contains("%s") {
                engine.replace("%s", &encode(&query))
            } else {
                format!("{engine}{}", encode(&query))
            }
        }
    };
    system.shell_open(&url);
    Ok(Outcome::note(String::new()))
}

fn readable_name(text: &str, fallback: &str) -> String {
    let line = text.lines().find(|line| !line.trim().is_empty()).unwrap_or_default();
    let name: String =
        line.chars().filter(|c| !c.is_control() && !r#"\/:*?"<>|"#.contains(*c)).take(NAME_LIMIT).collect();
    let name = name.trim().trim_end_matches(['.', ' ']);
    if name.is_empty() { fallback.to_string() } else { name.to_string() }
}

fn inputs(payload: Payload, storage: &Storage) -> Result<(Vec<PathBuf>, store::Scratch)> {
    let mut scratch = store::Scratch::new(storage.clone());
    let files = match payload {
        Payload::Paths(paths) => paths,
        Payload::Stored(stored) => {
            let files = stored.paths().to_vec();
            scratch = stored;
            files
        }
        Payload::Image(data) => {
            let image = store::decode(data)?;
            let path = storage.allocate(&format!("{}.png", readable_name(&store::title(&image), "image")))?;
            scratch.push(path.clone());
            store::write_png(storage, &image, &path)?;
            vec![path]
        }
        Payload::Text(text) => {
            let path = storage.allocate(&format!("{}.txt", readable_name(&text, tr().tab_text)))?;
            scratch.push(path.clone());
            fs::write(&path, &text)?;
            vec![path]
        }
    };
    if files.is_empty() {
        bail!(tr().unsupported);
    }
    Ok((files, scratch))
}

fn stem(path: &Path) -> String {
    path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| base(path))
}

fn output(source: &Path, name: &str, storage: &Storage) -> Result<PathBuf> {
    if let Some(folder) = source.parent().filter(|_| !storage.owned(source)) {
        let target = store::vacant(folder.join(name));
        let writable = fs::OpenOptions::new().write(true).create_new(true).open(&target).is_ok();
        if writable {
            return Ok(target);
        }
        log::info!("{} isn't writable, saving to Traydex storage", folder.display());
    }
    Ok(storage.allocate(name)?)
}

struct Guarded<'a, T> {
    inner: T,
    progress: &'a Progress,
}

impl<T: io::Read> io::Read for Guarded<'_, T> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.progress.ensure_running()?;
        self.inner.read(buf)
    }
}

impl<T: io::Write> io::Write for Guarded<'_, T> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.progress.ensure_running()?;
        self.inner.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

impl<T: io::Seek> io::Seek for Guarded<'_, T> {
    fn seek(&mut self, position: io::SeekFrom) -> io::Result<u64> {
        self.inner.seek(position)
    }
}

#[derive(Default)]
struct Report {
    created: Vec<PathBuf>,
    skipped: usize,
    error: Option<anyhow::Error>,
}

impl Report {
    fn fail(&mut self, file: &Path, error: anyhow::Error) {
        log::warn!("{}: {error:#}", file.display());
        self.skipped += 1;
        self.error.get_or_insert(error.context(base(file)));
    }

    fn finish(self, wrong: String) -> Result<Outcome> {
        if self.created.is_empty() {
            return Err(self.error.unwrap_or_else(|| anyhow!(wrong)));
        }
        let mut message = fill(tr().converted, self.created.len());
        if self.skipped > 0 {
            message = format!("{message} · {}", fill(tr().skipped, self.skipped));
        }
        Ok(Outcome { message, created: self.created, clipboard: None, show: None })
    }
}

fn last_line(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    text.split(['\r', '\n']).rev().find(|line| !line.trim().is_empty()).unwrap_or_default().trim().to_string()
}

#[cfg(test)]
mod tests;
