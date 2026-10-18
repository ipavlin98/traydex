use crate::i18n::{fill, tr};
use crate::os;
use crate::store::{Item, Kind, human_size};
use crate::system::{Handler, Services};
use gpui::{RenderImage, SharedString};
use std::fs::{self, File};
use std::io::Read;
use std::path::Path;
use std::sync::Arc;

const TEXT_LIMIT: usize = 2 << 20;
const SNIFF: usize = 8000;
const LINE_CHUNK: usize = 1000;
const ENTRY_LIMIT: usize = 5000;
const THUMBNAIL: i32 = 768;
pub(crate) const TEXT: &[&str] = &[
    "txt",
    "md",
    "markdown",
    "log",
    "csv",
    "tsv",
    "json",
    "jsonc",
    "xml",
    "yaml",
    "yml",
    "toml",
    "ini",
    "cfg",
    "conf",
    "env",
    "properties",
    "css",
    "scss",
    "less",
    "js",
    "mjs",
    "cjs",
    "ts",
    "tsx",
    "jsx",
    "vue",
    "svelte",
    "rs",
    "py",
    "rb",
    "go",
    "java",
    "kt",
    "kts",
    "swift",
    "c",
    "h",
    "cpp",
    "hpp",
    "cc",
    "cs",
    "fs",
    "php",
    "pl",
    "lua",
    "r",
    "sql",
    "sh",
    "bash",
    "zsh",
    "ps1",
    "psm1",
    "bat",
    "cmd",
    "gradle",
    "cmake",
    "make",
    "dockerfile",
    "gitignore",
    "gitattributes",
    "editorconfig",
    "srt",
    "vtt",
    "tex",
    "rst",
    "lock",
];
const ARCHIVES: &[&str] = &["zip", "jar", "apk", "nupkg", "vsix", "whl", "xpi"];

pub struct Line {
    pub number: Option<usize>,
    pub text: SharedString,
}

pub struct Entry {
    pub name: SharedString,
    pub folder: bool,
    pub size: Option<u64>,
}

pub enum Content {
    Image,
    Text { lines: Arc<[Line]>, mono: bool },
    Entries(Arc<[Entry]>),
    Shell(Handler),
    Thumbnail(Option<Arc<RenderImage>>),
    Missing,
    Failed,
}

pub struct Loaded {
    pub content: Content,
    pub detail: String,
    pub partial: bool,
}

impl Loaded {
    fn new(content: Content, facts: impl IntoIterator<Item = String>) -> Self {
        let facts: Vec<String> = facts.into_iter().filter(|fact| !fact.is_empty()).collect();
        Self { content, detail: facts.join(" · "), partial: false }
    }

    pub fn failed() -> Self {
        Self::new(Content::Failed, [])
    }

    fn partial(mut self, partial: bool) -> Self {
        self.partial = partial;
        self
    }

    pub fn count(&self) -> usize {
        match &self.content {
            Content::Text { lines, .. } => lines.len(),
            _ => 0,
        }
    }
}

pub fn load(item: &Item, system: &dyn Services) -> Loaded {
    let meta = item.describe();
    if !meta.exists {
        return Loaded::new(Content::Missing, []);
    }
    match item.kind {
        Kind::Text | Kind::Link | Kind::Note => {
            let (lines, _) = split(item.text.as_deref().unwrap_or_default().trim(), false);
            Loaded::new(Content::Text { lines, mono: false }, [meta.detail])
        }
        Kind::Image => {
            let dimensions = image::image_dimensions(&item.path)
                .map(|(width, height)| format!("{width} × {height}"))
                .unwrap_or_default();
            Loaded::new(Content::Image, [meta.detail, dimensions])
        }
        Kind::Folder => match folder(&item.path) {
            Some((entries, total)) => {
                let partial = total > entries.len();
                Loaded::new(Content::Entries(entries), [fill(tr().preview_items, total)]).partial(partial)
            }
            None => Loaded::new(Content::Failed, [meta.detail]),
        },
        Kind::File => file(&item.path, meta.detail, system),
    }
}

fn file(path: &Path, detail: String, system: &dyn Services) -> Loaded {
    let extension = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    if extension == "svg" {
        return Loaded::new(Content::Image, [detail]);
    }
    if TEXT.contains(&extension.as_str())
        && let Some(loaded) = text(path, &detail)
    {
        return loaded;
    }
    if ARCHIVES.contains(&extension.as_str())
        && let Some((entries, total)) = archive(path)
    {
        let partial = total > entries.len();
        return Loaded::new(Content::Entries(entries), [detail, fill(tr().preview_items, total)]).partial(partial);
    }
    let described = || {
        let size = fs::metadata(path).map(|m| human_size(m.len())).unwrap_or_default();
        [system.type_name(path).unwrap_or_else(|| detail.clone()), size]
    };
    if let Some(clsid) = system.preview_handler(path) {
        return Loaded::new(Content::Shell(clsid), described());
    }
    if let Some(loaded) = text(path, &detail) {
        return loaded;
    }
    Loaded::new(Content::Thumbnail(thumbnail(path, system)), described())
}

pub fn fallback(path: &Path, detail: String, system: &dyn Services) -> Loaded {
    Loaded::new(Content::Thumbnail(thumbnail(path, system)), [detail])
}

fn thumbnail(path: &Path, system: &dyn Services) -> Option<Arc<RenderImage>> {
    let (width, height, pixels) = system.thumbnail(path, THUMBNAIL)?;
    let buffer = image::RgbaImage::from_raw(width, height, pixels)?;
    Some(Arc::new(RenderImage::new(vec![image::Frame::new(buffer)])))
}

fn text(path: &Path, detail: &str) -> Option<Loaded> {
    let (text, partial) = read_text(path)?;
    let (lines, count) = split(&text, true);
    Some(
        Loaded::new(Content::Text { lines, mono: true }, [detail.to_owned(), fill(tr().preview_lines, count)])
            .partial(partial),
    )
}

pub(crate) fn read_text(path: &Path) -> Option<(String, bool)> {
    let mut bytes = Vec::new();
    File::open(path).ok()?.take(TEXT_LIMIT as u64 + 1).read_to_end(&mut bytes).ok()?;
    let partial = bytes.len() > TEXT_LIMIT;
    bytes.truncate(TEXT_LIMIT);
    if let Some(rest) = bytes.strip_prefix(b"\xEF\xBB\xBF") {
        return Some((String::from_utf8_lossy(rest).into_owned(), partial));
    }
    if let Some(rest) = bytes.strip_prefix(b"\xFF\xFE") {
        return Some((utf16(rest, u16::from_le_bytes), partial));
    }
    if let Some(rest) = bytes.strip_prefix(b"\xFE\xFF") {
        return Some((utf16(rest, u16::from_be_bytes), partial));
    }
    if binary(&bytes[..bytes.len().min(SNIFF)]) {
        return None;
    }
    let text = match std::str::from_utf8(&bytes) {
        Ok(text) => text.to_owned(),
        Err(error) if error.error_len().is_none() => {
            String::from_utf8_lossy(&bytes[..error.valid_up_to()]).into_owned()
        }
        Err(_) => os::ansi_string(&bytes),
    };
    Some((text, partial))
}

fn utf16(bytes: &[u8], word: fn([u8; 2]) -> u16) -> String {
    let words: Vec<u16> = bytes.as_chunks::<2>().0.iter().map(|pair| word([pair[0], pair[1]])).collect();
    String::from_utf16_lossy(&words)
}

fn binary(sample: &[u8]) -> bool {
    let control = sample.iter().filter(|b| **b < 0x20 && !matches!(**b, b'\t' | b'\n' | b'\r' | 0x0c | 0x1b)).count();
    sample.contains(&0) || control * 100 > sample.len()
}

fn split(text: &str, numbered: bool) -> (Arc<[Line]>, usize) {
    let mut lines = Vec::new();
    let mut count = 0;
    for (index, line) in text.lines().enumerate() {
        count += 1;
        let line = line.replace('\t', "    ");
        let mut rest = line.as_str();
        let mut number = numbered.then_some(index + 1);
        loop {
            let cut = rest.char_indices().nth(LINE_CHUNK).map_or(rest.len(), |(at, _)| at);
            let (head, tail) = rest.split_at_checked(cut).unwrap_or((rest, ""));
            lines.push(Line { number: number.take(), text: head.to_owned().into() });
            rest = tail;
            if rest.is_empty() {
                break;
            }
        }
    }
    if text.trim().is_empty() {
        lines.clear();
    }
    (lines.into(), count)
}

fn folder(path: &Path) -> Option<(Arc<[Entry]>, usize)> {
    let mut entries = Vec::new();
    let mut total = 0;
    for entry in fs::read_dir(path).ok()?.flatten() {
        total += 1;
        if entries.len() >= ENTRY_LIMIT {
            continue;
        }
        let meta = entry.metadata().ok();
        let folder = meta.as_ref().is_some_and(|m| m.is_dir());
        entries.push(Entry {
            name: entry.file_name().to_string_lossy().into_owned().into(),
            folder,
            size: meta.filter(|_| !folder).map(|m| m.len()),
        });
    }
    entries.sort_by(|a, b| b.folder.cmp(&a.folder).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    Some((entries.into(), total))
}

fn archive(path: &Path) -> Option<(Arc<[Entry]>, usize)> {
    let mut archive = zip::ZipArchive::new(File::open(path).ok()?).ok()?;
    let total = archive.len();
    let mut entries = Vec::new();
    for index in 0..total.min(ENTRY_LIMIT) {
        let Ok(file) = archive.by_index_raw(index) else {
            continue;
        };
        let folder = file.is_dir();
        let name = file.name().map(|n| n.trim_end_matches('/').to_owned()).unwrap_or_default();
        entries.push(Entry { name: name.into(), folder, size: (!folder).then(|| file.size()) });
    }
    Some((entries.into(), total))
}

#[cfg(test)]
mod tests;
