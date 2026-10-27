use super::widgets::{
    Look, Marked, Tip, badge_icon, copy_button, disabled_button, grid, icon_button, leading_icon, measure, spinner,
    text_button,
};
use super::{BAR_GAP, BAR_ICON, BUTTON_GAP, FADE, Hidden, bottom_bar, copy_id, edge_fades, fade, hidden_by, strip};
use crate::design::{self, glyph, line, radius, rem, scaled, space, text};
use crate::i18n::tr;
use crate::input;
use crate::keys::Command;
use crate::preview::{Content, Entry, Line, Stage};
use crate::shelf::{Preview, SafeListener, Shelf};
use crate::store::{Item, human_size, is_archive, is_image};
use crate::system::Frame;
use crate::theme::Theme;
use gpui::{prelude::*, *};
use std::cell::Cell;
use std::path::Path;
use std::rc::{Rc, Weak};
use std::sync::Arc;

const MONO: &str = "Consolas";
const LINE_HEIGHT: f32 = line::BODY;
const DIGIT_WIDTH: f32 = 7.;
const GUTTER_GAP: f32 = space::X2L;
const TILE_WIDTH: f32 = 112.;
const TILE_HEIGHT: f32 = 100.;
const TILE_GAP: f32 = space::M;
const GRID_PADDING: f32 = space::PAGE;
const THUMBNAIL_SIZE: f32 = 220.;
const TILE_PADDING: f32 = space::M;
const NAME_SIZE: f32 = text::SMALL;
const ENTRY_BADGE: f32 = 36.;
const IMAGE_PADDING: f32 = space::X3L;
const WHEEL_LINES: f32 = 3.;
const WHEEL_PIXELS: f32 = 120.;
const THUMBNAIL_BADGE: f32 = 64.;

fn centered() -> Div {
    div().size_full().p(rem(IMAGE_PADDING)).flex().flex_col().items_center().justify_center().gap(rem(space::L))
}

fn message(color: Rgba, text: &'static str) -> Div {
    centered().text_color(color).child(text)
}

fn line_row(t: Theme, line: &Line, gutter: f32, mono: bool) -> AnyElement {
    div()
        .w_full()
        .px(rem(space::PAGE))
        .flex()
        .line_height(rem(LINE_HEIGHT))
        .when(mono, |d| d.font_family(MONO).text_size(rem(text::SMALL)))
        .when(gutter > 0., |d| {
            d.child(
                div()
                    .w(rem(gutter))
                    .flex_shrink_0()
                    .pr(rem(GUTTER_GAP))
                    .text_right()
                    .text_color(t.faint)
                    .when_some(line.number, |d, number| d.child(number.to_string())),
            )
        })
        .child(div().flex_1().min_w_0().min_h(rem(LINE_HEIGHT)).child(line.text.clone()))
        .into_any_element()
}

fn entry_icon(entry: &Entry) -> &'static str {
    let path = Path::new(entry.name.as_ref());
    match entry {
        Entry { folder: true, .. } => "folder",
        _ if is_image(path) => "image",
        _ if is_archive(path) => "file-archive",
        _ => "file",
    }
}

fn entry_name(t: Theme, name: String, room: f32, window: &Window) -> Div {
    let label = div().text_size(rem(NAME_SIZE)).line_height(rem(line::SMALL)).whitespace_nowrap();
    if measure(window, &name, NAME_SIZE) <= room {
        return label.child(name);
    }
    label
        .relative()
        .w(rem(room))
        .overflow_hidden()
        .child(name)
        .child(div().absolute().top_0().bottom_0().right_0().w(rem(FADE)).bg(fade(t.surface, 90.)))
}

fn entry_tile(t: Theme, entry: &Entry, room: f32, window: &Window) -> Div {
    let name = entry.name.rsplit('/').next().unwrap_or_default().to_owned();
    let size: SharedString = match entry.size {
        Some(size) => human_size(size).into(),
        None => tr().folder.into(),
    };
    div()
        .w_full()
        .h(rem(TILE_HEIGHT))
        .px(rem(TILE_PADDING))
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(rem(space::S))
        .rounded(rem(radius::CONTROL))
        .border_1()
        .border_color(t.border)
        .bg(t.surface)
        .child(badge_icon(t, entry_icon(entry), ENTRY_BADGE, false))
        .child(
            div()
                .w_full()
                .flex()
                .flex_col()
                .items_center()
                .gap(rem(space::SUBTITLE))
                .child(entry_name(t, name, room, window))
                .child(
                    div().text_size(rem(text::CAPTION)).line_height(rem(line::CAPTION)).text_color(t.faint).child(size),
                ),
        )
}

fn text_list(t: Theme, state: ListState, lines: Arc<[Line]>, mono: bool) -> AnyElement {
    let widest = lines.iter().filter_map(|line| line.number).max().unwrap_or(0);
    let gutter = if mono { widest.to_string().len() as f32 * DIGIT_WIDTH + GUTTER_GAP } else { 0. };
    list(state, move |index, _, _| match lines.get(index) {
        Some(line) => line_row(t, line, gutter, mono),
        None => Empty.into_any_element(),
    })
    .size_full()
    .py(rem(space::M))
    .into_any_element()
}

fn scrolled(preview: &Preview, cx: &App) -> Option<Hidden> {
    if let Some(note) = &preview.note {
        let extent = note.area.read(cx).extent();
        return Some(Rc::new(move |top| {
            let (offset, max) = extent.get();
            if top { offset } else { max - offset }
        }));
    }
    match &preview.loaded.as_ref()?.content {
        Content::Text { lines, .. } if !lines.is_empty() => {
            let list = preview.list.clone();
            Some(Rc::new(move |top| {
                let offset = -list.scroll_px_offset_for_scrollbar().y;
                if top { offset } else { list.max_offset_for_scrollbar().height - offset }
            }))
        }
        Content::Entries(entries) if !entries.is_empty() => {
            Some(hidden_by(preview.grid.0.borrow().base_handle.clone()))
        }
        _ => None,
    }
}

fn entry_grid(t: Theme, preview: &Preview, entries: Arc<[Entry]>, width: f32) -> AnyElement {
    let available = width - GRID_PADDING * 2.;
    let columns = (((available + TILE_GAP) / (TILE_WIDTH + TILE_GAP)).floor() as usize).max(1);
    let rows = entries.len().div_ceil(columns);
    let fills = entries.len() >= columns;
    let width =
        if fills { (available - TILE_GAP * (columns - 1) as f32) / columns as f32 } else { TILE_WIDTH.min(available) };
    let room = width - TILE_PADDING * 2. - 2.;
    uniform_list(SharedString::from(format!("entries-{}", preview.key)), rows, move |range, window, _| {
        range
            .map(|row| {
                let start = row * columns;
                let tiles: Vec<_> =
                    entries.iter().skip(start).take(columns).map(|entry| entry_tile(t, entry, room, window)).collect();
                let line = div().px(rem(GRID_PADDING)).pb(rem(TILE_GAP));
                if fills {
                    line.children(grid(tiles, columns, TILE_GAP))
                } else {
                    line.child(
                        div()
                            .flex()
                            .gap(rem(TILE_GAP))
                            .children(tiles.into_iter().map(|tile| div().w(rem(width)).flex().child(tile))),
                    )
                }
            })
            .collect()
    })
    .track_scroll(preview.grid.clone())
    .size_full()
    .pt(rem(GRID_PADDING))
    .into_any_element()
}

fn notches(delta: ScrollDelta) -> f32 {
    match delta {
        ScrollDelta::Lines(lines) => lines.y / WHEEL_LINES,
        ScrollDelta::Pixels(pixels) => f32::from(pixels.y) / WHEEL_PIXELS,
    }
}

#[derive(Clone)]
struct Viewport {
    frame: Rc<Cell<Option<Frame>>>,
    natural: Size<f32>,
}

impl Viewport {
    fn stage(&self, window: &Window) -> Option<(Stage, Point<f32>)> {
        let frame = self.frame.get()?;
        let scale = window.scale_factor();
        let edge = |value: i32| value as f32 / scale;
        let area = Bounds::from_corners(
            point(edge(frame.left), edge(frame.top)),
            point(edge(frame.right), edge(frame.bottom)),
        );
        Some((Stage::new(self.natural, area.size, f32::from(scaled(IMAGE_PADDING, window))), area.origin))
    }

    fn at(&self, position: Point<Pixels>, window: &Window) -> Option<(Stage, Point<f32>)> {
        let (stage, origin) = self.stage(window)?;
        Some((stage, point(f32::from(position.x) - origin.x, f32::from(position.y) - origin.y)))
    }
}

fn frame_probe(preview: &Preview) -> impl IntoElement {
    let frame = preview.frame.clone();
    let host = preview.host.as_ref().map(Rc::downgrade);
    canvas(
        move |bounds, window, _| {
            let scale = window.scale_factor();
            let edge = |value: Pixels| design::device(value.into(), scale) as i32;
            let rect = Frame {
                left: edge(bounds.left()),
                top: edge(bounds.top()),
                right: edge(bounds.right()),
                bottom: edge(bounds.bottom()),
            };
            frame.set(Some(rect));
            if let Some(host) = host.as_ref().and_then(Weak::upgrade) {
                host.place(rect);
            }
        },
        |_, _, _, _| {},
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

impl Shelf {
    pub(super) fn preview_page(&self, t: Theme, width: f32, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let found = self
            .preview()
            .and_then(|preview| Some((preview, self.store().items().iter().find(|item| item.id == preview.key)?)));
        let Some((preview, item)) = found else {
            return message(t.faint, tr().empty_category).flex_1().into_any_element();
        };
        let content = match &preview.note {
            Some(note) => div()
                .marked("note")
                .size_full()
                .child(note.area.clone())
                .children(self.note_overlays(t, window, cx))
                .into_any_element(),
            None => self.preview_content(t, item, preview, width, window, cx),
        };
        let footer = match &preview.note {
            Some(_) => Some((SharedString::from(item.describe().detail), false)),
            None => preview.loaded.as_ref().map(|loaded| (SharedString::from(loaded.detail.clone()), loaded.partial)),
        };
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(self.preview_header(t, item, preview, cx))
            .child(
                div()
                    .marked("preview-content")
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .child(frame_probe(preview))
                    .child(content)
                    .when_some(scrolled(preview, cx), |d, hidden| d.children(edge_fades(t, hidden))),
            )
            .when_some(footer, |d, (detail, partial)| {
                d.child(preview_footer(t, detail, partial, self.cite_link(t, item, cx)))
            })
            .into_any_element()
    }

    fn preview_title(&self, t: Theme, item: &Item, preview: &Preview) -> AnyElement {
        let title = div().flex_1().min_w_0().font_weight(FontWeight::MEDIUM);
        match &preview.note {
            Some(note) => title.marked("note-title").child(note.title.clone()).into_any_element(),
            None => {
                title.marked("preview-title").child(input::label(item.title.clone().into(), t.bg)).into_any_element()
            }
        }
    }

    fn preview_header(&self, t: Theme, item: &Item, preview: &Preview, cx: &mut Context<Self>) -> Div {
        let order = self.visible_items();
        let index = order.iter().position(|other| other.id == item.id);
        let position = index.map(|index| format!("{} / {}", index + 1, order.len()));
        let has_previous = index.is_some_and(|index| index > 0);
        let has_next = index.is_some_and(|index| index + 1 < order.len());
        let copy = copy_id(&item.id);
        let copied = self.copied(&copy, cx);
        strip(t)
            .marked("preview-header")
            .gap(rem(BAR_GAP))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap(rem(space::LABEL))
                    .when(preview.back.is_some(), |d| {
                        d.child(
                            icon_button(
                                t,
                                "preview-back",
                                "arrow-left",
                                Tip::keys(tr().back_to_note, &[tr().escape_key]),
                                false,
                            )
                            .marked("preview-back")
                            .on_click(cx.safe_listener(|shelf, _, window, cx| {
                                shelf.back_to_note(window, cx);
                            })),
                        )
                    })
                    .child(leading_icon(item.icon(), t.muted, BAR_ICON))
                    .child(self.preview_title(t, item, preview)),
            )
            .child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .gap(rem(BUTTON_GAP))
                    .child(if has_previous {
                        icon_button(t, "preview-previous", "chevron-left", Tip::keys(tr().previous, &["←"]), false)
                            .on_click(cx.safe_listener(|shelf, _, window, cx| shelf.step_preview(-1, window, cx)))
                    } else {
                        disabled_button(t, "preview-previous", "chevron-left")
                    })
                    .child(if has_next {
                        icon_button(t, "preview-next", "chevron-right", Tip::keys(tr().next, &["→"]), false)
                            .on_click(cx.safe_listener(|shelf, _, window, cx| shelf.step_preview(1, window, cx)))
                    } else {
                        disabled_button(t, "preview-next", "chevron-right")
                    }),
            )
            .when_some(position, |d, position| {
                d.child(div().flex_shrink_0().text_size(rem(text::SMALL)).text_color(t.faint).child(position))
            })
            .child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .gap(rem(BUTTON_GAP))
                    .child(
                        copy_button(t, copy, copied, Tip::keys(tr().copy, &self.keys(Command::Copy)), t.hover)
                            .on_click(cx.safe_listener(|shelf, _, _, cx| shelf.copy_previewed(cx))),
                    )
                    .child(icon_button(t, "preview-open", "open", self.open_tip(), false).on_click(cx.safe_listener(
                        |shelf, event: &ClickEvent, _, cx| shelf.open_previewed(event.modifiers().control, cx),
                    )))
                    .child(
                        icon_button(t, "preview-close", "close", Tip::keys(tr().hide_hint, &[tr().escape_key]), false)
                            .marked("preview-close")
                            .on_click(cx.safe_listener(|shelf, _, _, cx| shelf.leave_preview(cx))),
                    ),
            )
    }

    fn preview_content(
        &self,
        t: Theme,
        item: &Item,
        preview: &Preview,
        width: f32,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(loaded) = &preview.loaded else {
            return centered().child(spinner(t.muted, glyph::XL)).into_any_element();
        };
        match &loaded.content {
            Content::Missing => message(t.danger, tr().missing).into_any_element(),
            Content::Failed => message(t.muted, tr().preview_failed).into_any_element(),
            Content::Image(natural) => self.image_view(t, item, preview, *natural, window, cx),
            Content::Text { lines, .. } if lines.is_empty() => message(t.faint, tr().empty_category).into_any_element(),
            Content::Text { lines, mono } => text_list(t, preview.list.clone(), lines.clone(), *mono),
            Content::Entries(entries) if entries.is_empty() => message(t.faint, tr().empty_category).into_any_element(),
            Content::Entries(entries) => entry_grid(t, preview, entries.clone(), width),
            Content::Shell(_) if preview.host.is_none() => {
                centered().child(spinner(t.muted, glyph::XL)).into_any_element()
            }
            Content::Shell(_) => div().into_any_element(),
            Content::Thumbnail(image) => centered()
                .child(match image {
                    Some(image) => img(image.clone())
                        .max_w(rem(THUMBNAIL_SIZE))
                        .max_h(rem(THUMBNAIL_SIZE))
                        .object_fit(ObjectFit::Contain)
                        .into_any_element(),
                    None => badge_icon(t, item.icon(), THUMBNAIL_BADGE, false).into_any_element(),
                })
                .child(div().text_color(t.muted).child(tr().preview_none))
                .child(text_button(t, "preview-open-external", tr().open_button, Look::Plain).on_click(
                    cx.safe_listener(|shelf, event: &ClickEvent, _, cx| {
                        shelf.open_previewed(event.modifiers().control, cx)
                    }),
                ))
                .into_any_element(),
        }
    }
}

impl Shelf {
    fn image_view(
        &self,
        t: Theme,
        item: &Item,
        preview: &Preview,
        natural: Option<Size<f32>>,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let picture = || {
            img(item.path.clone())
                .with_loading(move || spinner(t.muted, glyph::XL).into_any_element())
                .with_fallback(move || message(t.muted, tr().preview_failed).into_any_element())
        };
        let fitted = || centered().child(picture().size_full().object_fit(ObjectFit::ScaleDown));
        let Some(natural) = natural else {
            return fitted().into_any_element();
        };
        let viewport = Viewport { frame: preview.frame.clone(), natural };
        let zoom = &preview.zoom;
        let placed = viewport.stage(window).map(|(stage, _)| zoom.place(stage)).filter(|_| zoom.zoomed());
        let hand = if zoom.grabbed() { CursorStyle::ClosedHand } else { CursorStyle::OpenHand };
        let (wheel, press, drag) = (viewport.clone(), viewport.clone(), viewport);
        div()
            .id("preview-image")
            .marked("preview-image")
            .size_full()
            .relative()
            .overflow_hidden()
            .map(|d| match placed {
                Some(placed) => d.cursor(hand).child(
                    picture()
                        .absolute()
                        .left(px(placed.origin.x))
                        .top(px(placed.origin.y))
                        .w(px(placed.size.width))
                        .h(px(placed.size.height))
                        .object_fit(ObjectFit::Fill),
                ),
                None => d.child(fitted()),
            })
            .on_scroll_wheel(cx.safe_listener(move |shelf, event: &ScrollWheelEvent, window, cx| {
                if let Some((stage, at)) = wheel.at(event.position, window) {
                    shelf.zoom_preview(notches(event.delta), at, stage, cx);
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.safe_listener(move |shelf, event: &MouseDownEvent, window, cx| {
                    let Some((stage, at)) = press.at(event.position, window) else {
                        return;
                    };
                    match event.click_count {
                        2 => shelf.toggle_zoom(at, stage, cx),
                        _ => shelf.grab_preview(at, cx),
                    }
                }),
            )
            .on_mouse_move(cx.safe_listener(move |shelf, event: &MouseMoveEvent, window, cx| {
                match (event.pressed_button, drag.at(event.position, window)) {
                    (Some(MouseButton::Left), Some((stage, at))) => shelf.drag_preview(at, stage, cx),
                    _ => shelf.release_preview(cx),
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.safe_listener(|shelf, _: &MouseUpEvent, _, cx| shelf.release_preview(cx)),
            )
            .into_any_element()
    }
}

fn preview_footer(t: Theme, detail: SharedString, partial: bool, cited: Option<AnyElement>) -> Div {
    bottom_bar(t)
        .marked("preview-footer")
        .justify_between()
        .gap(rem(BAR_GAP))
        .bg(t.surface)
        .text_color(t.muted)
        .child(div().min_w_0().truncate().child(detail))
        .when(partial, |d| d.child(div().flex_shrink_0().text_color(t.faint).child(tr().preview_partial)))
        .children(cited)
}
