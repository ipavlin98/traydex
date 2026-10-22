use super::{Item, Store, Unsaved};
use std::path::PathBuf;

const REMEMBERED: usize = 100;

impl Store {
    pub(super) fn remember_recycled(&mut self, item: Item) {
        self.binned.push(item.path.clone());
        self.recycled.push(item);
        let excess = self.recycled.len().saturating_sub(REMEMBERED);
        self.recycled.drain(..excess);
    }

    pub fn recycling(&self) -> bool {
        !self.binned.is_empty()
    }

    pub fn recycled(&self) -> Vec<PathBuf> {
        self.recycled.iter().map(|item| item.path.clone()).collect()
    }

    pub fn bring_back(&mut self, found: &[PathBuf]) -> usize {
        let (back, gone): (Vec<Item>, Vec<Item>) =
            std::mem::take(&mut self.recycled).into_iter().partition(|item| found.contains(&item.path));
        self.recycled = gone;
        if !back.is_empty() {
            self.touch(Unsaved::Now);
        }
        let mut count = 0;
        for mut item in back {
            if self.items.iter().any(|other| other.path == item.path) {
                continue;
            }
            item.managed = self.storage.owned(&item.path);
            let at = if item.pinned { 0 } else { self.front() };
            self.items.insert(at, item);
            count += 1;
        }
        count
    }
}
