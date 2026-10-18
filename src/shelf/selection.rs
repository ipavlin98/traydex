use crate::store::ItemId;
use std::collections::HashSet;

#[derive(Default)]
pub struct Selection {
    ids: HashSet<ItemId>,
    anchor: Option<ItemId>,
    pressed: Option<HashSet<ItemId>>,
}

impl Selection {
    pub fn contains(&self, id: &ItemId) -> bool {
        self.ids.contains(id)
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    pub fn len(&self) -> usize {
        self.ids.len()
    }

    pub fn clear(&mut self) {
        self.ids.clear();
    }

    pub fn anchor(&self) -> Option<&ItemId> {
        self.anchor.as_ref()
    }

    pub fn ids(&self) -> impl Iterator<Item = &ItemId> {
        self.ids.iter()
    }

    pub(super) fn extend(&mut self, ids: impl IntoIterator<Item = ItemId>) {
        self.ids.extend(ids);
    }

    pub(super) fn set(&mut self, ids: HashSet<ItemId>) {
        self.ids = ids;
    }

    pub(super) fn press(&mut self) {
        self.pressed = Some(self.ids.clone());
    }

    pub(super) fn restore_pressed(&mut self) {
        if let Some(ids) = self.pressed.take() {
            self.ids = ids;
        }
    }

    pub(super) fn reset(&mut self) {
        self.ids.clear();
        self.anchor = None;
    }

    pub(super) fn toggle(&mut self, id: &ItemId) {
        if !self.ids.remove(id) {
            self.ids.insert(id.clone());
        }
        self.anchor = Some(id.clone());
    }

    pub(super) fn remove(&mut self, id: &ItemId) {
        self.ids.remove(id);
    }

    pub(super) fn retain(&mut self, keep: impl Fn(&ItemId) -> bool) {
        self.ids.retain(keep);
    }

    pub(super) fn take(&mut self) -> HashSet<ItemId> {
        std::mem::take(&mut self.ids)
    }
}
