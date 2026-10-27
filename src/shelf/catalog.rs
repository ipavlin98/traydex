use super::{Page, Shelf};
use crate::search::{Grade, Index, Indexed, Query};
use crate::store::{Item, ItemId, Kind, Layout, Meta, mention};
use gpui::*;
use std::collections::HashMap;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const THUMBNAIL: i32 = 256;

#[derive(Default)]
pub struct Catalog {
    meta: HashMap<ItemId, Meta>,
    thumbs: HashMap<PathBuf, Option<Arc<RenderImage>>>,
    index: Index,
    hits: Option<HashMap<ItemId, Vec<Range<usize>>>>,
    cited: HashMap<ItemId, Vec<ItemId>>,
    describing: Option<Task<()>>,
}

impl Catalog {
    pub fn citing(&self, id: &ItemId) -> &[ItemId] {
        self.cited.get(id).map(Vec::as_slice).unwrap_or_default()
    }

    pub(super) fn cite(&mut self, items: &[Item]) {
        self.cited.clear();
        for note in items.iter().filter(|item| item.note()) {
            for found in mention::mentions(note.text.as_deref().unwrap_or_default()) {
                let notes = self.cited.entry(found.id).or_default();
                if !notes.contains(&note.id) {
                    notes.push(note.id.clone());
                }
            }
        }
    }

    pub fn meta(&self, id: &ItemId) -> Option<&Meta> {
        self.meta.get(id)
    }

    pub fn exists(&self, id: &ItemId) -> bool {
        self.meta.get(id).is_none_or(|meta| meta.exists)
    }

    pub fn thumbnail(&self, path: &Path) -> Option<Arc<RenderImage>> {
        self.thumbs.get(path).cloned().flatten()
    }

    pub fn searching(&self) -> bool {
        self.hits.is_some()
    }

    pub fn found(&self, id: &ItemId) -> bool {
        self.hits.as_ref().is_none_or(|hits| hits.contains_key(id))
    }

    pub fn hits(&self, id: &ItemId) -> &[Range<usize>] {
        self.hits.as_ref().and_then(|hits| hits.get(id)).map(Vec::as_slice).unwrap_or_default()
    }

    fn search(&mut self, query: Option<Query>, items: &[Item]) {
        let Some(query) = query else {
            self.hits = None;
            return;
        };
        let scan = |grade: Grade| -> HashMap<ItemId, Vec<Range<usize>>> {
            items
                .iter()
                .filter_map(|item| Some((item.id.clone(), query.find(&item.title, self.index.get(&item.id), grade)?)))
                .collect()
        };
        self.hits = Some(Grade::ALL.into_iter().map(scan).find(|hits| !hits.is_empty()).unwrap_or_default());
    }

    pub(super) fn clear_search(&mut self) {
        self.hits = None;
    }

    pub(super) fn describe(&mut self, item: &Item) {
        self.meta.insert(item.id.clone(), item.describe());
    }

    fn claim_thumbnails(&mut self, paths: Vec<PathBuf>) -> Vec<PathBuf> {
        let wanted: Vec<PathBuf> = paths.into_iter().filter(|path| !self.thumbs.contains_key(path)).collect();
        for path in &wanted {
            self.thumbs.insert(path.clone(), None);
        }
        wanted
    }
}

impl Shelf {
    pub(crate) fn catalog(&self) -> &Catalog {
        &self.catalog
    }

    pub(super) fn update_hits(&mut self, cx: &App) {
        let query = Query::parse(self.search.read(cx).text());
        let searching = self.catalog.searching();
        self.catalog.search(query, self.store.items());
        if !searching && self.catalog.searching() {
            self.search_started();
        }
    }

    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        self.catalog.cite(self.store.items());
        self.index_contents(cx);
        let items = self.store.items().to_vec();
        let task = self.background_task(
            cx,
            move || Ok(items.iter().map(|item| (item.id.clone(), item.describe())).collect::<HashMap<_, _>>()),
            |shelf, meta, cx| {
                match meta {
                    Ok(meta) => shelf.catalog.meta = meta,
                    Err(error) => log::warn!("Couldn't describe the items: {error:#}"),
                }
                shelf.refresh_mentions(cx);
                shelf.load_thumbnails(cx);
                cx.notify();
            },
        );
        self.catalog.describing = Some(task);
    }

    pub(super) fn index_contents(&mut self, cx: &mut Context<Self>) {
        let Some(pass) = self.catalog.index.start(self.store.items()) else {
            return;
        };
        self.background(
            cx,
            move || Ok(pass.run()),
            |shelf, indexed, cx| {
                let indexed = indexed.inspect_err(|error| log::warn!("Couldn't index the contents: {error:#}")).ok();
                let changed = indexed.as_ref().is_some_and(Indexed::changed);
                if shelf.catalog.index.finish(indexed) {
                    shelf.index_contents(cx);
                }
                if changed && shelf.catalog.searching() {
                    shelf.update_hits(cx);
                    shelf.load_thumbnails(cx);
                    cx.notify();
                }
            },
        );
    }

    pub(super) fn load_thumbnails(&mut self, cx: &mut Context<Self>) {
        if self.page() != Page::Shelf || self.store.settings().layout != Layout::Grid {
            return;
        }
        let paths = self
            .visible_items()
            .into_iter()
            .filter(|item| matches!(item.kind, Kind::File | Kind::Folder))
            .filter(|item| self.catalog.meta(&item.id).is_some_and(|meta| meta.exists))
            .map(|item| item.path.clone())
            .collect();
        let wanted = self.catalog.claim_thumbnails(paths);
        if wanted.is_empty() {
            return;
        }
        let services = self.services.clone();
        self.background(
            cx,
            move || {
                Ok(wanted
                    .into_iter()
                    .map(|path| {
                        let image = services.thumbnail(&path, THUMBNAIL).and_then(|(width, height, pixels)| {
                            let buffer = image::RgbaImage::from_raw(width, height, pixels)?;
                            Some(Arc::new(RenderImage::new(vec![image::Frame::new(buffer)])))
                        });
                        (path, image)
                    })
                    .collect::<Vec<_>>())
            },
            |shelf, loaded, cx| {
                for (path, image) in loaded.unwrap_or_default() {
                    if let Some(slot) = shelf.catalog.thumbs.get_mut(&path) {
                        *slot = image;
                    }
                }
                cx.notify();
            },
        );
    }
}
