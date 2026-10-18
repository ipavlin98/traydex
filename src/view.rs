mod actions;
mod hashes;
mod items;
mod list;
mod media;
mod motion;
mod password;
mod preview;
mod rename;
mod settings;
mod share;
mod status;
mod tiles;
mod title;
mod widgets;

use self::motion::{Enter, Motion};
pub(crate) use self::tiles::STRIP_COVER;
use self::widgets::{
    KEYS_SEARCH, Look, Marked, Tip, badge, badge_icon, bare_button, copy_button, highlighted, hint, hint_card, icon,
    icon_button, kbd, leading_icon, measure, row_button, subtitle_lift, subtitled, text_button,
};
use crate::design::{self, glyph, line, radius, rem, scaled, size, space, text, timing, units};
use crate::diagnostics;
use crate::geometry::{self, STRIP_PADDING, TITLE_HEIGHT};
use crate::guard;
use crate::i18n::{self, fill, tr};
use crate::keys::Command;
use crate::shelf::{
    COPY_SELECTED, Confirm, Edge, Filter, Hovered, Jobs, Lane, Notice, Page, Place, SafeListener, Shelf, Target,
    copy_id,
};
use crate::store::{self, Item, Kind, Layout};
use crate::system::Caption;
use crate::theme::{Theme, ThemeMode};
pub(super) use crate::theme::{fade, fade_until};
use gpui::{prelude::*, *};
use std::cell::Cell;
use std::rc::Rc;

const ROW_CHROME: f32 = 210.;
const STATUS_HEIGHT: f32 = 41.;
const BAR_PADDING: f32 = space::PAGE;
const BAR_GAP: f32 = space::M;
const BAR_ICON: f32 = glyph::M;
const BAR_TEXT: f32 = text::SMALL;
const ICON_SIZE: f32 = size::ROW_BADGE;
const ROW_HEIGHT: f32 = size::ROW;
const TILE_SIZE: f32 = geometry::CARD_SIZE;
const TILE_GAP: f32 = geometry::CARD_GAP;
const TILE_FOOTER: f32 = 44.;
const TILE_PADDING: f32 = space::M;
const TILE_RADIUS: f32 = radius::LARGE;
const TILE_BADGE: f32 = 26.;
const TILE_INSET: f32 = space::S;
const TILE_BADGE_GAP: f32 = space::XS;
const TILE_BADGE_MARGIN: f32 = (TILE_FOOTER - TILE_BADGE) / 2.;
const TILE_FALLBACK: f32 = glyph::COVER;
const TILE_THUMB_MARGIN: f32 = space::M;
const TILE_THUMB_SCALE: f32 = 0.8;
const TILE_OVERLAY_RADIUS: f32 = radius::CONTROL;
const TILE_OVERLAY_PADDING: f32 = space::X2S;
const TILE_TEXT: usize = 1200;
const TILE_TEXT_FADE: f32 = 28.;
const TEXT_SIZE: f32 = text::BODY;
const CARD_TEXT_SIZE: f32 = text::SMALL;
const STRIP_HEIGHT: f32 = 34.;
const TAB_GAP: f32 = space::X2L;
const COUNT_GAP: f32 = space::S;
const SELECTION_LABEL_GAP: f32 = space::S;
const SELECTION_MARGIN: f32 = space::X3L;
const BAR_BUTTON: f32 = size::BUTTON;
const BUTTON_GAP: f32 = space::X2S;
pub(super) const FADE: f32 = 32.;

fn strip(t: Theme) -> Div {
    div()
        .relative()
        .h(rem(STRIP_HEIGHT))
        .flex_shrink_0()
        .px(rem(STRIP_PADDING))
        .flex()
        .items_center()
        .child(div().absolute().left_0().right_0().bottom_0().h(px(size::HAIRLINE)).bg(t.border))
}

fn faded(t: Theme, width: f32, content: impl IntoElement, under: Rgba, pointer: bool, covered: f32) -> Div {
    div().relative().w(rem(width)).overflow_hidden().whitespace_nowrap().child(content).child(fade_cover(
        FADE + covered,
        under,
        pointer.then_some(("row", t.hover)),
    ))
}

fn fade_cover(width: f32, under: Rgba, hover: Option<(&'static str, Rgba)>) -> Stateful<Div> {
    let solid = FADE / width;
    div()
        .id("fade")
        .absolute()
        .top_0()
        .bottom_0()
        .right_0()
        .w(rem(width))
        .bg(fade_until(under, 90., solid))
        .when_some(hover, |d, (group, color)| d.group_hover(group, move |s| s.bg(fade_until(color, 90., solid))))
}

pub(super) type Hidden = Rc<dyn Fn(bool) -> Pixels>;

pub(super) fn hidden_by(scroll: ScrollHandle) -> Hidden {
    Rc::new(move |top| {
        let offset = -scroll.offset().y;
        if top { offset } else { scroll.max_offset().height - offset }
    })
}

pub(super) fn edge_fades(t: Theme, hidden: Hidden) -> [impl IntoElement; 2] {
    [edge_fade(t, hidden.clone(), true), edge_fade(t, hidden, false)]
}

fn edge_fade(t: Theme, hidden: Hidden, top: bool) -> impl IntoElement {
    let fade = canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            if hidden(top) > px(1.) {
                window.paint_quad(gpui::fill(bounds, fade(t.bg, if top { 0. } else { 180. })));
            }
        },
    )
    .absolute()
    .left_0()
    .right_0()
    .h(rem(FADE));
    if top { fade.top_0() } else { fade.bottom_0() }
}

fn lane_line(t: Theme, lane: Lane) -> Option<Rgba> {
    match lane {
        Lane::Tiles => None,
        Lane::Tabs => Some(t.border),
    }
}

fn lane_inset(lane: Lane) -> f32 {
    match lane {
        Lane::Tiles => 0.,
        Lane::Tabs => space::XS,
    }
}

fn fade_angle(edge: Edge) -> f32 {
    match edge {
        Edge::Start => 270.,
        Edge::End => 90.,
    }
}

fn cover(
    window: &Window,
    bounds: Bounds<Pixels>,
    edge: Edge,
    button: bool,
    band: (Pixels, Pixels),
) -> [Bounds<Pixels>; 2] {
    let solid = if button { scaled(size::BUTTON, window) } else { px(0.) };
    let shade = scaled(FADE, window).min(bounds.size.width / 2.);
    let (solid_left, shade_left) = match edge {
        Edge::Start => (bounds.left(), bounds.left() + solid),
        Edge::End => (bounds.right() - solid, bounds.right() - solid - shade),
    };
    let (top, height) = band;
    [Bounds::new(point(solid_left, top), size(solid, height)), Bounds::new(point(shade_left, top), size(shade, height))]
}

fn lane_edges(scroll: &ScrollHandle) -> [bool; 2] {
    let (offset, max) = (-scroll.offset().x, scroll.max_offset().width);
    Edge::BOTH.map(|edge| edge.hidden(offset, max))
}

fn side_fades(
    t: Theme,
    lane: Lane,
    scroll: ScrollHandle,
    buttons: [bool; 2],
    active: Option<usize>,
) -> impl IntoElement {
    let line = lane_line(t, lane);
    canvas(
        |_, _, _| {},
        move |bounds, _, window, cx| {
            let inset = scaled(lane_inset(lane), window);
            let hidden = lane_edges(&scroll);
            if hidden != buttons {
                let view = window.current_view();
                cx.defer(move |cx| cx.notify(view));
            }
            let rule = px(size::HAIRLINE);
            let underline = active.and_then(|index| scroll.bounds_for_item(index)).map(|tab| {
                Bounds::new(point(tab.left() + scroll.offset().x, tab.bottom() - rule), size(tab.size.width, rule))
            });
            for ((edge, button), hidden) in Edge::BOTH.into_iter().zip(buttons).zip(hidden) {
                if !button && !hidden {
                    continue;
                }
                let angle = fade_angle(edge);
                if !button {
                    let [_, body] =
                        cover(window, bounds, edge, false, (bounds.top() + inset, bounds.size.height - inset * 2.));
                    window.paint_quad(gpui::fill(body, fade(t.bg, angle)));
                }
                let (Some(line), Some(underline)) = (line, underline) else {
                    continue;
                };
                let [solid, shade] = cover(window, bounds, edge, button, (underline.top(), underline.size.height));
                window.with_content_mask(Some(ContentMask { bounds: underline }), |window| {
                    window.paint_quad(gpui::fill(solid, line));
                    window.paint_quad(gpui::fill(shade, fade(line, angle)));
                });
            }
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

fn text_width(window: &Window, text: &str) -> f32 {
    measure(window, text, TEXT_SIZE)
}

impl Shelf {
    fn lane_covers(&self, t: Theme, lane: Lane, active: Option<usize>, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let scroll = self.lane(lane);
        let buttons = lane_edges(scroll);
        let fades = side_fades(t, lane, scroll.clone(), buttons, active).into_any_element();
        let sides = Edge::BOTH.into_iter().zip(buttons).filter(|(_, shown)| *shown);
        std::iter::once(fades)
            .chain(sides.map(|(edge, _)| self.scroll_button(t, lane, edge, cx).into_any_element()))
            .collect()
    }

    fn scroll_button(&self, t: Theme, lane: Lane, edge: Edge, cx: &mut Context<Self>) -> Div {
        let tip = match lane {
            Lane::Tiles => tr().tile_strip_scroll,
            Lane::Tabs => tr().tab_strip_scroll,
        };
        let mark = match (lane, edge) {
            (Lane::Tiles, Edge::Start) => "strip-start",
            (Lane::Tiles, Edge::End) => "strip-end",
            (Lane::Tabs, Edge::Start) => "tabs-start",
            (Lane::Tabs, Edge::End) => "tabs-end",
        };
        let name = match edge {
            Edge::Start => "chevron-left",
            Edge::End => "chevron-right",
        };
        let button = div().h_full().flex().items_center().bg(t.bg).child(
            icon_button(t, mark, name, Tip::plain(tip), false).marked(mark).on_click(cx.safe_listener(
                move |shelf, _, window, cx| shelf.scroll_lane(lane, edge, scaled(STRIP_COVER, window).into(), cx),
            )),
        );
        let shade = div().w(rem(FADE)).h_full().bg(fade(t.bg, fade_angle(edge)));
        let inset = rem(lane_inset(lane));
        let side = div().absolute().top(inset).bottom(inset).flex().block_mouse_except_scroll();
        match edge {
            Edge::Start => side.left_0().child(button).child(shade),
            Edge::End => side.right_0().child(shade).child(button),
        }
    }

    fn key_button(&self, t: Theme, id: &'static str) -> Stateful<Div> {
        div()
            .id(id)
            .marked(id)
            .flex()
            .flex_shrink_0()
            .items_center()
            .rounded(rem(radius::CONTROL))
            .border_1()
            .border_color(t.border_strong)
            .text_size(rem(text::SMALL))
            .text_color(t.muted)
            .cursor_pointer()
            .hover(move |s| s.bg(t.hover).text_color(t.text))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
    }

    fn shortcut_button(&self, t: Theme, id: &'static str, command: Command, label: &'static str) -> Stateful<Div> {
        self.key_button(t, id)
            .p(rem(space::XS))
            .pr(rem(space::L))
            .gap(rem(space::M))
            .child(kbd(t, &self.keys(command)))
            .child(label)
    }

    pub(super) fn drop_area(&self, t: Theme, compact: bool, hover: bool, cx: &mut Context<Self>) -> Div {
        let accent = if hover { t.accent_text } else { t.muted };
        let base = div()
            .marked("drop-area")
            .m(rem(space::PAGE))
            .flex()
            .items_center()
            .justify_center()
            .rounded(rem(radius::CONTROL))
            .border_1()
            .border_dashed()
            .border_color(if hover { t.accent } else { t.border_strong })
            .when(hover, |d| d.bg(t.accent_tint));
        if compact {
            return base
                .h(rem(geometry::COMPACT_ZONE_HEIGHT))
                .flex_shrink_0()
                .gap(rem(space::M))
                .child(icon("drop", accent, glyph::M))
                .child(div().text_color(accent).child(if hover { tr().release_to_add } else { tr().drop_here }))
                .when(!hover, |d| {
                    d.child(
                        self.key_button(t, "paste-compact")
                            .h(rem(size::BUTTON))
                            .px(rem(space::M))
                            .child(tr().or_paste)
                            .on_click(cx.safe_listener(|shelf, _, _, cx| shelf.paste(cx))),
                    )
                });
        }
        base.flex_1()
            .flex_col()
            .gap(rem(space::S))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(rem(space::M))
                    .child(icon("drop", if hover { t.accent_text } else { t.text }, glyph::L))
                    .child(div().font_weight(FontWeight::MEDIUM).child(if hover {
                        tr().release_to_add
                    } else {
                        tr().drop_anything
                    })),
            )
            .child(div().text_size(rem(text::SMALL)).text_color(t.muted).child(tr().drop_kinds))
            .when(!hover, |d| d.child(self.empty_buttons(t, cx)))
    }

    fn scroll_area(&self, t: Theme, page: Page, mark: &'static str, content: Stateful<Div>) -> AnyElement {
        let scroll = self.scroll(page).clone();
        div()
            .marked(mark)
            .relative()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(content.flex_1().min_h_0().overflow_y_scroll().track_scroll(&scroll))
            .children(edge_fades(t, hidden_by(scroll)))
            .into_any_element()
    }
}

fn bottom_bar(t: Theme) -> Div {
    div()
        .h(rem(STATUS_HEIGHT))
        .flex_shrink_0()
        .px(rem(BAR_PADDING))
        .flex()
        .items_center()
        .gap(rem(BAR_GAP))
        .border_t_1()
        .border_color(t.border)
        .text_size(rem(BAR_TEXT))
}

fn broken(t: Theme) -> AnyElement {
    div()
        .size_full()
        .p(rem(space::X3L))
        .flex()
        .items_center()
        .justify_center()
        .bg(t.bg)
        .text_color(t.danger)
        .font_family(i18n::current().font())
        .text_size(rem(text::BODY))
        .child(tr().internal_error)
        .into_any_element()
}

impl Render for Shelf {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = self.theme();
        guard::or_else(move || broken(t), || self.compose(t, window, cx))
    }
}

impl Shelf {
    pub(crate) fn compose(&mut self, t: Theme, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        diagnostics::begin();
        self.sync_window(window, cx);
        self.regions().clear();
        self.reveal_after_frame(scaled(FADE, window).into(), window, cx);
        let width = units(window.viewport_size().width, window);
        let content = match self.page() {
            Page::Shelf => self.shelf_page(t, window, cx),
            Page::Actions => self.actions_page(t, width, cx),
            Page::Edit => self.editor_page(t, window, width, cx),
            Page::Settings => self.settings_page(t, cx),
            Page::Preview => self.preview_page(t, width, cx),
            Page::Share => self.share_page(t, window, cx),
            Page::Hashes => self.hashes_page(t, window, cx),
            Page::Qr => self.qr_page(t, window, cx),
            Page::Rename => self.rename_page(t, window, cx),
            Page::Media => self.media_page(t, window, cx),
            Page::Password => self.password_page(t, window, cx),
        };
        div()
            .id("root")
            .relative()
            .track_focus(self.focus_handle())
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(t.bg)
            .text_color(t.text)
            .font_family(i18n::current().font())
            .text_size(rem(text::BODY))
            .on_key_down(cx.safe_listener(Shelf::key))
            .capture_any_mouse_down(cx.safe_listener(|shelf, _: &MouseDownEvent, _, cx| shelf.pointer_pressed(cx)))
            .on_mouse_move(cx.safe_listener(|shelf, event: &MouseMoveEvent, _, cx| shelf.track(event, cx)))
            .child(self.title_bar(t, cx))
            .child(content)
            .when(self.status_visible(), |d| d.child(self.status_bar(t, window, cx)))
            .children(self.notice_bar(t, window, cx))
            .children(self.diagnostics(t, window, cx))
            .into_any_element()
    }

    fn animated(&self) -> bool {
        self.store().settings().interface_animations
    }

    fn copied(&self, origin: &str, cx: &App) -> bool {
        self.notices(cx).is_copied(origin)
    }

    fn diagnostics(&self, t: Theme, window: &Window, cx: &App) -> Option<impl IntoElement + use<>> {
        if !diagnostics::enabled() {
            return None;
        }
        let viewport = window.viewport_size();
        let state = diagnostics::State {
            pid: std::process::id(),
            page: self.page().name(),
            layout: match self.store().settings().layout {
                Layout::List => "list",
                Layout::Grid => "grid",
            },
            dark: t.bg == Theme::resolve(ThemeMode::Dark, window.appearance()).bg,
            language: i18n::current(),
            scale: window.scale_factor(),
            size: [viewport.width.into(), viewport.height.into()],
            dropping: self.dropping(),
            over: self.session().look().over.and_then(|over| match over {
                Target::Shelf => Some("zone".to_string()),
                Target::Cancel => Some("cancel".to_string()),
                Target::Tile(id) => Some(format!("tile:{}", self.tiles().position(id)?)),
            }),
            pinned: self.store().settings().pinned,
            alert: self.notices(cx).alert().map(ToString::to_string),
            jobs: self.jobs(cx).iter().map(|job| job.label.clone()).collect(),
            selected: self.selection().len(),
            preview: self.preview().map(|preview| preview.key.to_string()),
            preview_loaded: self.preview().is_some_and(|preview| preview.loaded.is_some()),
            items: self.store().items().to_vec(),
            tiles: self
                .tiles()
                .iter()
                .map(|tile| diagnostics::Tile { title: tile.action.title().to_string(), action: tile.action.clone() })
                .collect(),
            marks: Default::default(),
        };
        let state = Cell::new(Some(state));
        let root = self.store().storage().root().to_path_buf();
        let regions = self.regions().clone();
        let order: Vec<_> = self.tiles().iter().map(|tile| tile.id).collect();
        Some(tiles::probe(move |_| {
            if let Some(state) = state.take() {
                let targets: Vec<_> = regions
                    .tiles()
                    .into_iter()
                    .filter_map(|(bounds, id)| Some((bounds, order.iter().position(|tile| *tile == id)?)))
                    .collect();
                diagnostics::publish(&root, state, &targets, regions.zone());
            }
        }))
    }
}
