use super::{ItemId, Storage, restore_note};
use anyhow::Result;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

pub struct Snapshot {
    pub(super) storage: Storage,
    pub(super) shelf: Arc<[u8]>,
    pub(super) notes: Vec<(ItemId, PathBuf, String)>,
    pub(super) doomed: Vec<PathBuf>,
}

impl Snapshot {
    pub fn write(&self) -> Result<()> {
        let root = self.storage.root();
        let temp = root.join("shelf.tmp");
        fs::write(&temp, &*self.shelf)?;
        fs::rename(temp, root.join("shelf.json"))?;
        for (_, path, text) in &self.notes {
            restore_note(path, text)?;
        }
        for path in &self.doomed {
            self.storage.delete(path);
        }
        Ok(())
    }

    pub fn merge(self, newer: Snapshot) -> Snapshot {
        let mut notes: Vec<_> =
            self.notes.into_iter().filter(|(key, ..)| !newer.notes.iter().any(|(fresh, ..)| fresh == key)).collect();
        notes.extend(newer.notes);
        let mut doomed = self.doomed;
        doomed.extend(newer.doomed);
        Snapshot { storage: newer.storage, shelf: newer.shelf, notes, doomed }
    }
}
