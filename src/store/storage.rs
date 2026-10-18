use std::{
    fs, io,
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicU32, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

pub fn id() -> String {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    format!("{nanos}{:04}", COUNTER.fetch_add(1, Ordering::Relaxed) % 10_000)
}

#[derive(Clone, Default, Debug, PartialEq)]
pub struct Storage {
    root: PathBuf,
}

impl Storage {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn content(&self) -> PathBuf {
        self.root.join("content")
    }

    pub fn notes(&self) -> PathBuf {
        self.content().join("notes")
    }

    pub fn note(&self) -> PathBuf {
        self.notes().join(format!("note-{}.txt", id()))
    }

    pub fn slot(&self, path: &Path) -> Option<PathBuf> {
        let content = self.content();
        match path.strip_prefix(&content).ok()?.components().next()? {
            Component::Normal(name) => Some(content.join(name)),
            _ => None,
        }
    }

    pub fn owned(&self, path: &Path) -> bool {
        self.slot(path).is_some()
    }

    pub fn allocate(&self, name: &str) -> io::Result<PathBuf> {
        let folder = self.content().join(format!("files-{}", id()));
        fs::create_dir_all(&folder)?;
        Ok(folder.join(name))
    }

    pub fn unique(&self, prefix: &str, extension: &str) -> PathBuf {
        self.content().join(format!("{prefix}-{}.{extension}", id()))
    }

    pub fn delete(&self, path: &Path) {
        if !self.owned(path) {
            log::warn!("Refused to delete {}: it's outside the storage", path.display());
            return;
        }
        let removed = match fs::symlink_metadata(path) {
            Ok(metadata) if metadata.is_symlink() => fs::remove_dir(path).or_else(|_| fs::remove_file(path)),
            Ok(metadata) if metadata.is_dir() => fs::remove_dir_all(path),
            Ok(_) => fs::remove_file(path),
            Err(error) if error.kind() == io::ErrorKind::NotFound => return,
            Err(error) => Err(error),
        };
        match removed {
            Ok(()) => log::debug!("Removed {} from the storage", path.display()),
            Err(error) => log::warn!("Couldn't remove {}: {error}", path.display()),
        }
    }

    pub fn release(&self, path: &Path) {
        if let Some(slot) = self.slot(path) {
            self.delete(&slot);
        }
    }

    pub fn discard(&self, path: &Path) {
        if self.owned(path) {
            self.release(path);
            return;
        }
        if let Err(error) = fs::remove_file(path)
            && error.kind() != io::ErrorKind::NotFound
        {
            log::warn!("Couldn't delete {}: {error}", path.display());
        }
    }

    pub fn collect(&self, referenced: &[&Path]) {
        let in_use = |path: &Path| referenced.iter().any(|item| related(item, path));
        let whole = |path: &Path| referenced.iter().any(|item| folded(path).starts_with(folded(item)));
        let Ok(entries) = fs::read_dir(self.content()) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !in_use(&path) {
                self.delete(&path);
                continue;
            }
            if whole(&path) || !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                continue;
            }
            for child in fs::read_dir(&path).into_iter().flatten().flatten() {
                let child = child.path();
                if !in_use(&child) {
                    self.delete(&child);
                }
            }
        }
    }
}

fn folded(path: &Path) -> PathBuf {
    PathBuf::from(path.to_string_lossy().to_lowercase())
}

pub fn related(a: &Path, b: &Path) -> bool {
    let (a, b) = (folded(a), folded(b));
    a.starts_with(&b) || b.starts_with(&a)
}

pub struct Scratch {
    storage: Storage,
    paths: Vec<PathBuf>,
}

impl Scratch {
    pub fn new(storage: Storage) -> Self {
        Self { storage, paths: Vec::new() }
    }

    pub fn push(&mut self, path: PathBuf) {
        self.paths.push(path);
    }

    pub fn paths(&self) -> &[PathBuf] {
        &self.paths
    }

    pub fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }

    pub fn keep(mut self) -> Vec<PathBuf> {
        std::mem::take(&mut self.paths)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        for path in &self.paths {
            self.storage.release(path);
        }
    }
}
