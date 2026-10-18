mod files;
mod images;
mod settings;
mod snapshot;
mod storage;

pub use files::{is_archive, is_image, restore_note, save_virtual, set_aside, vacant};
pub use images::{decode, dib_bytes, open_image, open_oriented, png_bytes, save_image, title, write_png};
pub use settings::{Corner, DEFAULT_SIZE, FONT_SIZES, MIN_SIZE, Settings, ShakeLevel, TileInput};
pub use snapshot::Snapshot;
pub use storage::{Scratch, Storage, id};

use self::files::{copy_path, save_note};
use crate::i18n::{fill, tr};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const TITLE_LIMIT: usize = 96;

#[derive(Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub enum Layout {
    #[default]
    List,
    Grid,
}

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum Kind {
    File,
    Folder,
    Image,
    Text,
    Link,
    Note,
}

#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ItemId(String);

impl ItemId {
    pub fn new() -> Self {
        Self(id())
    }
}

impl fmt::Display for ItemId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(from = "StoredItem")]
pub struct Item {
    pub id: ItemId,
    pub kind: Kind,
    pub title: String,
    pub path: PathBuf,
    pub text: Option<String>,
    pub managed: bool,
    pub named: bool,
    pub pinned: bool,
}

#[derive(Deserialize)]
struct StoredItem {
    id: ItemId,
    kind: Kind,
    title: String,
    path: PathBuf,
    text: Option<String>,
    managed: bool,
    #[serde(default)]
    named: Option<bool>,
    #[serde(default)]
    pinned: bool,
}

impl From<StoredItem> for Item {
    fn from(stored: StoredItem) -> Self {
        let StoredItem { id, kind, title, path, text, managed, named, pinned } = stored;
        let mut item = Item { id, kind, title, path, text, managed, named: false, pinned };
        if item.editable() {
            let text = item.text.as_deref().unwrap_or_default();
            let auto = title_of(text);
            let legacy: String = squash(text).chars().take(TITLE_LIMIT).collect();
            item.named = named.unwrap_or(item.title != auto && item.title != legacy);
            if !item.named {
                item.title = auto;
            }
        }
        item
    }
}

#[derive(Clone)]
pub struct Meta {
    pub exists: bool,
    pub detail: String,
}

pub enum Payload {
    Paths(Vec<PathBuf>),
    Stored(Scratch),
    Text(String),
    Image(ImageData),
}

impl Payload {
    pub fn of(items: &[Item]) -> Option<Payload> {
        match items {
            [] => None,
            [item] => match item.text.as_deref().filter(|text| !text.trim().is_empty()) {
                Some(text) => Some(Payload::Text(text.to_owned())),
                None => Some(Payload::Paths(vec![item.path.clone()])),
            },
            items => Some(Payload::Paths(items.iter().map(|item| item.path.clone()).collect())),
        }
    }

    pub fn subject(&self) -> Option<String> {
        let named = |paths: &[PathBuf]| {
            let first = paths.first()?.file_name()?.to_string_lossy().into_owned();
            Some(match paths.len() {
                1 => first,
                count => format!("{first} {}", fill(tr().and_more, count - 1)),
            })
        };
        match self {
            Payload::Paths(paths) => named(paths),
            Payload::Stored(stored) => named(stored.paths()),
            Payload::Text(text) => Some(title_of(text)).filter(|title| !title.is_empty()),
            Payload::Image(_) => None,
        }
    }

    pub fn describe(&self) -> String {
        match self {
            Payload::Paths(paths) => format!("{} path(s)", paths.len()),
            Payload::Stored(stored) => format!("{} stored file(s)", stored.paths().len()),
            Payload::Text(text) => format!("text, {} chars", text.chars().count()),
            Payload::Image(ImageData::Png(bytes)) => format!("PNG image, {} bytes", bytes.len()),
            Payload::Image(ImageData::Dib(bytes)) => format!("DIB image, {} bytes", bytes.len()),
        }
    }
}

pub enum ImageData {
    Png(Vec<u8>),
    Dib(Vec<u8>),
}

impl Item {
    pub fn icon(&self) -> &'static str {
        match self.kind {
            Kind::File if is_archive(&self.path) => "file-archive",
            Kind::File => "file",
            Kind::Folder => "folder",
            Kind::Image => "image",
            Kind::Text => "text",
            Kind::Link => "link",
            Kind::Note => "note",
        }
    }

    pub fn editable(&self) -> bool {
        matches!(self.kind, Kind::Text | Kind::Link | Kind::Note) && self.text.is_some()
    }

    pub fn note(&self) -> bool {
        self.kind == Kind::Note
    }

    pub fn auto_title(&self) -> bool {
        !self.named
    }

    pub fn describe(&self) -> Meta {
        match self.kind {
            Kind::Text | Kind::Note => Meta {
                exists: true,
                detail: fill(tr().text_detail, self.text.as_deref().unwrap_or_default().chars().count()),
            },
            Kind::Link => Meta { exists: true, detail: link_host(self.text.as_deref().unwrap_or_default()) },
            Kind::Folder => Meta { exists: self.path.is_dir(), detail: tr().folder.into() },
            Kind::File | Kind::Image => match fs::metadata(&self.path) {
                Ok(metadata) => {
                    let label = self
                        .path
                        .extension()
                        .map(|e| e.to_string_lossy().to_uppercase())
                        .unwrap_or_else(|| tr().file.into());
                    Meta { exists: true, detail: format!("{label} · {}", human_size(metadata.len())) }
                }
                Err(_) => Meta { exists: false, detail: String::new() },
            },
        }
    }
}

fn kind_of(text: &str) -> Kind {
    let trimmed = text.trim();
    if (trimmed.starts_with("https://") || trimmed.starts_with("http://")) && !trimmed.contains(char::is_whitespace) {
        Kind::Link
    } else {
        Kind::Text
    }
}

fn squash(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn title_of(text: &str) -> String {
    let title = squash(text);
    if title.chars().count() <= TITLE_LIMIT {
        return title;
    }
    let cut: String = title.chars().take(TITLE_LIMIT - 1).collect();
    format!("{}…", gpui::cut_before_ellipsis(&cut))
}

pub fn host(url: &str) -> &str {
    let rest = url.trim().split("://").nth(1).unwrap_or(url);
    let host = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    host.trim_start_matches("www.")
}

fn link_host(url: &str) -> String {
    fill(tr().link_detail, host(url))
}

pub fn human_size(bytes: u64) -> String {
    match bytes {
        b if b >= 1 << 30 => format!("{:.1} {}", b as f64 / (1u64 << 30) as f64, tr().unit_gb),
        b if b >= 1 << 20 => format!("{:.1} {}", b as f64 / (1u64 << 20) as f64, tr().unit_mb),
        b if b >= 1 << 10 => format!("{} {}", b.div_ceil(1 << 10), tr().unit_kb),
        b => format!("{b} {}", tr().unit_b),
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Unsaved {
    Soon,
    Now,
}

#[derive(Serialize, Deserialize)]
pub struct Store {
    items: Vec<Item>,
    #[serde(flatten)]
    settings: Settings,
    #[serde(skip)]
    storage: Storage,
    #[serde(skip)]
    doomed: Vec<PathBuf>,
    #[serde(skip)]
    edited: Vec<ItemId>,
    #[serde(skip)]
    unsaved: Option<Unsaved>,
    #[serde(skip)]
    trouble: Option<String>,
    #[serde(skip)]
    last: Option<Arc<[u8]>>,
}

fn local_data() -> PathBuf {
    crate::profile::current().local_data()
}

pub fn root() -> PathBuf {
    crate::profile::current().data_root()
}

fn legacy_root() -> PathBuf {
    local_data().join("DragonDrop")
}

fn migrate(root: &Path, legacy: &Path) -> Option<PathBuf> {
    if root.exists() || !legacy.join("shelf.json").exists() {
        return None;
    }
    if fs::rename(legacy, root).is_err() {
        copy_path(legacy, root).ok()?;
    }
    Some(legacy.to_path_buf())
}

impl Store {
    pub fn load() -> Self {
        Self::open(Storage::new(root()), &legacy_root())
    }

    pub fn open(storage: Storage, legacy: &Path) -> Self {
        let legacy = migrate(storage.root(), legacy);
        let (store, readable) = Self::read(storage, legacy);
        if readable {
            store.sweep();
        }
        store
    }

    pub fn reverted(&self) -> Self {
        let Some(last) = self.last.clone() else {
            return Self::read(self.storage.clone(), None).0;
        };
        match serde_json::from_slice::<Self>(&last) {
            Ok(store) => Self { last: Some(last), ..store.attached(self.storage.clone()) },
            Err(error) => {
                log::error!("Couldn't restore the last snapshot, reading the shelf from disk: {error:#}");
                Self::read(self.storage.clone(), None).0
            }
        }
    }

    fn attached(mut self, storage: Storage) -> Self {
        self.storage = storage;
        for item in &mut self.items {
            item.managed = self.storage.owned(&item.path);
        }
        self.items.sort_by_key(|item| !item.pinned);
        self
    }

    fn read(storage: Storage, legacy: Option<PathBuf>) -> (Self, bool) {
        let root = storage.root().to_path_buf();
        let _ = fs::create_dir_all(storage.content());
        let path = root.join("shelf.json");
        let loaded = match fs::read(&path) {
            Ok(data) => serde_json::from_slice::<Self>(&data).map_err(anyhow::Error::from),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::empty()),
            Err(error) => Err(error.into()),
        };
        let readable = loaded.is_ok();
        let mut store = loaded.unwrap_or_else(|error| {
            log::error!("Couldn't read {}: {error:#}", path.display());
            let trouble = match set_aside(&path) {
                Ok(broken) => fill(tr().shelf_set_aside, broken.display()),
                Err(backup) => {
                    log::error!("Couldn't set the shelf aside: {backup:#}");
                    fill(tr().shelf_unreadable, error)
                }
            };
            Self { trouble: Some(trouble), ..Self::empty() }
        });
        if let Some(legacy) = &legacy {
            for item in &mut store.items {
                if let Ok(rest) = item.path.strip_prefix(legacy) {
                    item.path = root.join(rest);
                }
            }
        }
        let mut store = store.attached(storage);
        let gathered = store.gather_notes();
        if (legacy.is_some() || gathered)
            && let Err(error) = store.save()
        {
            log::warn!("Couldn't save the migrated shelf: {error:#}");
        }
        (store, readable)
    }

    fn gather_notes(&mut self) -> bool {
        let content = self.storage.content();
        let notes = self.storage.notes();
        let mut moved = false;
        for item in self.items.iter_mut().filter(|item| item.note() && item.path.parent() == Some(content.as_path())) {
            let Some(name) = item.path.file_name() else {
                continue;
            };
            let target = notes.join(name);
            let relocated = fs::create_dir_all(&notes).and_then(|()| match fs::rename(&item.path, &target) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                other => other,
            });
            match relocated {
                Ok(()) => {
                    item.path = target;
                    moved = true;
                }
                Err(error) => log::warn!("Couldn't move {} to the notes folder: {error}", item.path.display()),
            }
        }
        moved
    }

    fn sweep(&self) {
        let referenced: Vec<&Path> = self.items.iter().map(|item| item.path.as_path()).collect();
        self.storage.collect(&referenced);
    }

    fn empty() -> Self {
        Self {
            items: Vec::new(),
            settings: Settings::default(),
            storage: Storage::default(),
            doomed: Vec::new(),
            edited: Vec::new(),
            unsaved: None,
            trouble: None,
            last: None,
        }
    }

    pub fn items(&self) -> &[Item] {
        &self.items
    }

    pub fn item(&self, id: &ItemId) -> Option<&Item> {
        self.items.iter().find(|item| item.id == *id)
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    pub fn settings_mut(&mut self) -> &mut Settings {
        self.touch(Unsaved::Now);
        &mut self.settings
    }

    pub fn storage(&self) -> &Storage {
        &self.storage
    }

    pub fn take_trouble(&mut self) -> Option<String> {
        self.trouble.take()
    }

    pub fn reset_settings(&mut self) {
        *self.settings_mut() = Settings::default();
    }

    fn touch(&mut self, unsaved: Unsaved) {
        self.unsaved = self.unsaved.max(Some(unsaved));
    }

    pub fn take_unsaved(&mut self) -> Option<Unsaved> {
        self.unsaved.take()
    }

    pub fn snapshot(&mut self) -> Result<Snapshot> {
        let shelf: Arc<[u8]> = serde_json::to_vec_pretty(self)?.into();
        self.last = Some(shelf.clone());
        let edited = std::mem::take(&mut self.edited);
        let notes = edited
            .iter()
            .filter_map(|key| {
                let item = self.item(key)?;
                Some((key.clone(), item.path.clone(), item.text.clone()?))
            })
            .collect();
        Ok(Snapshot { storage: self.storage.clone(), shelf, notes, doomed: std::mem::take(&mut self.doomed) })
    }

    pub fn restore(&mut self, unsaved: Snapshot) {
        for (key, ..) in unsaved.notes {
            if !self.edited.contains(&key) {
                self.edited.push(key);
            }
        }
        self.doomed.extend(unsaved.doomed);
    }

    pub fn save(&mut self) -> Result<()> {
        let snapshot = self.snapshot()?;
        let written = snapshot.write();
        if written.is_err() {
            self.restore(snapshot);
        }
        written
    }

    pub fn add_paths(&mut self, paths: Vec<PathBuf>) -> usize {
        let front = self.front();
        let mut count = 0;
        for path in paths {
            if !path.exists() || self.items.iter().any(|i| i.path == path) {
                continue;
            }
            let kind = if path.is_dir() {
                Kind::Folder
            } else if is_image(&path) {
                Kind::Image
            } else {
                Kind::File
            };
            let title = path.file_name().unwrap_or(path.as_os_str()).to_string_lossy().into_owned();
            let managed = self.storage.owned(&path);
            let item = Item { id: ItemId::new(), kind, title, managed, path, text: None, named: false, pinned: false };
            self.items.insert(front + count, item);
            count += 1;
        }
        if count > 0 {
            self.touch(Unsaved::Now);
        }
        count
    }

    pub fn add_text(&mut self, text: String) -> Result<usize> {
        if text.trim().is_empty() || self.items.iter().any(|i| i.text.as_ref() == Some(&text)) {
            return Ok(0);
        }
        let path = save_note(&self.storage, &text)?;
        self.items.insert(
            self.front(),
            Item {
                id: ItemId::new(),
                kind: kind_of(&text),
                title: title_of(&text),
                path,
                text: Some(text),
                managed: true,
                named: false,
                pinned: false,
            },
        );
        self.touch(Unsaved::Now);
        Ok(1)
    }

    pub fn add_note(&mut self) -> ItemId {
        let key = ItemId::new();
        self.items.insert(
            self.front(),
            Item {
                id: key.clone(),
                kind: Kind::Note,
                title: String::new(),
                path: self.storage.note(),
                text: Some(String::new()),
                managed: true,
                named: false,
                pinned: false,
            },
        );
        self.touch(Unsaved::Now);
        key
    }

    pub fn edit_text(&mut self, key: &ItemId, text: &str) {
        let Some(item) = self.items.iter_mut().find(|item| item.id == *key && item.editable()) else {
            return;
        };
        let old = item.text.as_deref().unwrap_or_default();
        if old == text {
            return;
        }
        if !item.named {
            item.title = title_of(text);
        }
        if !item.note() {
            item.kind = kind_of(text);
        }
        item.text = Some(text.to_owned());
        if self.storage.owned(&item.path) && !self.edited.contains(key) {
            self.edited.push(key.clone());
        }
        self.touch(Unsaved::Soon);
    }

    pub fn rename(&mut self, key: &ItemId, title: &str) -> bool {
        let Some(item) = self.items.iter_mut().find(|item| item.id == *key && item.editable()) else {
            return false;
        };
        let custom = title_of(title);
        let named = !custom.is_empty();
        let title = if named { custom } else { title_of(item.text.as_deref().unwrap_or_default()) };
        if item.title == title && item.named == named {
            return false;
        }
        item.title = title;
        item.named = named;
        self.touch(Unsaved::Soon);
        true
    }

    pub fn add_image(&mut self, path: PathBuf, title: String) -> usize {
        let item = Item {
            id: ItemId::new(),
            kind: Kind::Image,
            title,
            path,
            text: None,
            managed: true,
            named: false,
            pinned: false,
        };
        self.items.insert(self.front(), item);
        self.touch(Unsaved::Now);
        1
    }

    fn front(&self) -> usize {
        self.items.iter().take_while(|item| item.pinned).count()
    }

    pub fn set_pinned(&mut self, keys: &HashSet<ItemId>, pinned: bool) -> bool {
        let (mut moved, kept): (Vec<Item>, Vec<Item>) = std::mem::take(&mut self.items)
            .into_iter()
            .partition(|item| keys.contains(&item.id) && item.pinned != pinned);
        self.items = kept;
        if moved.is_empty() {
            return false;
        }
        for item in &mut moved {
            item.pinned = pinned;
        }
        let at = if pinned { 0 } else { self.front() };
        self.items.splice(at..at, moved);
        self.touch(Unsaved::Now);
        true
    }

    pub fn relocate(&mut self, moves: &[(PathBuf, PathBuf)]) {
        for item in &mut self.items {
            for (from, to) in moves {
                if item.path == *from {
                    item.title = to.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
                    item.path = to.clone();
                    break;
                }
                if let Ok(rest) = item.path.strip_prefix(from) {
                    item.path = to.join(rest);
                    break;
                }
            }
        }
        self.touch(Unsaved::Now);
    }

    pub fn adopt(&mut self, scratch: Scratch) -> usize {
        let paths = scratch.keep();
        let count = self.add_paths(paths.clone());
        for path in paths.iter().filter(|path| !self.items.iter().any(|i| i.path == **path)) {
            self.storage.release(path);
        }
        count
    }

    pub fn remove(&mut self, keys: &HashSet<ItemId>) {
        let (gone, kept): (Vec<Item>, Vec<Item>) =
            std::mem::take(&mut self.items).into_iter().partition(|item| keys.contains(&item.id));
        self.items = kept;
        if !gone.is_empty() {
            self.touch(Unsaved::Now);
        }
        self.forget(gone);
    }

    fn forget(&mut self, gone: Vec<Item>) {
        for item in gone.into_iter().filter(|item| item.managed) {
            let Some(slot) = self.storage.slot(&item.path) else {
                continue;
            };
            let others: Vec<&Path> =
                self.items.iter().filter(|other| other.managed).map(|other| other.path.as_path()).collect();
            if !others.iter().any(|other| storage::related(other, &slot)) {
                self.doomed.push(slot);
            } else if !others.iter().any(|other| storage::related(other, &item.path)) {
                self.doomed.push(item.path);
            }
        }
    }
}

#[cfg(test)]
mod tests;
