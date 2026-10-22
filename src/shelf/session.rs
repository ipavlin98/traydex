use super::{Screen, Shelf, Target};
use crate::actions::TileId;
use crate::design;
use crate::store::Item;
use crate::system::{Caption, Offer};
use gpui::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

const POLL: Duration = Duration::from_millis(30);
const DISMISS_GRACE: Duration = Duration::from_millis(500);
const DRAG_THRESHOLD: Pixels = px(5.);

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Launch {
    Active,
    Inactive,
    Hidden,
}

#[derive(Clone, Copy)]
pub struct Look {
    pub active: bool,
    pub hovering: bool,
    pub over: Option<Target>,
}

#[derive(Default)]
pub struct Session {
    dragging: bool,
    hovering: bool,
    lingering: bool,
    pointer: Option<Point<Pixels>>,
    over: Option<Target>,
    offer: Option<Offer>,
    pressed: Option<(Vec<Item>, Point<Pixels>)>,
    lent: Option<Vec<Item>>,
    automatic: bool,
    picking: bool,
    dismissed: Option<Instant>,
    displaced: bool,
    frozen: Option<Look>,
    task: Option<Task<()>>,
}

impl Session {
    pub fn end(&mut self) {
        if self.automatic {
            log::debug!("Automatic session ended");
        }
        self.automatic = false;
        self.task = None;
    }

    fn reset(&mut self) {
        self.end();
        self.dragging = false;
        self.hovering = false;
        self.lingering = false;
        self.pointer = None;
        self.over = None;
        self.pressed = None;
    }

    pub fn automatic(&self) -> bool {
        self.automatic
    }

    pub fn offer(&self) -> Option<&Offer> {
        self.offer.as_ref()
    }

    pub(super) fn press(&mut self, items: Vec<Item>, at: Point<Pixels>) {
        self.pressed = Some((items, at));
    }

    pub(super) fn release(&mut self) {
        self.pressed = None;
    }

    pub(super) fn pull(&mut self, event: &MouseMoveEvent) -> Option<Vec<Item>> {
        let (_, origin) = self.pressed.as_ref()?;
        if event.pressed_button != Some(MouseButton::Left) {
            self.pressed = None;
            return None;
        }
        let delta = event.position - *origin;
        if delta.x.abs() + delta.y.abs() < DRAG_THRESHOLD {
            return None;
        }
        self.pressed.take().map(|(items, _)| items)
    }

    pub(super) fn lend(&mut self, items: Vec<Item>) {
        self.lent = Some(items);
    }

    pub(super) fn returned(&mut self) -> Vec<Item> {
        self.offer = None;
        self.lent.take().unwrap_or_default()
    }

    pub(super) fn lend_ended(&mut self) {
        self.lent = None;
        self.offer = None;
        self.over = None;
    }

    pub fn look(&self) -> Look {
        self.frozen.unwrap_or(Look {
            active: self.dragging || self.hovering || self.lingering,
            hovering: self.hovering,
            over: self.over,
        })
    }

    fn freeze(&mut self) {
        self.frozen = Some(self.look());
    }

    fn thaw(&mut self) {
        self.frozen = None;
    }

    #[cfg(test)]
    pub(super) fn watching(&self) -> bool {
        self.task.is_some()
    }

    #[cfg(test)]
    pub fn dragging(&self) -> bool {
        self.dragging
    }

    #[cfg(test)]
    pub(super) fn lingering(&self) -> bool {
        self.lingering
    }

    #[cfg(test)]
    pub fn pointer(&self) -> Option<Point<Pixels>> {
        self.pointer
    }

    #[cfg(test)]
    pub(super) fn picking(&self) -> bool {
        self.picking
    }

    #[cfg(test)]
    pub(super) fn set_picking(&mut self, picking: bool) {
        self.picking = picking;
    }

    pub fn lending(&self) -> bool {
        self.lent.is_some()
    }

    pub(super) fn just_dismissed(&self, now: Instant) -> bool {
        self.dismissed.is_some_and(|at| now.saturating_duration_since(at) < DISMISS_GRACE)
    }
}

type TileBounds = Vec<(Bounds<Pixels>, TileId)>;

#[derive(Clone, Default)]
pub struct Regions {
    tiles: Rc<RefCell<TileBounds>>,
    zone: Rc<Cell<Option<Bounds<Pixels>>>>,
    cancel: Rc<Cell<Option<Bounds<Pixels>>>>,
    hits: Rc<RefCell<Vec<Bounds<Pixels>>>>,
}

impl Regions {
    pub fn clear(&self) {
        self.tiles.borrow_mut().clear();
        self.zone.set(None);
        self.cancel.set(None);
        self.hits.borrow_mut().clear();
    }

    pub fn add_hit(&self, bounds: Bounds<Pixels>) {
        self.hits.borrow_mut().push(bounds);
    }

    pub(super) fn first_hit(&self) -> Option<Bounds<Pixels>> {
        self.hits
            .borrow()
            .iter()
            .copied()
            .min_by(|a, b| (a.top(), a.left()).partial_cmp(&(b.top(), b.left())).unwrap_or(std::cmp::Ordering::Equal))
    }

    pub fn set_cancel(&self, bounds: Bounds<Pixels>) {
        self.cancel.set(Some(bounds));
    }

    pub fn add_tile(&self, bounds: Bounds<Pixels>, id: TileId) {
        self.tiles.borrow_mut().push((bounds, id));
    }

    pub fn set_zone(&self, bounds: Bounds<Pixels>) {
        self.zone.set(Some(bounds));
    }

    pub fn zone(&self) -> Option<Bounds<Pixels>> {
        self.zone.get()
    }

    #[cfg(test)]
    pub fn tile(&self, id: TileId) -> Option<Bounds<Pixels>> {
        self.tiles.borrow().iter().find(|(_, tile)| *tile == id).map(|(bounds, _)| *bounds)
    }

    pub fn tiles(&self) -> Vec<(Bounds<Pixels>, TileId)> {
        self.tiles.borrow().clone()
    }

    pub(super) fn target_at(&self, position: Point<Pixels>) -> Option<Target> {
        if self.cancel.get().is_some_and(|bounds| bounds.contains(&position)) {
            return Some(Target::Cancel);
        }
        let tile =
            self.tiles.borrow().iter().find(|(bounds, _)| bounds.contains(&position)).map(|(_, id)| Target::Tile(*id));
        tile.or_else(|| self.zone.get().filter(|bounds| bounds.contains(&position)).map(|_| Target::Shelf))
    }
}

impl Shelf {
    pub(crate) fn session(&self) -> &Session {
        &self.session
    }

    pub(crate) fn regions(&self) -> &Regions {
        &self.regions
    }

    pub(crate) fn caption_sink(&self) -> impl Fn(Caption) + 'static {
        let desktop = self.desktop.clone();
        move |caption| desktop.set_caption(caption)
    }

    pub(crate) fn zoom(&self) -> f32 {
        design::zoom(self.store.settings().font_size)
    }

    pub(crate) fn sync_window(&mut self, window: &mut Window, cx: &App) {
        let fade = self.fade.read(cx);
        if !fade.animating() {
            window.set_opacity(self.store.settings().opacity() * fade.level(cx.background_executor().now()));
        }
        window.set_rem_size(px(design::BASE * self.zoom()));
        let backdrop = self.store.settings().backdrop();
        if self.backdrop != backdrop {
            self.backdrop = backdrop;
            window.set_background_appearance(backdrop);
        }
    }

    pub(super) fn dialog<T: Send + 'static>(
        &mut self,
        cx: &mut Context<Self>,
        job: impl FnOnce() -> anyhow::Result<T> + Send + 'static,
        done: impl FnOnce(&mut Self, anyhow::Result<T>, &mut Context<Self>) + 'static,
    ) {
        self.session.end();
        self.session.picking = true;
        self.background(cx, job, move |shelf, result, cx| {
            shelf.session.picking = false;
            done(shelf, result, cx);
        });
    }

    pub(super) fn reveal(&mut self, cx: &mut Context<Self>) {
        self.session.lingering = false;
        self.return_home(cx);
        self.refresh(cx);
        self.appear(true, cx);
    }

    fn near_cursor(&self) -> bool {
        self.store.settings().automatic && self.store.settings().near_cursor
    }

    fn return_home(&mut self, cx: &mut Context<Self>) {
        if !self.session.displaced || self.desktop.visible() {
            return;
        }
        self.session.displaced = false;
        let position = self.store.settings().position;
        self.after_update(move |desktop| desktop.place(position), cx);
    }

    pub(super) fn shown(&self, cx: &App) -> bool {
        self.desktop.visible() && !self.fade.read(cx).hiding()
    }

    pub(super) fn follow_fade(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let fade = self.fade.read(cx);
        window.set_opacity(self.store.settings().opacity() * fade.level(cx.background_executor().now()));
        if fade.animating() {
            let this = cx.entity().downgrade();
            window.on_next_frame(move |window, cx| {
                let _ = this.update(cx, |shelf, cx| shelf.guarded(cx, |shelf, cx| shelf.follow_fade(window, cx)));
            });
        }
    }

    fn fades(&self) -> bool {
        self.store.settings().window_animations && self.preview().is_none_or(|preview| preview.host.is_none())
    }

    pub(super) fn appear(&mut self, activate: bool, cx: &mut Context<Self>) {
        log::debug!("Showing the window, activate: {activate}");
        self.session.thaw();
        self.find_recycled(cx);
        if self.fades() {
            let from_zero = !self.desktop.visible();
            self.fade.update(cx, |fade, cx| fade.fade_in(from_zero, cx));
        } else {
            self.fade.update(cx, |fade, cx| fade.reset(cx));
        }
        self.after_update(move |desktop| desktop.show(activate), cx);
    }

    pub(super) fn drag_started(&mut self, cx: &mut Context<Self>) {
        self.session.offer = None;
        self.session.dragging = true;
        self.session.lingering = false;
        if !self.store.settings().automatic {
            log::debug!("Drag started, auto-show is off");
            return;
        }
        self.show_drop_targets(cx);
    }

    pub(super) fn drag_reached_corner(&mut self, cx: &mut Context<Self>) {
        log::debug!("Drag reached the hot corner");
        self.session.offer = None;
        self.session.dragging = true;
        self.session.lingering = false;
        self.show_drop_targets(cx);
    }

    fn show_drop_targets(&mut self, cx: &mut Context<Self>) {
        if !matches!(self.screen, Screen::Shelf) {
            log::debug!("Drag started, leaving the open page for the drop targets");
        }
        self.navigate(Screen::Shelf, cx);
        self.confirming = None;
        self.menu.close();
        if self.shown(cx) {
            log::debug!("Drag started, the window is already visible");
        } else {
            log::debug!("Drag started, showing the window without focus");
            self.session.automatic = true;
            self.refresh(cx);
            if self.near_cursor() && !self.desktop.visible() {
                self.session.displaced = true;
                self.after_update(|desktop| desktop.place_near_cursor(), cx);
            }
            self.appear(false, cx);
        }
        if self.session.automatic {
            self.watch_session(cx);
        }
        cx.notify();
    }

    pub(super) fn drag_ended(&mut self, cx: &mut Context<Self>) {
        let inside = self.desktop.pointer_inside();
        log::debug!("Drag ended, pointer inside the window: {inside}");
        self.session.dragging = false;
        self.session.hovering = false;
        self.session.lingering = self.session.automatic && !inside;
        cx.notify();
        if self.session.automatic {
            self.watch_session(cx);
        }
    }

    pub(super) fn hover_changed(&mut self, hovering: bool, cx: &mut Context<Self>) {
        self.session.hovering = hovering;
        if hovering {
            self.navigate(Screen::Shelf, cx);
            self.confirming = None;
        } else {
            self.session.pointer = None;
            self.session.over = None;
            self.desktop.set_reject(false);
        }
        cx.notify();
    }

    pub(super) fn pointer_moved(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        self.session.pointer = Some(position);
        let over = self.regions.target_at(position);
        let exporting = self.desktop.exporting();
        let lands = match over {
            Some(Target::Tile(tile)) => !self.refuses(tile),
            Some(Target::Shelf) => !exporting,
            Some(Target::Cancel) => exporting,
            None => false,
        };
        self.desktop.set_reject((exporting || self.regions.zone().is_some()) && !lands);
        if over != self.session.over {
            self.session.over = over;
            cx.notify();
        }
    }

    pub(super) fn offered(&mut self, offer: Offer, cx: &mut Context<Self>) {
        self.session.offer = Some(offer);
        cx.notify();
    }

    pub(super) fn dropped_target(&self) -> Option<Target> {
        self.session.pointer.and_then(|position| self.regions.target_at(position))
    }

    pub(super) fn dismiss(&mut self, pressed: bool, cx: &mut Context<Self>) {
        if !self.shown(cx) || self.store.settings().pinned || self.session.picking || self.desktop.exporting() {
            return;
        }
        if pressed || self.desktop.buttons_down() {
            log::debug!("Clicked outside or lost focus while a button is held, deciding after release");
            self.session.automatic = true;
            self.watch_session(cx);
            return;
        }
        self.hide("clicked outside or focus moved to another window", cx);
        self.session.dismissed = Some(cx.background_executor().now());
    }

    pub(super) fn cancel_drag(&mut self, cx: &mut Context<Self>) {
        self.hide("right button pressed during a drag", cx);
        self.session.dismissed = Some(cx.background_executor().now());
    }

    pub(super) fn recently_dismissed(&self, cx: &App) -> bool {
        self.session.just_dismissed(cx.background_executor().now())
    }

    fn should_stay(&self) -> bool {
        self.session.picking || self.desktop.exporting() || self.desktop.buttons_down() || self.desktop.pointer_inside()
    }

    fn settle(&mut self, cx: &mut Context<Self>) {
        if std::mem::take(&mut self.session.lingering) {
            cx.notify();
        }
    }

    fn watch_session(&mut self, cx: &mut Context<Self>) {
        if self.session.task.is_some() {
            return;
        }
        self.session.task = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(POLL).await;
                let done = this
                    .update(cx, |shelf, cx| {
                        if !shelf.session.automatic || shelf.store.settings().pinned {
                            shelf.session.task = None;
                            shelf.settle(cx);
                            return true;
                        }
                        if shelf.should_stay() {
                            shelf.settle(cx);
                            return false;
                        }
                        shelf.session.task = None;
                        shelf.hide("buttons released and pointer outside the window", cx);
                        shelf.session.dismissed = Some(cx.background_executor().now());
                        true
                    })
                    .unwrap_or(true);
                if done {
                    break;
                }
            }
        }));
    }

    pub fn hide(&mut self, reason: &str, cx: &mut Context<Self>) {
        log::debug!("Hiding the window: {reason}");
        self.notices.update(cx, |notices, cx| notices.clear(cx));
        let animated = self.fades();
        if animated {
            self.session.freeze();
        }
        self.session.reset();
        self.stop_recording(cx);
        if animated {
            self.fade.update(cx, |fade, cx| fade.fade_out(cx));
        } else {
            self.hidden(cx);
        }
        cx.notify();
    }

    pub(super) fn hidden(&mut self, cx: &mut Context<Self>) {
        self.session.thaw();
        self.confirming = None;
        self.menu.close();
        self.navigate(Screen::Shelf, cx);
        self.after_update(|desktop| desktop.hide(), cx);
    }
}
