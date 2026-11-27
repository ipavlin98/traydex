use super::widgets::{
    Finder, Look, Marked, Tip, action_badge, action_icon, badge_icon, choice, disabled_button, edge_reach, grid,
    heading, highlighted, hint, icon, inset, leading_icon, measure, notice, page, picked, row_button, searched,
    section, segment_base, segments, spinner, subtitle_lift, subtitled, switch, text_button, unmatched,
    zoned_row_button,
};
use super::{FADE, fade_cover};
use crate::actions::{
    Action, Algorithm, Compression, Custom, ENGINES, Fetch, Field, Input, Kind, MAX_TILES, Presence, Setting, Slot,
    Tile, TileId, Tool, Upload, VIRUSTOTAL_JOIN, engine_name,
};
use crate::design::{glyph, line, opacity, radius, rem, size, snap_up, space, text};
use crate::i18n::{self, fill, tr};
use crate::input::TextInput;
use crate::keys::Command;
use crate::shelf::{Hovered, Page, SafeListener, Shelf};
use crate::theme::Theme;
use gpui::{prelude::*, *};
use serde_json::Value;

mod editor;

const PERCENT_TEXT: f32 = text::BODY;
const LIST_CHROME: f32 = 238.;
const ROW_HEIGHT: f32 = size::ROW;
const ROW_RADIUS: f32 = radius::LARGE - size::HAIRLINE;
const ROW_BADGE: f32 = size::ROW_BADGE;
const EDITOR_CHROME: f32 = 150.;
const CHIP_COLUMNS: usize = 2;
const ENGINE_COLUMNS: usize = 3;
const LANGUAGE_COLUMNS: usize = 2;
const WINDOWS_LANGUAGES: &str = "ms-settings:regionlanguage";
const CHIP_GROUP: &str = "chip";
const CHIP_END: f32 = (size::CHIP - size::BUTTON) / 2.;
const CHIP_BUTTON_REACH: Edges<f32> = edge_reach(size::CHIP, space::XS, CHIP_END);
const IMGUR_REGISTER: &str = "https://api.imgur.com/oauth2/addclient";

pub(super) fn group(t: Theme) -> Div {
    div()
        .flex_shrink_0()
        .flex()
        .flex_col()
        .rounded(rem(radius::LARGE))
        .border_1()
        .border_color(t.border)
        .bg(t.surface)
        .overflow_hidden()
}

pub(super) fn label(t: Theme, text: impl Into<SharedString>) -> Div {
    div().text_size(rem(text::SMALL)).text_color(t.muted).child(text.into())
}

pub(super) fn labelled(t: Theme, caption: impl Into<SharedString>) -> Div {
    div().flex().flex_col().gap(rem(space::LABEL)).child(label(t, caption))
}

pub(super) fn options(t: Theme, caption: &'static str, items: Vec<Stateful<Div>>) -> Div {
    labelled(t, caption).child(segments(t).children(items))
}

struct Row<'a> {
    previous: Option<TileId>,
    next: Option<TileId>,
    width: f32,
    text_width: f32,
    lift: f32,
    finder: Finder<'a>,
}

#[derive(Clone, Copy)]
struct Toggle {
    id: &'static str,
    icon: &'static str,
    name: fn() -> &'static str,
    hint: fn(&Shelf) -> &'static str,
    kind: fn() -> Kind,
    focus: Option<Field>,
}

const fn toggle(
    id: &'static str,
    icon: &'static str,
    name: fn() -> &'static str,
    hint: fn(&Shelf) -> &'static str,
    kind: fn() -> Kind,
) -> Toggle {
    Toggle { id, icon, name, hint, kind, focus: None }
}

static TOGGLES: [Toggle; 16] = [
    toggle("add-zip", "file-archive", || tr().add_zip, |_| tr().add_zip_hint, || Kind::Zip),
    toggle("add-unpack", "unpack", || tr().add_unpack, |_| tr().add_unpack_hint, || Kind::Unpack),
    toggle("add-jpeg", "image", || tr().add_jpeg, |_| tr().add_convert_hint, || Kind::Jpeg),
    toggle("add-png", "image", || tr().add_png, |_| tr().add_convert_hint, || Kind::Png),
    Toggle {
        focus: Some(Field::ClientId),
        ..toggle(
            "add-imgur",
            "upload",
            || "Imgur",
            |_| tr().add_imgur_hint,
            || Kind::Imgur { client_id: String::new() },
        )
    },
    toggle("add-gif", "film", || tr().add_gif, gif_hint, || Kind::Gif),
    toggle("add-path", "link", || tr().add_path, |_| tr().add_path_hint, || Kind::CopyPath),
    toggle("add-phone", "smartphone", || tr().add_phone, |_| tr().add_phone_hint, || Kind::Phone),
    toggle("add-base64", "code", || "Base64", |_| tr().add_base64_hint, || Kind::DataUri),
    toggle(
        "add-hash",
        "hash",
        || tr().add_hash,
        |_| tr().add_hash_hint,
        || Kind::Hash { algorithm: Algorithm::default(), virustotal: String::new() },
    ),
    toggle("add-ocr", "scan-text", || tr().add_ocr, |_| tr().add_ocr_hint, || Kind::Ocr { language: String::new() }),
    toggle(
        "add-compress",
        "shrink",
        || tr().add_compress,
        |_| tr().add_compress_hint,
        || Kind::Compress { format: Compression::default() },
    ),
    toggle("add-pdf", "file-text", || tr().add_pdf, |_| tr().add_pdf_hint, || Kind::Pdf),
    toggle("add-rename", "rename", || tr().add_rename, |_| tr().add_rename_hint, || Kind::Rename),
    toggle("add-qr", "qr", || tr().add_qr, |_| tr().add_qr_hint, || Kind::Qr),
    toggle("add-ytdlp", "video", || tr().add_ytdlp, ytdlp_hint, || Kind::Ytdlp(Fetch::default())),
];

fn gif_hint(shelf: &Shelf) -> &'static str {
    if shelf.presence(Tool::Ffmpeg).ready() { tr().verb_gif } else { tr().add_gif_hint }
}

fn ytdlp_hint(shelf: &Shelf) -> &'static str {
    let tools = Action::new(Kind::Ytdlp(Fetch::default())).tools();
    if tools.iter().all(|tool| shelf.presence(*tool).ready()) { tr().verb_ytdlp } else { tr().add_ytdlp_hint }
}

enum Mark {
    Empty,
    Added,
    Count(usize),
}

#[derive(Clone, Copy)]
struct Chips<'a> {
    full: bool,
    finder: Finder<'a>,
}

#[derive(Clone)]
struct DraggedTile {
    tile: TileId,
    icon: SharedString,
    title: SharedString,
    width: f32,
    theme: Theme,
}

impl Render for DraggedTile {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let t = self.theme;
        div()
            .marked("tile-ghost")
            .w(rem(self.width))
            .h(rem(ROW_HEIGHT))
            .px(rem(space::L))
            .flex()
            .items_center()
            .gap(rem(space::L))
            .rounded(rem(ROW_RADIUS))
            .border_1()
            .border_color(t.accent)
            .bg(t.surface)
            .shadow_md()
            .font_family(i18n::current().font())
            .text_size(rem(text::BODY))
            .text_color(t.text)
            .child(badge_icon(t, self.icon.clone(), ROW_BADGE, false))
            .child(div().flex_1().min_w_0().truncate().child(self.title.clone()))
    }
}

fn chip(
    t: Theme,
    id: impl Into<SharedString>,
    name: impl Into<SharedString>,
    text: impl Into<SharedString>,
    tip: impl Into<SharedString>,
    mark: Mark,
    chips: Chips,
) -> Stateful<Div> {
    let id: SharedString = id.into();
    let (text, tip): (SharedString, SharedString) = (text.into(), tip.into());
    let found = chips.finder.locate(&text, &tip);
    let missed = found.is_none();
    let (title, _) = found.unwrap_or_default();
    let blocked = chips.full && !matches!(mark, Mark::Added);
    let added = !blocked && !matches!(mark, Mark::Empty | Mark::Count(0));
    let color = if blocked {
        t.faint
    } else if added {
        t.accent_text
    } else {
        t.muted
    };
    let tip = if blocked { Tip::from(fill(tr().tiles_full, MAX_TILES)) } else { Tip::from(tip) };
    let shown = if missed { opacity::UNMATCHED } else { 1. };
    let seen = |fill: Rgba| t.bg.blend(Rgba { a: fill.a * shown, ..fill });
    let under = seen(if added { t.accent_tint } else { t.surface });
    let hover = (!added && !blocked).then(|| (CHIP_GROUP, seen(t.hover)));
    let label = div()
        .relative()
        .flex_1()
        .min_w_0()
        .overflow_hidden()
        .whitespace_nowrap()
        .child(highlighted(t, text, &title))
        .child(fade_cover(FADE, under, hover));
    div()
        .id(id.clone())
        .group(CHIP_GROUP)
        .marked(format!("chip:{id}"))
        .flex_1()
        .min_w_0()
        .h(rem(size::CHIP))
        .px(rem(space::L))
        .flex()
        .items_center()
        .gap(rem(space::M))
        .rounded(rem(radius::CONTROL))
        .border_1()
        .cursor_pointer()
        .when(added, |d| picked(d, t))
        .when(blocked, |d| {
            d.border_color(t.border).bg(t.surface).text_color(t.faint).cursor(CursorStyle::OperationNotAllowed)
        })
        .when(!added && !blocked, |d| {
            d.border_color(t.border).bg(t.surface).hover(move |s| s.bg(t.hover).border_color(t.border_strong))
        })
        .relative()
        .when(missed, unmatched)
        .children(chips.finder.hit(!missed))
        .child(icon(name, color, glyph::M))
        .child(label)
        .map(|d| match mark {
            Mark::Added => d.child(icon("check", t.accent_text, glyph::S)).tooltip(hint(t, tr().remove_action)),
            Mark::Count(count) if count > 0 => d
                .child(div().flex_shrink_0().text_size(rem(text::SMALL)).text_color(color).child(count.to_string()))
                .tooltip(hint(t, tip)),
            _ => d.tooltip(hint(t, tip)),
        })
}

fn link(t: Theme, id: &'static str, text: &'static str, url: &'static str, cx: &mut Context<Shelf>) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .gap(rem(space::XS))
        .text_size(rem(text::SMALL))
        .text_color(t.accent_text)
        .cursor_pointer()
        .hover(|s| s.underline())
        .child(text)
        .child(icon("open", t.accent_text, glyph::XS))
        .on_click(cx.safe_listener(move |shelf, _, _, _| shelf.open_link(url)))
}

pub(super) fn segment(t: Theme, id: &'static str, text: &'static str, active: bool) -> Stateful<Div> {
    segment_base(t, id, active).flex_1().px(rem(space::L)).justify_center().child(text)
}

impl Shelf {
    fn action_text(&self, t: Theme, index: usize, action: &Action, row: &Row) -> (Div, bool) {
        let ready = self.ready(action);
        let (title, detail) = (action.title(), if ready { action.detail() } else { tr().needs_setup.into() });
        let found = row.finder.locate(&title, &detail);
        let missed = found.is_none();
        let (title_hits, detail_hits) = found.unwrap_or_default();
        let text = subtitled(
            row.lift,
            div().marked(format!("action-title:{index}")).w(rem(row.text_width)).truncate().child(highlighted(
                t,
                title,
                &title_hits,
            )),
            div()
                .w(rem(row.text_width))
                .truncate()
                .text_color(if ready { t.muted } else { t.danger })
                .child(highlighted(t, detail, &detail_hits)),
        )
        .flex_1()
        .min_w_0();
        (text, missed)
    }

    fn action_row(&self, t: Theme, index: usize, tile: &Tile, row: &Row, cx: &mut Context<Self>) -> Stateful<Div> {
        let (action, id) = (&tile.action, tile.id);
        let (text, missed) = self.action_text(t, index, action, row);
        let lifted = cx.active_drag_value::<DraggedTile>().is_some_and(|dragged| dragged.tile == id);
        let dragged = DraggedTile {
            tile: id,
            icon: action_icon(action),
            title: action.title().into(),
            width: row.width,
            theme: t,
        };
        div()
            .id(("action", index))
            .marked(format!("action-row:{index}"))
            .h(rem(ROW_HEIGHT))
            .px(rem(space::L))
            .flex()
            .items_center()
            .gap(rem(space::L))
            .cursor_pointer()
            .when(row.previous.is_none(), |d| d.rounded_t(rem(ROW_RADIUS)))
            .when(row.next.is_none(), |d| d.rounded_b(rem(ROW_RADIUS)))
            .when(row.next.is_some(), |d| d.border_b_1().border_color(t.border))
            .when(lifted, |d| d.opacity(opacity::DISABLED))
            .when(missed && !lifted, unmatched)
            .relative()
            .children(row.finder.hit(!missed))
            .hover(move |s| s.bg(t.hover))
            .on_hover(cx.safe_listener(move |shelf, hovered: &bool, _, _| shelf.hover(Hovered::Tile(id), *hovered)))
            .on_click(cx.safe_listener(move |shelf, _, window, cx| shelf.open_editor(id, None, window, cx)))
            .on_drag(dragged, |dragged, _, _, cx| cx.new(|_| dragged.clone()))
            .drag_over::<DraggedTile>(
                move |style, dragged, _, _| {
                    if dragged.tile == id { style } else { style.bg(t.accent_tint) }
                },
            )
            .on_drop(cx.safe_listener(move |shelf, dragged: &DraggedTile, _, cx| shelf.move_tile(dragged.tile, id, cx)))
            .child(icon("grip", t.faint, glyph::S))
            .child(action_badge(t, action, ROW_BADGE))
            .child(text)
            .child(self.row_buttons(t, index, tile, row, cx))
    }

    fn row_buttons(&self, t: Theme, index: usize, tile: &Tile, row: &Row, cx: &mut Context<Self>) -> Div {
        let (own, fixed) = (tile.id, tile.action.cancels());
        let step = |id: &'static str, name: &'static str, tip: &'static str, target: Option<TileId>| match target {
            Some(target) => row_button(t, (id, index), name, tip).marked(format!("{id}:{index}")).on_click(
                cx.safe_listener(move |shelf, _, _, cx| {
                    cx.stop_propagation();
                    shelf.move_tile(own, target, cx);
                }),
            ),
            None => disabled_button(t, (id, index), name)
                .marked(format!("{id}:{index}"))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation()),
        };
        let remove = Tip::keys(tr().remove_action, &self.keys(Command::Remove));
        div()
            .marked(format!("action-buttons:{index}"))
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(rem(space::XS))
            .child(step("move-up", "arrow-up", tr().move_up, row.previous))
            .child(step("move-down", "arrow-down", tr().move_down, row.next))
            .child(if fixed {
                disabled_button(t, ("remove-action", index), "trash")
                    .tooltip(hint(t, tr().cancel_tile_fixed))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            } else {
                row_button(t, ("remove-action", index), "trash", remove).on_click(cx.safe_listener(
                    move |shelf, _, _, cx| {
                        cx.stop_propagation();
                        shelf.remove_action(own, cx);
                    },
                ))
            })
            .child(icon("chevron-right", t.faint, glyph::S))
    }

    fn toggle_chip(&self, t: Theme, toggle: &'static Toggle, chips: Chips, cx: &mut Context<Self>) -> Stateful<Div> {
        let make = || Action::new((toggle.kind)());
        let mark = if self.has_action(&make().slot()) { Mark::Added } else { Mark::Empty };
        chip(t, toggle.id, toggle.icon, (toggle.name)(), (toggle.hint)(self), mark, chips).on_click(
            cx.safe_listener(move |shelf, _, window, cx| shelf.toggle_action(make(), toggle.focus, window, cx)),
        )
    }

    fn instances(&self, kind: fn(&Kind) -> bool) -> Mark {
        Mark::Count(self.tiles().iter().filter(|tile| kind(&tile.action.kind)).count())
    }

    fn chips(&self, t: Theme, finder: Finder, cx: &mut Context<Self>) -> Vec<Stateful<Div>> {
        let context = Chips { full: self.tiles().full(), finder };
        let mut chips = vec![
            chip(
                t,
                "add-folder",
                "folder",
                tr().add_folder,
                tr().add_folder_hint,
                self.instances(|kind| matches!(kind, Kind::Move { .. } | Kind::Copy { .. })),
                context,
            )
            .on_click(cx.safe_listener(|shelf, _, _, cx| {
                shelf.pick_action(tr().pick_folder, true, |folder| Kind::Move { folder }, cx)
            })),
            chip(
                t,
                "add-run",
                "terminal",
                tr().program_label,
                tr().add_run_hint,
                self.instances(|kind| matches!(kind, Kind::Run { .. })),
                context,
            )
            .on_click(cx.safe_listener(|shelf, _, _, cx| {
                shelf.pick_action(tr().pick_program, false, |program| Kind::Run { program }, cx)
            })),
            chip(
                t,
                "add-search",
                "search",
                tr().add_search,
                tr().add_search_hint,
                self.instances(|kind| matches!(kind, Kind::Search { .. })),
                context,
            )
            .on_click(cx.safe_listener(|shelf, _, window, cx| shelf.add_search(window, cx))),
            chip(
                t,
                "add-upload",
                "server",
                tr().add_upload,
                tr().add_upload_hint,
                self.instances(|kind| matches!(kind, Kind::Upload(_))),
                context,
            )
            .on_click(cx.safe_listener(|shelf, _, window, cx| {
                let upload = Upload { field: "file".into(), ..Upload::default() };
                shelf.add_or_edit(Action::new(Kind::Upload(upload)), Some(Field::Url), window, cx)
            })),
        ];
        for toggle in &TOGGLES {
            chips.push(self.toggle_chip(t, toggle, context, cx));
        }
        chips
    }

    fn plugin_chips(&self, t: Theme, finder: Finder, cx: &mut Context<Self>) -> Vec<Stateful<Div>> {
        let chips = Chips { full: self.tiles().full(), finder };
        self.plugins()
            .iter()
            .map(|plugin| {
                let custom = Custom::new(plugin.clone());
                let mark = if self.has_action(&Slot::Custom(custom.id.clone())) { Mark::Added } else { Mark::Empty };
                let tip = if plugin.description().is_empty() {
                    tr().verb_run.to_string()
                } else {
                    plugin.description().to_string()
                };
                let make = move || Action::new(Kind::Custom(custom.clone()));
                let id = plugin.id().to_string();
                let name = format!("uninstall-plugin:{id}");
                let uninstall = zoned_row_button(
                    t,
                    SharedString::from(name.clone()),
                    "trash",
                    tr().plugin_uninstall,
                    CHIP_BUTTON_REACH,
                )
                .marked(name)
                .on_click(cx.safe_listener(move |shelf, _, _, cx| {
                    cx.stop_propagation();
                    shelf.ask_uninstall(&id, cx);
                }));
                chip(
                    t,
                    format!("plugin:{}", plugin.id()),
                    action_icon(&make()),
                    plugin.name().to_string(),
                    tip,
                    mark,
                    chips,
                )
                .pr(rem(CHIP_END))
                .child(uninstall)
                .on_click(cx.safe_listener(move |shelf, _, window, cx| shelf.toggle_action(make(), None, window, cx)))
            })
            .collect()
    }

    fn plugins_section(&self, t: Theme, finder: Finder, cx: &mut Context<Self>) -> Div {
        let chips = self.plugin_chips(t, finder, cx);
        let hint = div()
            .marked("plugins-hint")
            .text_size(rem(text::SMALL))
            .line_height(rem(line::SMALL))
            .text_color(t.muted)
            .child(tr().plugins_hint);
        let list = (!chips.is_empty())
            .then(|| div().flex().flex_col().gap(rem(space::S)).children(grid(chips, CHIP_COLUMNS, space::S)));
        section(t, tr().section_plugins)
            .marked("plugins")
            .child(div().flex().flex_col().gap(rem(space::M)).children(list).child(hint))
    }

    pub(super) fn actions_page(&self, t: Theme, width: f32, cx: &mut Context<Self>) -> AnyElement {
        let query = self.page_query(cx);
        let content = searched(query.as_ref(), self.regions(), |finder| self.actions_content(t, width, finder, cx));
        self.scroll_area(t, Page::Actions, "actions-page", content)
    }

    fn actions_content(&self, t: Theme, width: f32, finder: Finder, cx: &mut Context<Self>) -> Stateful<Div> {
        let text_width = (width - LIST_CHROME).max(40.);
        let row_width = width - space::PAGE * 2.;
        let lift = subtitle_lift(cx);
        let ids: Vec<TileId> = self.tiles().iter().map(|tile| tile.id).collect();
        let rows: Vec<_> = self
            .tiles()
            .iter()
            .enumerate()
            .map(|(index, tile)| {
                let row = Row {
                    previous: index.checked_sub(1).and_then(|previous| ids.get(previous)).copied(),
                    next: ids.get(index + 1).copied(),
                    width: row_width,
                    text_width,
                    lift,
                    finder,
                };
                self.action_row(t, index, tile, &row, cx)
            })
            .collect();
        let chips = self.chips(t, finder, cx);
        let limit = self.tiles().full().then(|| {
            div()
                .marked("tiles-full")
                .pb(rem(space::M))
                .flex()
                .items_start()
                .gap(rem(space::S))
                .text_size(rem(text::SMALL))
                .line_height(rem(line::SMALL))
                .text_color(t.muted)
                .child(icon("alert", t.muted, glyph::S))
                .child(div().flex_1().min_w_0().child(fill(tr().tiles_full, MAX_TILES)))
        });
        page("actions")
            .gap(rem(space::SECTION))
            .child(
                section(t, tr().section_add)
                    .children(limit)
                    .child(div().flex().flex_col().gap(rem(space::S)).children(grid(chips, CHIP_COLUMNS, space::S))),
            )
            .child(self.plugins_section(t, finder, cx))
            .child(section(t, tr().section_tiles).child(if rows.is_empty() {
                div().py(rem(space::M)).text_color(t.faint).child(tr().no_actions)
            } else {
                group(t).children(rows)
            }))
    }
}
