use super::notes::Picker;
use super::preview::Back;
use super::{Filter, Hovered, Page, Screen, Shelf, plugins};
use crate::i18n::{fill, tr};
use crate::search::{Grade, Query};
use crate::store::{self, Item, ItemId, Kind, Payload};
use crate::textarea::{AreaEvent, Chip, Peek};
use gpui::*;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

const PEEK_DELAY: Duration = Duration::from_millis(350);

impl Shelf {
    pub(super) fn area_event(&mut self, event: &AreaEvent, window: &mut Window, cx: &mut Context<Self>) {
        match event {
            AreaEvent::Suggest(query) => self.suggest(query.as_deref(), cx),
            AreaEvent::Anchored => cx.notify(),
            AreaEvent::Step(delta) => self.step_pick(*delta, cx),
            AreaEvent::Choose => self.choose_pick(cx),
            AreaEvent::Hover(peek) => self.peek(peek.clone(), cx),
            AreaEvent::Open { id, reveal: true } => self.show_on_shelf(id, cx),
            AreaEvent::Open { id, reveal: false } => self.open_mention(id, window, cx),
            AreaEvent::Press { id, position } => {
                if let Some(item) = self.store.item(id).filter(|_| self.catalog.exists(id)).cloned() {
                    self.session.press(vec![item], *position);
                }
            }
            AreaEvent::Paste => self.paste_into_note(cx),
        }
    }

    pub(super) fn chip_looks(&self, ids: &[ItemId]) -> HashMap<ItemId, Chip> {
        ids.iter()
            .filter_map(|id| {
                let item = self.store.item(id)?;
                let chip =
                    Chip { label: item.title.clone().into(), icon: item.icon(), missing: !self.catalog.exists(id) };
                Some((id.clone(), chip))
            })
            .collect()
    }

    pub(super) fn refresh_mentions(&mut self, cx: &mut Context<Self>) {
        let Some(area) = self.note().map(|note| note.area.clone()) else {
            return;
        };
        let looks = self.chip_looks(&area.read(cx).referenced());
        area.update(cx, |area, cx| area.set_looks(looks, cx));
    }

    fn mention_hits(&self, note: &ItemId, query: &str) -> Vec<ItemId> {
        let candidates =
            self.store.items().iter().filter(|item| item.id != *note && !matches!(item.kind, Kind::Note | Kind::Text));
        let Some(query) = Query::parse(query) else {
            return candidates.map(|item| item.id.clone()).collect();
        };
        let mut graded: Vec<(usize, usize, &Item)> = candidates
            .enumerate()
            .filter_map(|(order, item)| {
                let grade = Grade::ALL.into_iter().position(|grade| query.find(&item.title, None, grade).is_some())?;
                Some((grade, order, item))
            })
            .collect();
        graded.sort_by_key(|(grade, order, _)| (*grade, *order));
        graded.into_iter().map(|(_, _, item)| item.id.clone()).collect()
    }

    fn suggest(&mut self, query: Option<&str>, cx: &mut Context<Self>) {
        let Some(key) = self.previewed() else {
            return;
        };
        let picker =
            query.map(|query| Picker { hits: self.mention_hits(&key, query), chosen: 0, scroll: ScrollHandle::new() });
        let Some(note) = self.note_mut() else {
            return;
        };
        let open = picker.is_some();
        note.picker = picker;
        note.area.update(cx, |area, _| area.set_suggesting(open));
        cx.notify();
    }

    fn step_pick(&mut self, delta: isize, cx: &mut Context<Self>) {
        let Some(picker) = self.note_mut().and_then(|note| note.picker.as_mut()) else {
            return;
        };
        let count = picker.hits.len();
        if count > 0 {
            picker.chosen = (picker.chosen as isize + delta).rem_euclid(count as isize) as usize;
            picker.scroll.scroll_to_item(picker.chosen);
            cx.notify();
        }
    }

    pub fn point_pick(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(picker) = self.note_mut().and_then(|note| note.picker.as_mut()) else {
            return;
        };
        if picker.chosen != index && index < picker.hits.len() {
            picker.chosen = index;
            cx.notify();
        }
    }

    fn choose_pick(&mut self, cx: &mut Context<Self>) {
        let chosen = self.note().and_then(|note| note.picker.as_ref()).and_then(|p| p.hits.get(p.chosen)).cloned();
        match chosen {
            Some(id) => self.pick_mention(&id, cx),
            None => {
                if let Some(area) = self.note().map(|note| note.area.clone()) {
                    area.update(cx, |area, cx| area.dismiss(cx));
                }
            }
        }
    }

    pub fn pick_mention(&mut self, id: &ItemId, cx: &mut Context<Self>) {
        let (Some(area), Some(item)) = (self.note().map(|note| note.area.clone()), self.store.item(id)) else {
            return;
        };
        let title = item.title.clone();
        area.update(cx, |area, cx| area.choose(id, &title, cx));
        self.refresh_mentions(cx);
    }

    fn peek(&mut self, peek: Option<Peek>, cx: &mut Context<Self>) {
        let Some(note) = self.note_mut() else {
            return;
        };
        note.peeking = None;
        let Some(peek) = peek else {
            if note.peek.take().is_some() {
                cx.notify();
            }
            return;
        };
        note.peek = None;
        note.peeking = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(PEEK_DELAY).await;
            let _ = this.update(cx, |shelf, cx| {
                shelf.guarded(cx, |shelf, cx| {
                    if let Some(note) = shelf.note_mut() {
                        note.peek = Some(peek);
                        cx.notify();
                    }
                })
            });
        }));
        cx.notify();
    }

    fn open_mention(&mut self, id: &ItemId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(note) = self.previewed() else {
            return;
        };
        if self.store.item(id).is_none() {
            return self.fail(tr().mention_gone, cx);
        }
        let cursor = self.note().map_or(0, |current| current.area.read(cx).cursor());
        self.show_preview(id.clone(), window, cx);
        if let Some(preview) = self.preview_mut().filter(|preview| preview.key == *id) {
            preview.back = Some(Back { note, cursor });
        }
    }

    pub fn back_to_note(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(back) = self.preview().and_then(|preview| preview.back.clone()) else {
            return false;
        };
        self.show_preview(back.note.clone(), window, cx);
        let Some(area) = self.note().map(|note| note.area.clone()) else {
            return false;
        };
        area.update(cx, |area, cx| area.place_cursor(back.cursor, cx));
        area.update(cx, |input, cx| input.focus(window, cx));
        true
    }

    pub fn turn_cited(&mut self, delta: isize, cx: &mut Context<Self>) {
        let Some(key) = self.previewed() else {
            return;
        };
        let count = self.catalog.citing(&key).len();
        if let Some(preview) = self.preview_mut().filter(|_| count > 1) {
            preview.cited = (preview.cited as isize + delta).rem_euclid(count as isize) as usize;
            cx.notify();
        }
    }

    pub fn open_citing(&mut self, id: &ItemId, window: &mut Window, cx: &mut Context<Self>) {
        let shown = self.preview().filter(|preview| preview.key == *id).map_or(0, |preview| preview.cited);
        let citing = self.catalog.citing(id);
        let Some(note) = citing.get(shown).or(citing.first()).cloned() else {
            return;
        };
        self.show_preview(note, window, cx);
        if let Some(area) = self.note().map(|note| note.area.clone()) {
            area.update(cx, |area, cx| area.reveal_mention(id, cx));
            area.update(cx, |input, cx| input.focus(window, cx));
        }
    }

    fn show_on_shelf(&mut self, id: &ItemId, cx: &mut Context<Self>) {
        let Some(item) = self.store.item(id).cloned() else {
            return self.fail(tr().mention_gone, cx);
        };
        self.navigate(Screen::Shelf, cx);
        self.clear_search(cx);
        if !self.shows(self.filter(), &item) {
            self.set_filter(Filter::All, cx);
        }
        let Some(row) = self.visible_ids().iter().position(|other| *other == item.id) else {
            return;
        };
        self.selection.reset();
        self.selection.toggle(&item.id);
        self.hover.step_to(Hovered::Item(item.id.clone()));
        self.reveal_row(row / self.rows.get().columns.max(1));
        cx.notify();
    }

    fn paste_into_note(&mut self, cx: &mut Context<Self>) {
        let storage = self.store.storage().clone();
        self.after_update_then(
            move |desktop| desktop.paste(&storage),
            |shelf, result, cx| match result {
                Ok(payload) => shelf.attach(payload, cx),
                Err(error) => shelf.fail(error.to_string(), cx),
            },
            cx,
        );
    }

    pub(super) fn take_drop(&mut self, payload: Payload, cx: &mut Context<Self>) {
        if self.note().is_some() {
            return self.attach(payload, cx);
        }
        log::debug!("Drop goes to the shelf: {}", payload.describe());
        self.ingest(payload, cx);
    }

    fn attach(&mut self, payload: Payload, cx: &mut Context<Self>) {
        log::debug!("Attaching to the note: {}", payload.describe());
        match payload {
            Payload::Paths(paths) => {
                let (bundles, paths): (Vec<_>, Vec<_>) = paths.into_iter().partition(|path| plugins::is_bundle(path));
                if !bundles.is_empty() {
                    self.install_plugins(bundles, cx);
                }
                self.store.add_paths(paths.clone());
                self.mention_paths(&paths, cx);
            }
            Payload::Stored(stored) => {
                let paths = stored.paths().to_vec();
                self.store.adopt(stored);
                self.mention_paths(&paths, cx);
            }
            Payload::Text(text) => {
                if let Some(area) = self.note().map(|note| note.area.clone()) {
                    area.update(cx, |area, cx| area.insert_text(&text, cx));
                }
            }
            Payload::Image(data) => {
                let storage = self.store.storage().clone();
                self.background(
                    cx,
                    move || store::save_image(&storage, data),
                    |shelf, result, cx| match result {
                        Ok((path, title)) => {
                            shelf.store.add_image(path.clone(), title);
                            shelf.mention_paths(&[path], cx);
                        }
                        Err(error) => shelf.fail(fill(tr().add_failed, &error), cx),
                    },
                );
            }
        }
    }

    fn mention_paths(&mut self, paths: &[PathBuf], cx: &mut Context<Self>) {
        let picks: Vec<(ItemId, String)> = paths
            .iter()
            .filter_map(|path| self.store.items().iter().find(|item| item.path == *path))
            .map(|item| (item.id.clone(), item.title.clone()))
            .collect();
        if let Some(area) = self.note().map(|note| note.area.clone()).filter(|_| self.page() == Page::Preview) {
            area.update(cx, |area, cx| area.insert(&picks, cx));
        }
        self.items_changed(cx);
    }
}
