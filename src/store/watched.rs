use super::{ItemId, Store, Unsaved};
use serde::{Deserialize, Serialize};
use std::cmp::Reverse;
use std::collections::{BTreeSet, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

const UNFINISHED: [&str; 4] = ["crdownload", "part", "partial", "tmp"];

#[derive(Clone, Serialize, Deserialize)]
pub struct Watched {
    path: PathBuf,
    #[serde(default)]
    seen: BTreeSet<String>,
}

impl Watched {
    pub fn path(&self) -> &Path {
        &self.path
    }
}

fn name(path: &Path) -> Option<&str> {
    path.file_name()?.to_str()
}

fn unfinished(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| UNFINISHED.iter().any(|known| extension.to_string_lossy().eq_ignore_ascii_case(known)))
}

pub fn list_folder(folder: &Path) -> io::Result<Vec<PathBuf>> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(folder)? {
        let entry = entry?;
        let path = entry.path();
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if name(&path).is_none() || crate::os::hidden(&metadata) || unfinished(&path) {
            continue;
        }
        entries.push((metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH), path));
    }
    entries.sort_by_key(|(modified, _)| Reverse(*modified));
    Ok(entries.into_iter().map(|(_, path)| path).collect())
}

impl Store {
    pub fn watched(&self) -> &[Watched] {
        &self.watched
    }

    pub fn watch(&mut self, folder: PathBuf) -> bool {
        if self.watched.iter().any(|watched| watched.path == folder) {
            return false;
        }
        self.watched.push(Watched { path: folder, seen: BTreeSet::new() });
        self.touch(Unsaved::Now);
        true
    }

    pub fn unwatch(&mut self, folder: &Path) -> bool {
        let Some(at) = self.watched.iter().position(|watched| watched.path == folder) else {
            return false;
        };
        self.watched.remove(at);
        let keys: HashSet<ItemId> = self
            .items
            .iter()
            .filter(|item| !item.pinned && item.path.parent() == Some(folder))
            .map(|item| item.id.clone())
            .collect();
        self.remove(&keys);
        self.touch(Unsaved::Now);
        true
    }

    pub fn sync_folder(&mut self, folder: &Path, entries: Vec<PathBuf>) -> bool {
        let Some(watched) = self.watched.iter_mut().find(|watched| watched.path == folder) else {
            return false;
        };
        let current: BTreeSet<String> = entries.iter().filter_map(|path| name(path)).map(str::to_owned).collect();
        if current == watched.seen {
            return false;
        }
        let fresh: Vec<PathBuf> =
            entries.into_iter().filter(|path| name(path).is_some_and(|name| !watched.seen.contains(name))).collect();
        let gone: HashSet<PathBuf> = watched.seen.difference(&current).map(|name| folder.join(name)).collect();
        watched.seen = current;
        let keys: HashSet<ItemId> =
            self.items.iter().filter(|item| gone.contains(&item.path)).map(|item| item.id.clone()).collect();
        self.remove(&keys);
        self.add_paths(fresh);
        self.touch(Unsaved::Now);
        true
    }
}
