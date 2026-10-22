use super::{COPY_SELECTED, Confirm, Filter, Hovered, Page, Screen, Selection, Shelf, copy_id, plugins};
use crate::design::snap;
use crate::i18n::{fill, tr};
use crate::store::{self, Item, ItemId, Layout, Payload, Store};
use gpui::*;
use std::collections::HashSet;

#[derive(Clone, Copy)]
pub(crate) struct Rows {
    pub columns: usize,
    pub top: f32,
    pub pitch: f32,
    pub gap: f32,
}

impl Rows {
    pub fn snapped(columns: usize, top: f32, height: f32, gap: f32, scale: f32) -> Self {
        let gap = snap(gap, scale);
        Self { columns, top: snap(top, scale), pitch: snap(height, scale) + gap, gap }
    }
}

impl Default for Rows {
    fn default() -> Self {
        Self { columns: 1, top: 0., pitch: 0., gap: 0. }
    }
}

impl Shelf {
    pub(crate) fn store(&self) -> &Store {
        &self.store
    }

    pub(crate) fn selection(&self) -> &Selection {
        &self.selection
    }

    pub(crate) fn record_rows(&self, rows: Rows) {
        self.rows.set(rows);
    }

    fn reveal_row(&self, row: usize) {
        let rows = self.rows.get();
        let scroll = self.scroll(Page::Shelf);
        let view = f32::from(scroll.bounds().size.height);
        let top = if row == 0 { 0. } else { rows.top + row as f32 * rows.pitch };
        let bottom = rows.top + row as f32 * rows.pitch + rows.pitch - rows.gap;
        let scrolled = -f32::from(scroll.offset().y);
        let target = if top < scrolled {
            top
        } else if bottom > scrolled + view {
            bottom - view
        } else {
            return;
        };
        let target = target.clamp(0., f32::from(scroll.max_offset().height).max(0.));
        scroll.set_offset(point(px(0.), px(-target)));
    }

    pub(super) fn added(&mut self, count: usize, cx: &mut Context<Self>) {
        self.clear_search(cx);
        self.tabs.pick(Filter::All);
        self.reveal_tab();
        self.navigate(Screen::Shelf, cx);
        self.items_changed(cx);
        log::debug!("Added {count} item(s)");
    }

    pub(super) fn ingest(&mut self, payload: Payload, cx: &mut Context<Self>) {
        match payload {
            Payload::Paths(paths) => {
                let (bundles, paths): (Vec<_>, Vec<_>) = paths.into_iter().partition(|path| plugins::is_bundle(path));
                if !bundles.is_empty() {
                    self.install_plugins(bundles, cx);
                    if paths.is_empty() {
                        return;
                    }
                }
                let count = self.store.add_paths(paths);
                self.added(count, cx);
            }
            Payload::Stored(stored) => {
                let count = self.store.adopt(stored);
                self.added(count, cx);
            }
            Payload::Text(text) => match self.store.add_text(text) {
                Ok(count) => self.added(count, cx),
                Err(error) => self.fail(fill(tr().add_failed, &error), cx),
            },
            Payload::Image(data) => {
                let storage = self.store.storage().clone();
                self.background(
                    cx,
                    move || store::save_image(&storage, data),
                    |shelf, result, cx| match result {
                        Ok((path, title)) => {
                            let count = shelf.store.add_image(path, title);
                            shelf.added(count, cx);
                        }
                        Err(error) => shelf.fail(fill(tr().add_failed, &error), cx),
                    },
                )
            }
        }
    }

    pub fn paste(&mut self, cx: &mut Context<Self>) {
        let storage = self.store.storage().clone();
        self.after_update_then(
            move |desktop| desktop.paste(&storage),
            |shelf, result, cx| match result {
                Ok(payload) => {
                    log::debug!("Pasted {}", payload.describe());
                    shelf.ingest(payload, cx)
                }
                Err(error) => shelf.fail(error.to_string(), cx),
            },
            cx,
        );
    }

    pub(super) fn selected_items(&self) -> Vec<Item> {
        self.store.items().iter().filter(|i| self.selection.contains(&i.id)).cloned().collect()
    }

    fn target_items(&self) -> Vec<Item> {
        if self.selection.is_empty() {
            self.store.items().iter().filter(|item| self.shows(Filter::All, item)).cloned().collect()
        } else {
            self.selected_items()
        }
    }

    pub fn copy(&mut self, items: Vec<Item>, origin: impl Into<SharedString>, cx: &mut Context<Self>) {
        if items.is_empty() {
            return;
        }
        let origin = origin.into();
        let count = items.len();
        let texts: Option<Vec<&str>> = items.iter().map(|item| item.text.as_deref()).collect();
        if let Some(texts) = texts {
            cx.write_to_clipboard(ClipboardItem::new_string(texts.join("\r\n")));
            log::debug!("Copied {count} item(s) as plain text");
            return self.flash_copied(origin, cx);
        }
        self.after_update_then(
            move |desktop| desktop.copy(items),
            move |shelf, result, cx| match result {
                Ok(()) => {
                    log::debug!("Copied {count} item(s)");
                    shelf.flash_copied(origin, cx);
                }
                Err(error) => shelf.fail(error.to_string(), cx),
            },
            cx,
        );
    }

    pub(super) fn hovered_item(&self) -> Option<ItemId> {
        match self.hover.target() {
            Some(Hovered::Item(key))
                if self.page() == Page::Shelf
                    && self.confirming.is_none()
                    && self.store.items().iter().any(|item| item.id == *key) =>
            {
                Some(key.clone())
            }
            _ => None,
        }
    }

    pub(super) fn targets(&self) -> Vec<ItemId> {
        match self.hovered_item() {
            Some(key) if !self.selection.contains(&key) => vec![key],
            _ => self.selection.ids().cloned().collect(),
        }
    }

    pub(super) fn copy_targets(&mut self, cx: &mut Context<Self>) {
        let hovered = self.hovered_item().filter(|key| !self.selection.contains(key));
        match hovered.and_then(|key| self.store.item(&key).cloned()) {
            Some(item) => {
                let origin = copy_id(&item.id);
                self.copy(vec![item], origin, cx);
            }
            None => self.copy_selection(cx),
        }
    }

    pub(super) fn remove_targets(&mut self, cx: &mut Context<Self>) {
        self.discard(self.targets().into_iter().collect(), cx);
    }

    pub fn copy_selection(&mut self, cx: &mut Context<Self>) {
        let items = self.target_items();
        self.copy(items, COPY_SELECTED, cx);
    }

    pub fn open(&mut self, key: &ItemId, reveal: bool, cx: &mut Context<Self>) {
        if let Some(item) = self.store.item(key).cloned() {
            self.after_update(move |desktop| if reveal { desktop.reveal(&item) } else { desktop.open(&item) }, cx);
        }
    }

    pub fn toggle_stuck(&mut self, key: &ItemId, cx: &mut Context<Self>) {
        let pinned = self.store.items().iter().any(|item| item.id == *key && !item.pinned);
        self.stick(HashSet::from([key.clone()]), pinned, cx);
    }

    pub(super) fn stick_targets(&mut self, cx: &mut Context<Self>) {
        let keys: HashSet<ItemId> = self.targets().into_iter().collect();
        let pinned = self.store.items().iter().any(|item| keys.contains(&item.id) && !item.pinned);
        self.stick(keys, pinned, cx);
    }

    fn stick(&mut self, keys: HashSet<ItemId>, pinned: bool, cx: &mut Context<Self>) {
        if self.store.set_pinned(&keys, pinned) {
            self.items_changed(cx);
        }
    }

    pub fn remove(&mut self, key: &ItemId, cx: &mut Context<Self>) {
        self.discard(HashSet::from([key.clone()]), cx);
    }

    pub fn remove_selected(&mut self, cx: &mut Context<Self>) {
        self.discard(self.selection.ids().cloned().collect(), cx);
    }

    fn discard(&mut self, keys: HashSet<ItemId>, cx: &mut Context<Self>) {
        if !keys.is_empty() && !self.ask_remove(&keys, cx) {
            self.drop_items(&keys, cx);
        }
    }

    pub(super) fn ask_remove(&mut self, keys: &HashSet<ItemId>, cx: &mut Context<Self>) -> bool {
        let lost = self.store.items().iter().any(|item| keys.contains(&item.id) && item.only_in_storage());
        if lost {
            self.confirming = Some(Confirm::Remove(keys.clone()));
            cx.notify();
        }
        lost
    }

    pub(super) fn drop_items(&mut self, keys: &HashSet<ItemId>, cx: &mut Context<Self>) {
        self.store.remove(keys);
        self.selection.retain(|key| !keys.contains(key));
        self.items_changed(cx);
    }

    pub(super) fn clears(&self, item: &Item) -> bool {
        !item.pinned && self.shows(self.clearable(), item)
    }

    pub(crate) fn keeps_pinned(&self) -> bool {
        self.store.items().iter().any(|item| item.pinned && self.shows(self.clearable(), item))
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        let keys: HashSet<ItemId> =
            self.store.items().iter().filter(|item| self.clears(item)).map(|item| item.id.clone()).collect();
        self.store.remove(&keys);
        self.selection.reset();
        self.confirming = None;
        self.items_changed(cx);
    }

    pub fn select(&mut self, key: &ItemId, modifiers: Modifiers, cx: &mut Context<Self>) {
        if modifiers.control {
            return self.open(key, true, cx);
        }
        let range = modifiers.shift.then(|| self.selection.anchor().cloned()).flatten().and_then(|anchor| {
            let order = self.visible_ids();
            let a = order.iter().position(|i| *i == anchor)?;
            let b = order.iter().position(|i| i == key)?;
            Some(order.get(a.min(b)..=a.max(b))?.to_vec())
        });
        if let Some(range) = range {
            self.selection.extend(range);
        } else {
            self.toggle(key, cx);
        }
        cx.notify();
    }

    pub fn toggle(&mut self, key: &ItemId, cx: &mut Context<Self>) {
        self.selection.toggle(key);
        cx.notify();
    }

    pub fn set_filter(&mut self, filter: Filter, cx: &mut Context<Self>) {
        let shown = self.filter();
        self.tabs.pick(filter);
        if self.filter() != shown {
            self.reveal_tab();
            self.selection.reset();
            self.load_thumbnails(cx);
        }
        cx.notify();
    }

    pub fn clear_selection(&mut self, cx: &mut Context<Self>) {
        self.selection.clear();
        cx.notify();
    }

    pub fn select_all(&mut self, cx: &mut Context<Self>) {
        let all = self.visible_ids().into_iter().collect();
        self.selection.set(all);
        cx.notify();
    }

    fn grabbed(&self, key: &ItemId) -> Vec<Item> {
        if self.selection.contains(key) {
            return self.selected_items();
        }
        self.store.items().iter().filter(|i| i.id == *key).cloned().collect()
    }

    pub fn press(&mut self, key: &ItemId, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if event.click_count == 2 {
            self.session.release();
            self.selection.restore_pressed();
            if self.store.settings().double_click_preview {
                return self.show_preview(key.clone(), window, cx);
            }
            return self.open(key, false, cx);
        }
        self.selection.press();
        self.session.press(self.grabbed(key), event.position);
    }

    pub fn press_all(&mut self, event: &MouseDownEvent) {
        let items = self.visible_items().into_iter().filter(|item| self.catalog.exists(&item.id)).cloned().collect();
        self.session.press(items, event.position);
    }

    pub(super) fn step_hover(&mut self, delta: isize, cx: &mut Context<Self>) {
        let order = self.visible_ids();
        let Some(last) = order.len().checked_sub(1) else {
            return;
        };
        let current = self.hovered_item().and_then(|key| order.iter().position(|id| *id == key));
        let next = match current {
            Some(index) => index.saturating_add_signed(delta).min(last),
            None if delta > 0 => 0,
            None => last,
        };
        let Some(key) = order.get(next) else {
            return;
        };
        self.hover.step_to(Hovered::Item(key.clone()));
        self.reveal_row(next / self.rows.get().columns.max(1));
        cx.notify();
    }

    pub(super) fn row_step(&self) -> isize {
        self.rows.get().columns.max(1) as isize
    }

    pub fn toggle_layout(&mut self, cx: &mut Context<Self>) {
        self.update_settings(
            |settings| {
                settings.layout = match settings.layout {
                    Layout::List => Layout::Grid,
                    Layout::Grid => Layout::List,
                }
            },
            cx,
        );
        self.fit(cx);
        self.load_thumbnails(cx);
    }

    pub fn point(&mut self, key: &ItemId, cx: &mut Context<Self>) {
        if self.hover.point(Hovered::Item(key.clone())) {
            cx.notify();
        }
    }

    pub fn track(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        if self.hover.moved() {
            cx.notify();
        }
        let Some(items) = self.session.pull(event) else {
            return;
        };
        if items.is_empty() {
            return;
        }
        self.session.lend(items.clone());
        cx.notify();
        self.after_update_then(
            move |desktop| desktop.drag(items),
            |shelf, result, cx| {
                if let Err(error) = result {
                    shelf.fail(error.to_string(), cx);
                }
            },
            cx,
        );
    }
}
