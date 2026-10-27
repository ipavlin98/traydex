use super::folded;
use crate::preview::{TEXT, read_text};
use crate::store::{Item, ItemId, Kind};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

const DOCUMENTS: &[&str] = &["docx", "pptx", "xlsx", "odt", "ods", "odp"];
const MARKUP_LIMIT: u64 = 16 << 20;

#[derive(Clone, Copy, PartialEq)]
enum Stamp {
    Text(u64),
    File { len: u64, modified: Option<SystemTime> },
}

enum Source {
    Text(String),
    File(PathBuf),
}

impl Source {
    fn of(item: &Item) -> Option<Self> {
        match item.kind {
            Kind::Text | Kind::Link | Kind::Note => item.shown_text().map(|text| Source::Text(text.into_owned())),
            Kind::File => Some(Source::File(item.path.clone())),
            Kind::Folder | Kind::Image => None,
        }
    }

    fn stamp(&self) -> Stamp {
        match self {
            Source::Text(text) => {
                let mut hasher = DefaultHasher::new();
                text.hash(&mut hasher);
                Stamp::Text(hasher.finish())
            }
            Source::File(path) => match fs::metadata(path) {
                Ok(metadata) => Stamp::File { len: metadata.len(), modified: metadata.modified().ok() },
                Err(_) => Stamp::File { len: 0, modified: None },
            },
        }
    }

    fn read(self) -> String {
        match self {
            Source::Text(text) => folded(&text),
            Source::File(path) => extract(&path).map(|text| folded(&text)).unwrap_or_default(),
        }
    }
}

fn extract(path: &Path) -> Option<String> {
    let extension = path.extension()?.to_string_lossy().to_lowercase();
    if TEXT.contains(&extension.as_str()) {
        return read_text(path).map(|(text, _)| text);
    }
    if DOCUMENTS.contains(&extension.as_str()) {
        return document(path, &extension);
    }
    None
}

fn document(path: &Path, extension: &str) -> Option<String> {
    let mut archive = zip::ZipArchive::new(File::open(path).ok()?).ok()?;
    let mut text = String::new();
    for index in 0..archive.len() {
        let Ok(entry) = archive.by_index(index) else {
            continue;
        };
        if !entry.name().is_ok_and(|name| part(extension, &name)) {
            continue;
        }
        let mut xml = Vec::new();
        if entry.take(MARKUP_LIMIT).read_to_end(&mut xml).is_ok() {
            markup(&String::from_utf8_lossy(&xml), &mut text);
        }
    }
    Some(text)
}

fn part(extension: &str, name: &str) -> bool {
    match extension {
        "docx" => name == "word/document.xml",
        "pptx" => name.starts_with("ppt/slides/slide") && name.ends_with(".xml"),
        "xlsx" => name == "xl/sharedStrings.xml",
        "odt" | "ods" | "odp" => name == "content.xml",
        _ => false,
    }
}

fn markup(xml: &str, out: &mut String) {
    let mut rest = xml;
    while let Some((text, tail)) = rest.split_once('<') {
        unescape(text, out);
        let Some((tag, tail)) = tail.split_once('>') else {
            return;
        };
        let closed = tag.strip_prefix('/').and_then(|name| name.rsplit(':').next());
        if matches!(closed, Some("p" | "h" | "si")) {
            out.push('\n');
        }
        rest = tail;
    }
    unescape(rest, out);
}

fn unescape(text: &str, out: &mut String) {
    let mut rest = text;
    while let Some((plain, tail)) = rest.split_once('&') {
        out.push_str(plain);
        match tail.split_once(';').and_then(|(name, after)| Some((entity(name)?, after))) {
            Some((decoded, after)) => {
                out.push(decoded);
                rest = after;
            }
            None => {
                out.push('&');
                rest = tail;
            }
        }
    }
    out.push_str(rest);
}

fn entity(name: &str) -> Option<char> {
    match name {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        _ => {
            let number = name.strip_prefix('#')?;
            let code = match number.strip_prefix(['x', 'X']) {
                Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                None => number.parse().ok()?,
            };
            char::from_u32(code)
        }
    }
}

struct Entry {
    stamp: Stamp,
    text: String,
}

#[derive(Default, PartialEq)]
enum Run {
    #[default]
    Idle,
    Busy,
    Again,
}

#[derive(Default)]
pub struct Index {
    entries: HashMap<ItemId, Entry>,
    run: Run,
}

pub struct Pass {
    sources: Vec<(ItemId, Source)>,
    known: HashMap<ItemId, Stamp>,
}

pub struct Indexed {
    ids: HashSet<ItemId>,
    fresh: Vec<(ItemId, Entry)>,
}

impl Index {
    pub fn get(&self, id: &ItemId) -> Option<&str> {
        self.entries.get(id).map(|entry| entry.text.as_str())
    }

    pub fn start(&mut self, items: &[Item]) -> Option<Pass> {
        if self.run != Run::Idle {
            self.run = Run::Again;
            return None;
        }
        self.run = Run::Busy;
        Some(Pass {
            sources: items.iter().filter_map(|item| Some((item.id.clone(), Source::of(item)?))).collect(),
            known: self.entries.iter().map(|(id, entry)| (id.clone(), entry.stamp)).collect(),
        })
    }

    pub fn finish(&mut self, indexed: Option<Indexed>) -> bool {
        if let Some(Indexed { ids, fresh }) = indexed {
            self.entries.retain(|id, _| ids.contains(id));
            self.entries.extend(fresh);
        }
        std::mem::take(&mut self.run) == Run::Again
    }
}

impl Pass {
    pub fn run(self) -> Indexed {
        let Pass { sources, known } = self;
        let ids = sources.iter().map(|(id, _)| id.clone()).collect();
        let fresh = sources
            .into_iter()
            .filter_map(|(id, source)| {
                let stamp = source.stamp();
                (known.get(&id) != Some(&stamp)).then(|| (id, Entry { stamp, text: source.read() }))
            })
            .collect();
        Indexed { ids, fresh }
    }
}

impl Indexed {
    pub fn changed(&self) -> bool {
        !self.fresh.is_empty()
    }
}

#[cfg(test)]
mod tests;
