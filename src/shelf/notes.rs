use super::{Filter, Shelf};
use crate::i18n::tr;
use crate::input::{Changed, Submitted, TextInput};
use crate::store::{Item, ItemId};
use crate::textarea::{AreaEvent, Peek, TextArea};
use gpui::*;

fn placeholder(title: String) -> SharedString {
    if title.is_empty() { tr().new_note.into() } else { title.into() }
}

pub struct Note {
    pub area: Entity<TextArea>,
    pub title: Entity<TextInput>,
    pub picker: Option<Picker>,
    pub peek: Option<Peek>,
    pub(super) peeking: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

pub struct Picker {
    pub hits: Vec<ItemId>,
    pub chosen: usize,
    pub scroll: ScrollHandle,
}

impl Shelf {
    pub fn new_note(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.clear_search(cx);
        self.selection.clear();
        let key = self.store.add_note();
        if !self.store.items().iter().any(|item| item.id == key && self.shows(self.filter(), item)) {
            self.set_filter(Filter::Notes, cx);
        }
        self.items_changed(cx);
        self.show_preview(key, window, cx);
        if let Some(note) = self.note() {
            note.area.read(cx).focus(window);
        }
    }

    pub(super) fn note_for(&self, item: &Item, window: &mut Window, cx: &mut Context<Self>) -> Option<Note> {
        if !item.editable() {
            return None;
        }
        let text = item.text.clone().unwrap_or_default();
        let area = cx.new(|cx| TextArea::new(&text, tr().note_placeholder, cx));
        let custom = if item.auto_title() { String::new() } else { item.title.clone() };
        let placeholder = placeholder(item.title.clone());
        let title = cx.new(|cx| TextInput::new(custom, placeholder, false, cx).bare().fading(|t| t.bg));
        let subscriptions = vec![
            cx.subscribe(&area, |shelf, area, _: &Changed, cx| {
                shelf.guarded(cx, |shelf, cx| shelf.note_changed(area, cx))
            }),
            cx.subscribe(&title, |shelf, title, _: &Changed, cx| {
                shelf.guarded(cx, |shelf, cx| shelf.title_changed(title, cx))
            }),
            cx.subscribe_in(&title, window, |shelf, _, _: &Submitted, window, cx| shelf.focus_note(window, cx)),
            cx.subscribe_in(&area, window, |shelf, _, event: &AreaEvent, window, cx| {
                shelf.guarded(cx, |shelf, cx| shelf.area_event(event, window, cx))
            }),
        ];
        let looks = self.chip_looks(&area.read(cx).referenced());
        area.update(cx, |area, cx| area.set_looks(looks, cx));
        Some(Note { area, title, picker: None, peek: None, peeking: None, _subscriptions: subscriptions })
    }

    fn note_changed(&mut self, area: Entity<TextArea>, cx: &mut Context<Self>) {
        let Some(key) = self.previewed() else {
            return;
        };
        let before = self.title_of(&key);
        let text = area.read(cx).text();
        self.store.edit_text(&key, &text);
        let after = self.title_of(&key);
        if after != before
            && let Some(note) = self.note()
        {
            note.title.update(cx, |title, cx| {
                if !title.text().is_empty() {
                    title.set_text(String::new(), cx);
                }
                title.set_placeholder(placeholder(after), cx);
            });
        }
        self.note_edited(key, cx);
    }

    fn title_changed(&mut self, input: Entity<TextInput>, cx: &mut Context<Self>) {
        let Some(key) = self.previewed() else {
            return;
        };
        let text = input.read(cx).text().to_owned();
        if !self.store.rename(&key, &text) {
            return;
        }
        let title = self.title_of(&key);
        input.update(cx, |input, cx| input.set_placeholder(placeholder(title), cx));
        self.note_edited(key, cx);
    }

    fn title_of(&self, key: &ItemId) -> String {
        self.store.item(key).map(|item| item.title.clone()).unwrap_or_default()
    }

    fn note_edited(&mut self, key: ItemId, cx: &mut Context<Self>) {
        if let Some(item) = self.store.item(&key) {
            self.catalog.describe(item);
        }
        self.catalog.cite(self.store.items());
        self.update_hits(cx);
        self.index_contents(cx);
        cx.notify();
    }

    pub(super) fn note(&self) -> Option<&Note> {
        self.preview().and_then(|preview| preview.note.as_deref())
    }

    pub(super) fn note_mut(&mut self) -> Option<&mut Note> {
        self.preview_mut().and_then(|preview| preview.note.as_deref_mut())
    }

    pub(super) fn note_focused(&self, window: &Window, cx: &App) -> bool {
        self.note().is_some_and(|note| note.area.read(cx).focused(window))
    }

    pub(super) fn title_focused(&self, window: &Window, cx: &App) -> bool {
        self.note().is_some_and(|note| note.title.read(cx).focused(window))
    }

    pub(super) fn focus_title(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(title) = self.note().map(|note| note.title.clone()) {
            title.update(cx, |title, cx| title.select_all(cx));
            title.read(cx).focus(window);
        }
    }

    pub(super) fn focus_note(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(note) = self.note() {
            note.area.read(cx).focus(window);
        }
    }
}
