use super::{Page, Shelf};
use crate::i18n::tr;
use crate::input::TextInput;
use crate::search::Query;
use crate::store::{Item, ItemId, Kind};
use gpui::*;

pub(super) struct PageSearch {
    input: Entity<TextInput>,
    current: usize,
    revealing: bool,
}

impl PageSearch {
    pub(super) fn new(input: Entity<TextInput>) -> Self {
        Self { input, current: 0, revealing: false }
    }

    pub(super) fn input(&self) -> &Entity<TextInput> {
        &self.input
    }
}

#[derive(Clone, Copy, PartialEq, Default)]
pub enum Filter {
    #[default]
    All,
    Files,
    Images,
    Text,
    Notes,
    Found,
}

impl Filter {
    pub const ALL: [Filter; 5] = [Filter::All, Filter::Files, Filter::Images, Filter::Text, Filter::Notes];

    pub fn label(self) -> &'static str {
        match self {
            Filter::All => tr().tab_all,
            Filter::Files => tr().tab_files,
            Filter::Images => tr().tab_images,
            Filter::Text => tr().tab_text,
            Filter::Notes => tr().tab_notes,
            Filter::Found => tr().tab_found,
        }
    }

    pub fn matches(self, item: &Item, separate_notes: bool) -> bool {
        match self {
            Filter::All => !(separate_notes && item.note()),
            Filter::Files => matches!(item.kind, Kind::File | Kind::Folder),
            Filter::Images => item.kind == Kind::Image,
            Filter::Text => matches!(item.kind, Kind::Text | Kind::Link) || (item.note() && !separate_notes),
            Filter::Notes => item.note(),
            Filter::Found => true,
        }
    }
}

#[derive(Default)]
pub(super) struct Tabs {
    chosen: Filter,
    found: bool,
}

impl Tabs {
    pub(super) fn pick(&mut self, filter: Filter) {
        self.found = filter == Filter::Found;
        if !self.found {
            self.chosen = filter;
        }
    }
}

impl Shelf {
    pub fn filter(&self) -> Filter {
        if self.tabs.found && self.catalog.searching() { Filter::Found } else { self.tabs.chosen }
    }

    pub(crate) fn shown_tabs(&self) -> impl Iterator<Item = (usize, Filter)> {
        let found = self.catalog.searching().then_some((Filter::ALL.len(), Filter::Found));
        found.into_iter().chain(Filter::ALL.into_iter().enumerate())
    }

    pub(super) fn search_started(&mut self) {
        self.tabs.pick(Filter::Found);
        self.reveal_tab();
    }

    pub fn found(&self, item: &Item) -> bool {
        self.catalog.found(&item.id)
    }

    pub fn clear_search(&mut self, cx: &mut Context<Self>) {
        if self.search.read(cx).text().is_empty() {
            return;
        }
        self.search.update(cx, |input, cx| input.set_text(String::new(), cx));
        self.catalog.clear_search();
        cx.notify();
    }

    pub(crate) fn clear_current_search(&mut self, cx: &mut Context<Self>) {
        if self.page() == Page::Shelf {
            return self.clear_search(cx);
        }
        self.clear_page_search(cx);
    }

    pub(super) fn clear_page_search(&mut self, cx: &mut Context<Self>) {
        self.find.revealing = false;
        self.find.input.update(cx, |input, cx| input.set_text(String::new(), cx));
        cx.notify();
    }

    pub(super) fn page_searched(&mut self, cx: &mut Context<Self>) {
        self.find.current = 0;
        self.find.revealing = !self.find.input.read(cx).text().trim().is_empty();
        cx.notify();
    }

    pub(crate) fn current_hit(&self) -> usize {
        self.find.current
    }

    pub fn step_hit(&mut self, forward: bool, cx: &mut Context<Self>) {
        let count = self.regions.hit_count();
        if count == 0 {
            return;
        }
        let current = self.find.current.min(count - 1);
        self.find.current = if forward { (current + 1) % count } else { (current + count - 1) % count };
        self.find.revealing = true;
        cx.notify();
    }

    pub(crate) fn reveal_after_frame(&self, window: &Window, cx: &mut Context<Self>) {
        if !self.find.revealing {
            return;
        }
        let shelf = cx.entity().downgrade();
        window.defer(cx, move |_, cx| {
            let _ = shelf.update(cx, |shelf, cx| shelf.guarded(cx, |shelf, cx| shelf.reveal_current_hit(cx)));
        });
    }

    fn reveal_current_hit(&mut self, cx: &mut Context<Self>) {
        if !std::mem::take(&mut self.find.revealing) {
            return;
        }
        let Some(hit) = self.regions.hit(self.find.current) else {
            return;
        };
        let scroll = self.scroll(self.page());
        let view = scroll.bounds();
        let offset = scroll.offset();
        let target = (offset.y + view.center().y - hit.center().y).clamp(-scroll.max_offset().y, px(0.));
        if target != offset.y {
            scroll.set_offset(point(offset.x, target));
            cx.notify();
        }
    }

    pub fn searchable(&self) -> bool {
        self.confirming.is_none()
            && match self.page() {
                Page::Shelf => !self.store.items().is_empty(),
                Page::Settings | Page::Actions => true,
                Page::Edit
                | Page::Preview
                | Page::Share
                | Page::Hashes
                | Page::Qr
                | Page::Rename
                | Page::Media
                | Page::Password => false,
            }
    }

    pub(crate) fn search_input(&self) -> &Entity<TextInput> {
        if self.page() == Page::Shelf { &self.search } else { &self.find.input }
    }

    pub(crate) fn page_query(&self, cx: &App) -> Option<Query> {
        Query::parse(self.find.input.read(cx).text())
    }

    pub fn shows(&self, filter: Filter, item: &Item) -> bool {
        filter.matches(item, self.store.settings().separate_notes)
    }

    pub fn visible_items(&self) -> Vec<&Item> {
        self.store.items().iter().filter(|i| self.shows(self.filter(), i) && self.found(i)).collect()
    }

    pub(super) fn visible_ids(&self) -> Vec<ItemId> {
        self.visible_items().iter().map(|item| item.id.clone()).collect()
    }

    pub(super) fn reveal_tab(&self) {
        if let Some(index) = self.shown_tabs().position(|(_, filter)| filter == self.filter()) {
            self.tab_strip.scroll_to_item(index);
        }
    }

    pub(super) fn clearable(&self) -> Filter {
        if self.filter() == Filter::Notes && self.store.settings().separate_notes { Filter::Notes } else { Filter::All }
    }
}
