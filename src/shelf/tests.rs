use super::{Confirm, Dropdown, Filter, Hovered, JobKind, Launch, Page, Shelf, Target};
use crate::actions::{self, Action, Field, Single, Slot, TileId, Tool};
use crate::i18n::{Language, fill, tr};
use crate::keys::{Command, Shortcut};
use crate::preview::Content;
use crate::store::{self, Corner, ImageData, ItemId, Kind, Layout, Payload, ShakeLevel, Store};
use crate::system::{Event, Handler};
use crate::testing::{Call, Fake, Invocation, NARROW_SIZE, Sandbox, language, output, plant_tools, png, video_info};
use crate::textarea::TextArea;
use crate::theme::ThemeMode;
use futures::channel::mpsc::{self, UnboundedSender};
use gpui::{
    App, Context, Entity, EntityInputHandler, HoverTransition, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent,
    Pixels, Point, TestAppContext, VisualTestContext, Window, WindowBackgroundAppearance, point, px, size,
};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use windows::Win32::UI::Input::KeyboardAndMouse::*;

struct Harness<'a> {
    cx: &'a mut VisualTestContext,
    shelf: Entity<Shelf>,
    fake: Arc<Fake>,
    events: UnboundedSender<Event>,
    sandbox: Sandbox,
}

fn open(cx: &mut TestAppContext, visible: bool) -> Harness<'_> {
    open_with(cx, visible, |_, _, _| {})
}

fn open_with(cx: &mut TestAppContext, visible: bool, setup: impl FnOnce(&Sandbox, &mut Store, &Fake)) -> Harness<'_> {
    launch_with(cx, if visible { Launch::Active } else { Launch::Hidden }, setup)
}

fn launch_with(
    cx: &mut TestAppContext,
    launch: Launch,
    setup: impl FnOnce(&Sandbox, &mut Store, &Fake),
) -> Harness<'_> {
    let sandbox = Sandbox::new();
    let fake = Arc::new(Fake::new(&sandbox));
    let mut store = sandbox.store();
    setup(&sandbox, &mut store, &fake);
    let (events, rx) = mpsc::unbounded();
    let (desktop, services) = (fake.clone(), fake.clone());
    let (shelf, cx) =
        cx.add_window_view(move |window, cx| Shelf::new(store, rx, launch, desktop, services, window, cx));
    let mut harness = Harness { cx, shelf, fake, events, sandbox };
    harness.settle();
    harness
}

fn notes(texts: &'static [&'static str]) -> impl FnOnce(&Sandbox, &mut Store, &Fake) {
    move |_, store, _| {
        for text in texts.iter().rev() {
            store.add_text(text.to_string()).expect("note");
        }
    }
}

fn note_and_texts(note: &'static str, texts: &'static [&'static str]) -> impl FnOnce(&Sandbox, &mut Store, &Fake) {
    move |_, store, _| {
        for text in texts.iter().rev() {
            store.add_text(text.to_string()).expect("text");
        }
        let key = store.add_note();
        store.edit_text(&key, note);
    }
}

fn interned(name: &str) -> &'static str {
    static NAMES: Mutex<BTreeSet<&'static str>> = Mutex::new(BTreeSet::new());
    let mut names = NAMES.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(found) = names.get(name) {
        return found;
    }
    let leaked: &'static str = Box::leak(name.to_owned().into_boxed_str());
    names.insert(leaked);
    leaked
}

fn saved_tiles(h: &Harness) -> Vec<Action> {
    let mut tiles = actions::Tiles::new(h.sandbox.storage());
    tiles.load(&*h.fake).expect("tiles");
    tiles.actions().map(|tile| tile.action.clone()).collect()
}

fn shortcut(ctrl: bool, key: VIRTUAL_KEY) -> Shortcut {
    Shortcut { ctrl, key: key.0, ..Default::default() }
}

impl Harness<'_> {
    fn settle(&mut self) {
        loop {
            self.cx.run_until_parked();
            if self.fake.run_jobs() == 0 {
                break;
            }
        }
    }

    fn send(&mut self, event: Event) {
        self.events.unbounded_send(event).expect("send");
        self.settle();
    }

    fn act<R>(&mut self, f: impl FnOnce(&mut Shelf, &mut Window, &mut Context<Shelf>) -> R) -> R {
        let result = self.pending(f);
        self.settle();
        result
    }

    fn pending<R>(&mut self, f: impl FnOnce(&mut Shelf, &mut Window, &mut Context<Shelf>) -> R) -> R {
        let shelf = self.shelf.clone();
        let result = shelf.update_in(self.cx, |shelf, window, cx| {
            let result = f(shelf, window, cx);
            shelf.commit(cx);
            result
        });
        self.cx.run_until_parked();
        result
    }

    fn read<R>(&mut self, f: impl FnOnce(&Shelf) -> R) -> R {
        let shelf = self.shelf.clone();
        self.cx.update(|_, cx| f(shelf.read(cx)))
    }

    fn read_cx<R>(&mut self, f: impl FnOnce(&Shelf, &App) -> R) -> R {
        let shelf = self.shelf.clone();
        self.cx.update(|_, cx| f(shelf.read(cx), cx))
    }

    fn tile(&mut self, index: usize) -> TileId {
        self.read(|shelf| shelf.tiles.iter().nth(index).expect("tile").id)
    }

    fn tile_of(&mut self, kind: fn(&actions::Kind) -> bool) -> Option<TileId> {
        self.read(|shelf| shelf.tiles.iter().find(|tile| kind(&tile.action.kind)).map(|tile| tile.id))
    }

    fn editing(&mut self) -> Option<TileId> {
        self.read(|shelf| shelf.editor().map(|editor| editor.tile))
    }

    fn running(&mut self) -> usize {
        self.read_cx(|shelf, cx| shelf.jobs.read(cx).iter().count())
    }

    fn copied(&mut self) -> Option<String> {
        self.read_cx(|shelf, cx| shelf.notices.read(cx).copied().map(ToString::to_string))
    }

    fn toast(&mut self) -> Option<String> {
        self.read_cx(|shelf, cx| shelf.notices.read(cx).toast().map(ToString::to_string))
    }

    fn keys(&mut self, keystrokes: &str) {
        self.cx.simulate_keystrokes(keystrokes);
        self.settle();
    }

    fn elapse(&mut self, duration: Duration) {
        self.cx.executor().advance_clock(duration);
        self.settle();
    }

    fn ids(&mut self) -> Vec<ItemId> {
        self.read(|shelf| shelf.store.items().iter().map(|item| item.id.clone()).collect())
    }

    fn titles(&mut self) -> Vec<String> {
        self.read(|shelf| shelf.visible_items().iter().map(|item| item.title.clone()).collect())
    }

    fn selected(&mut self) -> Vec<String> {
        let mut selected: Vec<String> = self.read(|shelf| {
            shelf
                .store
                .items()
                .iter()
                .filter(|item| shelf.selection.contains(&item.id))
                .map(|item| item.title.clone())
                .collect()
        });
        selected.sort();
        selected
    }

    fn previewed(&mut self) -> Option<ItemId> {
        self.read(|shelf| shelf.preview().map(|preview| preview.key.clone()))
    }

    fn searching(&mut self) -> bool {
        let shelf = self.shelf.clone();
        self.cx.update(|window, cx| shelf.read(cx).search.read(cx).focused(window))
    }

    fn alert(&mut self) -> Option<String> {
        self.read_cx(|shelf, cx| shelf.notices.read(cx).alert().map(ToString::to_string))
    }

    fn saved(&self) -> Store {
        self.sandbox.store()
    }

    fn count(&self, call: &Call) -> usize {
        self.fake.calls().iter().filter(|c| *c == call).count()
    }

    fn activate(&mut self) {
        self.cx.update(|window, _| window.activate_window());
        self.settle();
    }

    fn deactivate(&mut self) {
        self.cx.deactivate_window();
        self.settle();
    }

    fn render(&mut self) {
        self.act(|shelf, window, cx| {
            let theme = shelf.theme();
            shelf.compose(theme, window, cx);
        });
    }

    fn min_size(&self) -> (f32, f32) {
        self.fake
            .calls()
            .into_iter()
            .rev()
            .find_map(|call| match call {
                Call::MinSize(width, height) => Some((width, height)),
                _ => None,
            })
            .expect("min size")
    }

    fn center(&mut self, mark: &str) -> Point<Pixels> {
        self.cx.debug_bounds(interned(mark)).unwrap_or_else(|| panic!("{mark} is rendered")).center()
    }

    fn order(&mut self) -> Vec<TileId> {
        self.read(|shelf| shelf.tiles.iter().map(|tile| tile.id).collect())
    }

    fn tile_center(&mut self, index: usize) -> (f32, f32) {
        let tile = self.tile(index);
        let center = self.read(|shelf| shelf.regions.tile(tile).expect("tile drawn")).center();
        (f32::from(center.x), f32::from(center.y))
    }
}

fn mouse_down(position: Point<Pixels>, click_count: usize) -> MouseDownEvent {
    MouseDownEvent {
        button: MouseButton::Left,
        position,
        modifiers: Modifiers::default(),
        click_count,
        first_mouse: false,
    }
}

fn mouse_move(position: Point<Pixels>, pressed: Option<MouseButton>) -> MouseMoveEvent {
    MouseMoveEvent { position, pressed_button: pressed, modifiers: Modifiers::default() }
}

fn add_tile(h: &mut Harness, kind: actions::Kind) -> TileId {
    h.act(|shelf, _, cx| shelf.add_action(Action::new(kind), cx)).expect("tile")
}

fn click(h: &mut Harness, mark: &'static str) {
    let position = h.cx.debug_bounds(mark).expect(mark).center();
    h.cx.simulate_click(position, Modifiers::default());
    h.settle();
}

fn fill_tiles(h: &mut Harness) {
    h.act(|shelf, _, cx| {
        let mut index = 0;
        while !shelf.tiles.full() {
            shelf.add_action(search(index), cx).expect("below the limit");
            index += 1;
        }
    });
}

fn search(index: usize) -> Action {
    Action::new(actions::Kind::Search { url: format!("https://find{index}.example/?q=%s") })
}

mod folders;
mod items;
mod jobs;
mod launch;
mod notes;
mod pages;
mod plugins;
mod saving;
mod session;
mod settings;
mod tiles;
mod unpack;
mod updates;
mod visual;
mod window;
