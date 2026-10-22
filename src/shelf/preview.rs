use super::notes::Note;
use super::{Page, Screen, Shelf, copy_id};
use crate::preview::{self, Content, Loaded};
use crate::store::ItemId;
use crate::system::{Embedded, Frame, Handler};
use crate::theme::Theme;
use gpui::*;
use std::cell::Cell;
use std::collections::HashSet;
use std::path::PathBuf;
use std::rc::Rc;

const OVERDRAW: f32 = 400.;

pub struct Preview {
    pub key: ItemId,
    pub loaded: Option<Loaded>,
    pub list: ListState,
    pub grid: UniformListScrollHandle,
    pub host: Option<Rc<dyn Embedded>>,
    pub frame: Rc<Cell<Option<Frame>>>,
    pub note: Option<Note>,
}

fn colorref(color: Rgba) -> u32 {
    let channel = |value: f32| (value.clamp(0., 1.) * 255.).round() as u32;
    channel(color.r) | channel(color.g) << 8 | channel(color.b) << 16
}

impl Shelf {
    pub(super) fn previewed(&self) -> Option<ItemId> {
        self.preview().map(|preview| preview.key.clone())
    }

    pub(super) fn toggle_preview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.page() == Page::Preview {
            return self.leave_preview(cx);
        }
        let target = self.hovered_item().or_else(|| {
            self.visible_items().into_iter().find(|item| self.selection.contains(&item.id)).map(|item| item.id.clone())
        });
        if let Some(key) = target {
            self.show_preview(key, window, cx);
        }
    }

    pub fn leave_preview(&mut self, cx: &mut Context<Self>) {
        if self.page() == Page::Preview {
            self.navigate(Screen::Shelf, cx);
        }
    }

    pub(super) fn close_preview(&mut self, preview: Preview, cx: &mut Context<Self>) {
        if preview.note.is_none() {
            return;
        }
        let blank = self
            .store
            .item(&preview.key)
            .is_some_and(|item| item.text.as_deref().is_some_and(|text| text.trim().is_empty()));
        if blank {
            self.store.remove(&HashSet::from([preview.key.clone()]));
            self.selection.remove(&preview.key);
            self.items_changed(cx);
        } else {
            self.flush_typing(cx);
        }
    }

    pub fn show_preview(&mut self, key: ItemId, window: &mut Window, cx: &mut Context<Self>) {
        if self.previewed().as_ref() == Some(&key) {
            return;
        }
        let Some(item) = self.store.item(&key).cloned() else {
            return;
        };
        let note = self.note_for(&item, window, cx);
        self.show(
            Screen::Preview(Preview {
                key: key.clone(),
                loaded: None,
                list: ListState::new(0, ListAlignment::Top, px(OVERDRAW)),
                grid: UniformListScrollHandle::new(),
                host: None,
                frame: Rc::default(),
                note,
            }),
            cx,
        );
        let path = item.path.clone();
        let services = self.services.clone();
        self.background(
            cx,
            move || Ok(preview::load(&item, &*services)),
            move |shelf, result, cx| {
                let loaded = result.unwrap_or_else(|error| {
                    log::warn!("Couldn't load the preview: {error:#}");
                    Loaded::failed()
                });
                shelf.loaded(key, path, loaded, cx);
            },
        );
    }

    fn loaded(&mut self, key: ItemId, path: PathBuf, loaded: Loaded, cx: &mut Context<Self>) {
        let Some(preview) = self.preview_of(&key) else {
            return;
        };
        preview.list.reset(loaded.count());
        let shell = match loaded.content {
            Content::Shell(handler) => Some(handler),
            _ => None,
        };
        preview.loaded = Some(loaded);
        if let Some(handler) = shell {
            self.attach_host(key, handler, path, cx);
        }
        cx.notify();
    }

    fn preview_of(&mut self, key: &ItemId) -> Option<&mut Preview> {
        self.preview_mut().filter(|preview| preview.key == *key)
    }

    fn attach_host(&mut self, key: ItemId, handler: Handler, path: PathBuf, cx: &mut Context<Self>) {
        let Some(preview) = self.preview() else {
            return;
        };
        let frame = preview.frame.clone();
        let t = *cx.global::<Theme>();
        let source = path.clone();
        self.after_update_then(
            move |desktop| {
                let rect = frame.get().unwrap_or_default();
                desktop.embed(handler, &source, rect, colorref(t.bg), colorref(t.text))
            },
            move |shelf, opened, cx| {
                let Some(preview) = shelf.preview_of(&key) else {
                    return;
                };
                match opened {
                    Ok(host) => {
                        if let Some(rect) = preview.frame.get() {
                            host.place(rect);
                        }
                        preview.host = Some(Rc::from(host));
                    }
                    Err(error) => {
                        log::warn!("The preview handler failed, showing a thumbnail: {error:#}");
                        let detail = preview.loaded.as_ref().map(|l| l.detail.clone()).unwrap_or_default();
                        let source = path.clone();
                        let services = shelf.services.clone();
                        shelf.background(
                            cx,
                            move || Ok(preview::fallback(&source, detail, &*services)),
                            move |shelf, result, cx| {
                                shelf.loaded(key, path, result.unwrap_or_else(|_| Loaded::failed()), cx)
                            },
                        );
                    }
                }
                cx.notify();
            },
            cx,
        );
    }

    pub fn step_preview(&mut self, delta: isize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(key) = self.previewed() else {
            return;
        };
        let order = self.visible_ids();
        let next = order
            .iter()
            .position(|id| *id == key)
            .and_then(|index| index.checked_add_signed(delta))
            .and_then(|index| order.get(index));
        if let Some(next) = next {
            self.show_preview(next.clone(), window, cx);
        }
    }

    pub fn open_previewed(&mut self, reveal: bool, cx: &mut Context<Self>) {
        if let Some(key) = self.previewed() {
            self.open(&key, reveal, cx);
        }
    }

    pub fn copy_previewed(&mut self, cx: &mut Context<Self>) {
        let Some(key) = self.previewed() else {
            return;
        };
        if let Some(item) = self.store.item(&key).cloned() {
            self.copy(vec![item], copy_id(&key), cx);
        }
    }

    pub(super) fn remove_previewed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(key) = self.previewed() else {
            return;
        };
        if !self.ask_remove(&HashSet::from([key]), cx) {
            self.drop_previewed(window, cx);
        }
    }

    pub(super) fn drop_previewed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(key) = self.previewed() else {
            return;
        };
        let order = self.visible_ids();
        let index = order.iter().position(|id| *id == key);
        self.drop_items(&HashSet::from([key]), cx);
        let next = index.and_then(|index| order.get(index + 1).or_else(|| order.get(index.checked_sub(1)?)));
        match next {
            Some(next) => self.show_preview(next.clone(), window, cx),
            None => self.leave_preview(cx),
        }
    }

    pub(super) fn reclaim_focus(&self, cx: &mut Context<Self>) {
        if self.preview().is_some_and(|preview| preview.host.is_some()) {
            self.after_update(|desktop| desktop.focus(), cx);
        }
    }
}
