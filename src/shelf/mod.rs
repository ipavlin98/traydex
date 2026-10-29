mod catalog;
mod fade;
mod filter;
mod folders;
mod hover;
mod items;
mod jobs;
mod keyboard;
mod launch;
mod media;
mod mentions;
mod notes;
mod notices;
mod plugins;
mod preview;
mod results;
mod saving;
mod screen;
mod selection;
mod session;
mod settings;
mod tasks;
mod tiles;
mod unpack;
mod updates;

use crate::actions::{TileId, Tiles, Tool};
use crate::i18n::{fill, tr};
use crate::input::{Changed, TextInput};
use crate::keys::Command;
use crate::store::{ItemId, Store};
use crate::system::{Desktop, Event, Services};
use crate::theme::Theme;
use futures::StreamExt;
use futures::channel::mpsc::UnboundedReceiver;
use gpui::*;
use std::cell::Cell;
use std::sync::Arc;

pub(crate) use catalog::Catalog;
pub use fade::{Fade, FadedOut};
pub use filter::Filter;
use filter::{PageSearch, Tabs};
use folders::{Folders, Scan};
pub(crate) use hover::Hover;
pub(crate) use items::Rows;
use jobs::Ended;
pub use jobs::{Job, JobId, JobKind, Jobs};
pub use notices::{Notice, Notices, Place};
pub use preview::Preview;
pub use results::QR_COPY;
use saving::Saving;
pub use screen::{Attempt, Confirm, Editor, Fetching, Page, Qr, Renaming, Screen, Share, Unlocking, Waiting};
pub(crate) use selection::Selection;
pub use session::Launch;
pub(crate) use session::{Regions, Session};
pub use settings::Dropdown;
pub(crate) use settings::{Menu, Recorder};
pub use tasks::SafeListener;
pub use tiles::{Edge, Lane};
use updates::{Check, Due, Updates};

pub const SHARE_COPY: &str = "share-copy";
pub const COPY_SELECTED: &str = "copy-selected";

pub fn copy_id(key: &ItemId) -> SharedString {
    format!("copy-{key}").into()
}

#[derive(Clone, PartialEq)]
pub enum Hovered {
    Item(ItemId),
    Tile(TileId),
    DragAll,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Target {
    Shelf,
    Tile(TileId),
    Cancel,
}

pub struct Shelf {
    store: Store,
    tiles: Tiles,
    desktop: Arc<dyn Desktop>,
    services: Arc<dyn Services>,
    screen: Screen,
    tabs: Tabs,
    selection: Selection,
    hover: Hover,
    search: Entity<TextInput>,
    find: PageSearch,
    catalog: Catalog,
    saving: Saving,
    session: Session,
    regions: Regions,
    notices: Entity<Notices>,
    jobs: Entity<Jobs>,
    updates: Entity<Updates>,
    folders: Entity<Folders>,
    fade: Entity<Fade>,
    confirming: Option<Confirm>,
    menu: Menu,
    recorder: Recorder,
    appearance: WindowAppearance,
    backdrop: WindowBackgroundAppearance,
    focus: FocusHandle,
    scrolls: [ScrollHandle; 11],
    tab_strip: ScrollHandle,
    tile_strip: ScrollHandle,
    fitting_width: Cell<f32>,
    rows: Cell<Rows>,
    _subscriptions: Vec<Subscription>,
}

impl Shelf {
    pub fn new(
        store: Store,
        events: UnboundedReceiver<Event>,
        launch: Launch,
        desktop: Arc<dyn Desktop>,
        services: Arc<dyn Services>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        cx.spawn(async move |this, cx| {
            let mut events = events;
            while let Some(event) = events.next().await {
                let handled = this.update(cx, |shelf, cx| shelf.guarded(cx, |shelf, cx| shelf.handle(event, cx)));
                if handled.is_err() {
                    break;
                }
            }
        })
        .detach();
        let search = cx.new(|cx| TextInput::new(String::new(), tr().search_placeholder, false, cx).bare());
        let find = cx.new(|cx| TextInput::new(String::new(), tr().search_placeholder, false, cx).bare());
        let notices = cx.new(|_| Notices::default());
        let jobs = cx.new(|_| Jobs::default());
        let updates = cx.new(|_| Updates::default());
        let folders = cx.new(|_| Folders::default());
        let fade = cx.new(|_| if store.settings().window_animations { Fade::out() } else { Fade::default() });
        let subscriptions = vec![
            cx.observe_window_appearance(window, |shelf, window, cx| {
                shelf.appearance = window.appearance();
                shelf.apply_theme(cx);
            }),
            cx.observe_window_activation(window, |shelf, window, cx| {
                log::debug!(
                    "Window active: {}, input focus: {}",
                    window.is_window_active(),
                    shelf.focused_input(window, cx)
                );
                if window.is_window_active() {
                    shelf.detect_tools(cx);
                    shelf.find_recycled(cx);
                    shelf.focus_password(window, cx);
                } else {
                    shelf.stop_recording(cx);
                    shelf.guarded(cx, |shelf, cx| shelf.dismiss(false, cx));
                }
            }),
            cx.on_focus_lost(window, |shelf, window, cx| {
                log::debug!("Input focus lost, returning it to the shelf");
                window.focus(&shelf.focus, cx);
            }),
            cx.subscribe(&search, |shelf, _, _: &Changed, cx| {
                shelf.selection.reset();
                shelf.update_hits(cx);
                shelf.load_thumbnails(cx);
                cx.notify();
            }),
            cx.subscribe(&find, |shelf, _, _: &Changed, cx| shelf.page_searched(cx)),
            cx.observe(&notices, |_, _, cx| cx.notify()),
            cx.observe(&jobs, |_, _, cx| cx.notify()),
            cx.observe(&updates, |_, _, cx| cx.notify()),
            cx.subscribe(&updates, |shelf, _, _: &Due, cx| shelf.guarded(cx, |shelf, cx| shelf.check(Check::Auto, cx))),
            cx.subscribe(&folders, |shelf, _, _: &Scan, cx| shelf.guarded(cx, |shelf, cx| shelf.scan_folders(cx))),
            cx.observe_in(&fade, window, |shelf, _, window, cx| shelf.follow_fade(window, cx)),
            cx.subscribe(&fade, |shelf, _, _: &FadedOut, cx| shelf.guarded(cx, |shelf, cx| shelf.hidden(cx))),
        ];
        let mut shelf = Self {
            tiles: Tiles::new(store.storage().clone()),
            store,
            desktop,
            services,
            screen: Screen::Shelf,
            tabs: Tabs::default(),
            selection: Selection::default(),
            hover: Hover::default(),
            search,
            find: PageSearch::new(find),
            catalog: Catalog::default(),
            saving: Saving::default(),
            session: Session::default(),
            regions: Regions::default(),
            notices,
            jobs,
            updates,
            folders,
            fade,
            confirming: None,
            menu: Menu::default(),
            recorder: Recorder::default(),
            appearance: window.appearance(),
            backdrop: WindowBackgroundAppearance::Opaque,
            focus,
            scrolls: Default::default(),
            tab_strip: ScrollHandle::new(),
            tile_strip: ScrollHandle::new(),
            fitting_width: Cell::default(),
            rows: Cell::default(),
            _subscriptions: subscriptions,
        };
        shelf.sync_settings(None, cx);
        shelf.start(launch, cx);
        shelf
    }

    fn start(&mut self, launch: Launch, cx: &mut Context<Self>) {
        let position = self.store.settings().position;
        let (width, height) = self.store.settings().window_size();
        self.after_update(
            move |desktop| {
                desktop.prepare(width, height);
                desktop.place(position);
            },
            cx,
        );
        match launch {
            Launch::Active => self.appear(true, cx),
            Launch::Inactive => self.appear(false, cx),
            Launch::Hidden => {}
        }
        Tool::sweep(self.store.storage());
        if let Some(trouble) = self.store.take_trouble() {
            self.fail(trouble, cx);
        }
        if let Err(error) = self.tiles.load(&*self.services) {
            self.fail(fill(tr().actions_failed, &error), cx);
        }
        self.detect_tools(cx);
        self.schedule_scans(cx);
        self.fit(cx);
        self.refresh(cx);
    }

    pub fn theme(&self) -> Theme {
        Theme::resolve(self.store.settings().theme, self.appearance)
    }

    fn apply_theme(&self, cx: &mut Context<Self>) {
        cx.set_global(self.theme());
        cx.notify();
    }

    fn handle(&mut self, event: Event, cx: &mut Context<Self>) {
        match event {
            Event::DragStart => self.drag_started(cx),
            Event::DragToCorner => self.drag_reached_corner(cx),
            Event::DragEnd => self.drag_ended(cx),
            Event::CancelDrag => self.cancel_drag(cx),
            Event::Hover(hovering) => self.hover_changed(hovering, cx),
            Event::Offer(offer) => self.offered(offer, cx),
            Event::Pointer(x, y) => self.pointer_moved(point(px(x), px(y)), cx),
            Event::DropBack => self.dropped_back(cx),
            Event::ExportEnd => self.lend_ended(cx),
            Event::Drop(payload) => {
                if self.session.automatic() {
                    log::debug!("Dropped into the window, it stays until a click outside");
                    self.session.end();
                }
                if self.regions.zone().is_none() {
                    return self.take_drop(payload, cx);
                }
                match self.dropped_target() {
                    Some(Target::Tile(tile)) => self.perform(tile, payload, cx),
                    Some(Target::Shelf) => self.take_drop(payload, cx),
                    Some(Target::Cancel) | None => log::debug!("Drop outside of any tile, ignored"),
                }
            }
            Event::Receive(paths) => {
                self.session.end();
                let count = self.store.add_paths(paths);
                self.added(count, cx);
                self.reveal(cx);
            }
            Event::Placed { x, y, width, height } => {
                self.session.end();
                let settings = self.store.settings_mut();
                settings.position = Some((x, y));
                settings.size = Some((width, height));
            }
            Event::ResetSize => self.reset_size(cx),
            Event::FitSize => self.fit_size(cx),
            Event::Toggle | Event::Hotkey(Command::Toggle) | Event::Corner => {
                self.session.end();
                if self.shown(cx) {
                    self.hide("toggled by the hotkey, the tray icon or the hot corner", cx);
                } else if !matches!(event, Event::Corner) && self.recently_dismissed(cx) {
                    log::debug!("Toggle ignored: the window was just hidden by the same click");
                } else {
                    self.reveal(cx);
                }
            }
            Event::PressOutside => self.dismiss(true, cx),
            Event::Hotkey(command) => log::debug!("No system action for {command:?}"),
            Event::Recording(held) => self.holding(held, cx),
            Event::Recorded(shortcut) => self.recorded(shortcut, cx),
            Event::Reveal => {
                self.session.end();
                self.reveal(cx);
            }
            Event::Escape => {
                if self.page() == Page::Preview {
                    self.leave_preview(cx);
                }
                self.after_update(|desktop| desktop.focus(), cx);
            }
            Event::Quit => self.quit(cx),
            Event::Failure(text) => self.fail(text, cx),
        }
    }

    fn hovered_tile(&self) -> Option<TileId> {
        match self.hover.target() {
            Some(&Hovered::Tile(tile)) if self.page() == Page::Actions && self.tiles.get(tile).is_some() => Some(tile),
            _ => None,
        }
    }

    pub(crate) fn confirming(&self) -> Option<&Confirm> {
        self.confirming.as_ref()
    }

    pub fn confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.confirming.take() {
            Some(Confirm::Clear) => self.clear(cx),
            Some(Confirm::Reset) => self.reset_settings(cx),
            Some(Confirm::Uninstall(id)) => self.uninstall_plugin(&id, cx),
            Some(Confirm::Update(release)) => self.install_update(release, cx),
            Some(Confirm::Remove(keys)) => match self.previewed() {
                Some(key) if keys.contains(&key) => self.drop_previewed(window, cx),
                _ => self.drop_items(&keys, cx),
            },
            None => {}
        }
    }

    pub fn ask_clear(&mut self, cx: &mut Context<Self>) {
        if self.store.items().iter().any(|item| self.clears(item)) {
            self.confirming = Some(Confirm::Clear);
            cx.notify();
        }
    }

    pub fn ask_reset(&mut self, cx: &mut Context<Self>) {
        self.confirming = Some(Confirm::Reset);
        cx.notify();
    }

    pub fn cancel_confirm(&mut self, cx: &mut Context<Self>) {
        if let Some(Confirm::Update(release)) = self.confirming.take() {
            self.decline_update(release, cx);
        }
        cx.notify();
    }

    pub fn dropping(&self) -> bool {
        (self.session.look().active || self.session.lending()) && !self.tiles.is_empty()
    }

    pub fn fail(&mut self, text: impl Into<SharedString>, cx: &mut Context<Self>) {
        let text = text.into();
        log::error!("{text}");
        self.notify(Notice::Alert, text, cx);
    }

    fn show_toast(&mut self, text: String, cx: &mut Context<Self>) {
        self.notify(Notice::Toast, text.into(), cx);
    }

    fn notify(&mut self, notice: Notice, text: SharedString, cx: &mut Context<Self>) {
        let place = self.notice_place();
        self.notices.update(cx, |notices, cx| notices.show(notice, text, place, cx));
    }

    pub(crate) fn notices<'a>(&self, cx: &'a App) -> &'a Notices {
        self.notices.read(cx)
    }

    pub fn notice_place(&self) -> Place {
        if self.page() == Page::Shelf { Place::Title } else { Place::Bottom }
    }

    pub fn dismiss_notice(&mut self, notice: Notice, cx: &mut Context<Self>) {
        self.notices.update(cx, |notices, cx| notices.dismiss(notice, cx));
    }

    pub fn hold_notice(&mut self, notice: Notice, held: bool, cx: &mut Context<Self>) {
        self.notices.update(cx, |notices, cx| notices.hold(notice, held, cx));
    }

    fn flash_copied(&mut self, origin: SharedString, cx: &mut Context<Self>) {
        self.notices.update(cx, |notices, cx| notices.flash(origin, cx));
    }

    fn items_changed(&mut self, cx: &mut Context<Self>) {
        self.update_hits(cx);
        self.refresh(cx);
        self.refresh_mentions(cx);
    }
}

#[cfg(test)]
mod tests;
