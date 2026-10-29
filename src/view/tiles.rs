use super::FADE;
use super::widgets::{Marked, Tip, action_glyph, dimmed, grid, hint, measure, spinner, subtitle_lift, subtitled};
use crate::actions::{Action, Tile, TileId};
use crate::design::{Raster, glyph, line, radius, rem, size, snap_up, space, text};
use crate::geometry::{
    COMPACT_HEIGHT, COMPACT_MIN, COMPACT_ZONE_HEIGHT, GUTTER, MAX_COLUMNS, TILE_GAP, block, columns, space,
};
use crate::i18n::tr;
use crate::shelf::{Job, Jobs, Lane, SafeListener, Shelf, Target};
use crate::store::TileInput;
use crate::theme::Theme;
use gpui::{prelude::*, *};

const TILE_MIN: f32 = 104.;
const TILE_HEIGHT: f32 = 88.;
const TILE_TEXT_INSET: f32 = 18.;
const ZONE_HEIGHT: f32 = 76.;
const COMPACT_PADDING: f32 = space::L;
const COMPACT_TEXT_INSET: f32 = 50.;
const PROGRESS_HEIGHT: f32 = size::HAIRLINE * 2.;
const STRIP_GAP: f32 = space::S;
const STRIP_LABEL_GAP: f32 = space::M;
const FILL_ASPECT: f32 = 0.75;
pub(crate) const STRIP_COVER: f32 = FADE + size::BUTTON;

pub(super) fn probe(record: impl Fn(Bounds<Pixels>) + 'static) -> impl IntoElement {
    canvas(move |bounds, window, _| record(bounds.intersect(&window.content_mask().bounds)), |_, _, _, _| {})
        .absolute()
        .top_0()
        .left_0()
        .size_full()
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Fit {
    Even,
    Grow,
    Scroll,
}

#[derive(Clone, Copy)]
struct Strip {
    running: bool,
    fit: Fit,
    title: Option<f32>,
}

fn strip_width(title: Option<f32>, raster: Raster) -> f32 {
    let chrome = raster.rem(space::L) * 2. + raster.rem(glyph::M) + raster.px(size::HAIRLINE) * 2.;
    chrome + title.map_or(0., |title| raster.rem(title) + raster.rem(STRIP_LABEL_GAP))
}

fn strip_gaps(count: usize, raster: Raster) -> f32 {
    raster.rem(STRIP_GAP) * count.saturating_sub(1) as f32
}

fn strip_span(titles: &[Option<f32>], raster: Raster) -> f32 {
    titles.iter().map(|title| strip_width(*title, raster)).sum::<f32>() + strip_gaps(titles.len(), raster)
}

fn strip_fit(titles: &[Option<f32>], room: f32, raster: Raster) -> Fit {
    let widest = titles.iter().map(|title| strip_width(*title, raster)).fold(0., f32::max);
    if widest * titles.len() as f32 + strip_gaps(titles.len(), raster) <= room {
        Fit::Even
    } else if strip_span(titles, raster) <= room {
        Fit::Grow
    } else {
        Fit::Scroll
    }
}

#[derive(Clone, Copy)]
struct Layout {
    columns: usize,
    compact: bool,
    zone: Option<f32>,
    text: f32,
    lift: f32,
}

fn filled_columns(room: f32, space: f32, count: usize) -> Option<usize> {
    let count = count.max(1);
    (1..=MAX_COLUMNS.min(count))
        .filter_map(|columns| {
            let rows = count.div_ceil(columns);
            let width = (room - TILE_GAP * (columns - 1) as f32) / columns as f32;
            let height = (space - TILE_GAP * (rows - 1) as f32) / rows as f32;
            (width >= TILE_MIN && height >= TILE_HEIGHT).then(|| (columns, (height / width / FILL_ASPECT).ln().abs()))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(columns, _)| columns)
}

impl Layout {
    fn glyph(self) -> f32 {
        if self.compact { glyph::L } else { glyph::XL }
    }
}

fn caption(layout: Layout, content: SharedString, size: f32, height: f32, color: Rgba) -> Div {
    div()
        .w(rem(layout.text))
        .truncate()
        .when(!layout.compact, |d| d.text_center())
        .text_size(rem(size))
        .line_height(rem(height))
        .text_color(color)
        .child(content)
}

fn arrange(tile: Div, layout: Layout, glyph: impl IntoElement, labels: Option<(Div, Div)>) -> Div {
    let height = if layout.compact { COMPACT_HEIGHT } else { TILE_HEIGHT };
    let tile = if layout.zone.is_some() { tile.h(rem(height)) } else { tile.min_h(rem(height)) };
    let Some((title, verb)) = labels else {
        return tile.justify_center().child(glyph);
    };
    if layout.compact {
        tile.px(rem(COMPACT_PADDING)).gap(rem(COMPACT_PADDING)).child(glyph).child(subtitled(layout.lift, title, verb))
    } else {
        tile.flex_col()
            .justify_center()
            .gap(rem(space::M))
            .child(glyph)
            .child(div().flex().flex_col().items_center().gap(rem(space::SUBTITLE)).child(title).child(verb))
    }
}

fn sized(room: f32, columns: usize, compact: bool, zone: Option<f32>) -> Layout {
    let tile = (room - TILE_GAP * (columns - 1) as f32) / columns as f32;
    let inset = if compact { COMPACT_TEXT_INSET } else { TILE_TEXT_INSET };
    Layout { columns, compact, zone, text: (tile - inset).floor().max(20.), lift: 0. }
}

fn layout(width: f32, height: f32, count: usize, zone: bool) -> Layout {
    let room = width - GUTTER * 2.;
    if !zone && let Some(columns) = filled_columns(room, space(height) + TILE_GAP, count) {
        return sized(room, columns, false, None);
    }
    let (space, wanted, least) =
        if zone { (space(height), ZONE_HEIGHT, COMPACT_ZONE_HEIGHT) } else { (space(height) + TILE_GAP, 0., 0.) };
    let fewest = count.max(1);
    let regular = columns(room, TILE_MIN, 2).min(fewest);
    let rest = space - block(count, regular, TILE_HEIGHT);
    let (columns, compact, rest) = if rest >= wanted {
        (regular, false, rest)
    } else {
        let compact = columns(room, COMPACT_MIN, 1).min(fewest);
        (compact, true, (space - block(count, compact, COMPACT_HEIGHT)).max(least))
    };
    sized(room, columns, compact, zone.then(|| rest.floor()))
}

impl Shelf {
    fn status(&self, t: Theme, id: TileId, action: &Action, jobs: &Jobs) -> (SharedString, Rgba) {
        if let Some(job) = jobs.for_tile(id) {
            let running = match job.percent() {
                Some(percent) => format!("{}… {percent}", action.activity()),
                None => format!("{}…", action.activity()),
            };
            (running.into(), t.accent_text)
        } else if self.refuses(id) {
            (tr().tile_refused.into(), t.muted)
        } else if !self.ready(action) {
            (tr().needs_setup.into(), t.danger)
        } else {
            (action.description().into(), t.muted)
        }
    }

    fn tile(&self, t: Theme, index: usize, id: TileId, action: &Action, layout: Layout, jobs: &Jobs) -> Div {
        let refused = self.refuses(id);
        let over = !refused && self.session().look().over == Some(Target::Tile(id));
        let job = jobs.for_tile(id);
        let fraction = job.and_then(Job::fraction);
        let regions = self.regions().clone();
        let (status, color) = self.status(t, id, action, jobs);
        let tint = if over {
            t.accent_text
        } else if action.icon_only {
            color
        } else {
            t.muted
        };
        let glyph = if job.is_some() && fraction.is_none() {
            spinner(t.accent, layout.glyph()).into_any_element()
        } else {
            action_glyph(action, tint, layout.glyph()).into_any_element()
        };
        let labels = (!action.icon_only).then(|| {
            let title = if over { t.accent_text } else { t.text };
            (
                caption(layout, action.title().into(), text::BODY, line::BODY, title),
                caption(layout, status, text::CAPTION, line::CAPTION, color),
            )
        });
        let tile = div()
            .marked(format!("drop-tile:{index}"))
            .relative()
            .flex_1()
            .min_w_0()
            .flex()
            .items_center()
            .rounded(rem(radius::CONTROL))
            .border_1()
            .border_color(if over { t.accent } else { t.border_strong })
            .bg(if over { t.accent_tint } else { t.surface })
            .when(refused, dimmed)
            .when_some(fraction, |tile, fraction| {
                tile.overflow_hidden().child(
                    div().absolute().bottom_0().left_0().h(rem(PROGRESS_HEIGHT)).w(relative(fraction)).bg(t.accent),
                )
            });
        arrange(tile, layout, glyph, labels).child(probe(move |bounds| regions.add_tile(bounds, id)))
    }

    fn cancel_tile(&self, t: Theme, action: &Action, layout: Layout) -> Div {
        let over = self.session().look().over == Some(Target::Cancel);
        let regions = self.regions().clone();
        let labels = (!action.icon_only).then(|| {
            (
                caption(layout, action.title().into(), text::BODY, line::BODY, t.danger),
                caption(layout, action.description().into(), text::CAPTION, line::CAPTION, t.muted),
            )
        });
        let tile = div()
            .marked("drop-cancel")
            .relative()
            .flex_1()
            .min_w_0()
            .flex()
            .items_center()
            .rounded(rem(radius::CONTROL))
            .border_1()
            .border_dashed()
            .border_color(t.danger)
            .when(over, |tile| tile.bg(t.danger_tint));
        arrange(tile, layout, action_glyph(action, t.danger, layout.glyph()), labels)
            .child(probe(move |bounds| regions.set_cancel(bounds)))
    }

    fn strip_tile(&self, t: Theme, index: usize, tile: &Tile, look: Strip, cx: &mut Context<Self>) -> Stateful<Div> {
        let (id, action, running) = (tile.id, &tile.action, look.running);
        let ready = self.ready(action);
        let color = if !ready {
            t.danger
        } else if running {
            t.accent_text
        } else {
            t.muted
        };
        let click = match self.store().settings().tile_input {
            TileInput::Clipboard => tr().tile_click_clipboard,
            TileInput::Selection => tr().tile_click_selection,
            TileInput::Picker => tr().tile_click_picker,
        };
        let tip = Tip::plain(action.title()).line(Tip::muted(if ready { click } else { tr().needs_setup }));
        div()
            .id(("strip-tile", index))
            .marked(format!("strip-tile:{index}"))
            .relative()
            .map(|d| match look.fit {
                Fit::Even => d.flex_1().min_w_0().justify_center(),
                Fit::Grow => d.flex_grow_1().flex_shrink_0().justify_center(),
                Fit::Scroll => d.flex_shrink_0(),
            })
            .h_full()
            .px(rem(space::L))
            .flex()
            .items_center()
            .gap(rem(STRIP_LABEL_GAP))
            .rounded(rem(radius::CONTROL))
            .border_1()
            .border_color(t.border)
            .bg(t.surface)
            .text_size(rem(text::SMALL))
            .text_color(t.text)
            .cursor_pointer()
            .hover(move |s| s.bg(t.hover).border_color(t.border_strong))
            .child(action_glyph(action, color, glyph::M))
            .when_some(look.title, |d, title| d.child(div().w(rem(title)).truncate().child(action.title())))
            .tooltip(hint(t, tip))
            .on_click(cx.safe_listener(move |shelf, _, _, cx| shelf.launch(id, cx)))
    }

    pub(super) fn tile_strip(&self, t: Theme, window: &Window, cx: &mut Context<Self>) -> Div {
        let raster = Raster::of(window);
        let room = raster.px(window.viewport_size().width.into()) - raster.rem(GUTTER) * 2.;
        let titles: Vec<Option<f32>> = self
            .tiles()
            .actions()
            .map(|tile| {
                let title = &tile.action.title();
                (!tile.action.icon_only).then(|| snap_up(measure(window, title, text::SMALL), raster.scale))
            })
            .collect();
        let fit = strip_fit(&titles, room, raster);
        self.record_fitting_width((strip_span(&titles, raster) + raster.rem(GUTTER) * 2.) / raster.factor);
        let jobs = self.jobs(cx);
        let looks: Vec<Strip> = self
            .tiles()
            .actions()
            .zip(titles)
            .map(|(tile, title)| Strip { running: jobs.for_tile(tile.id).is_some(), fit, title })
            .collect();
        let tiles: Vec<_> = self
            .tiles()
            .actions()
            .zip(looks)
            .enumerate()
            .map(|(index, (tile, look))| self.strip_tile(t, index, tile, look, cx))
            .collect();
        div()
            .marked("tile-strip")
            .relative()
            .flex_shrink_0()
            .h(rem(size::CHIP))
            .mx(rem(GUTTER))
            .overflow_hidden()
            .child(
                div()
                    .id("tile-strip")
                    .size_full()
                    .flex()
                    .gap(rem(STRIP_GAP))
                    .overflow_x_scroll()
                    .track_scroll(self.lane(Lane::Tiles))
                    .children(tiles),
            )
            .children(self.lane_covers(t, Lane::Tiles, None, cx))
    }

    pub(super) fn drop_targets(&self, t: Theme, viewport: Size<f32>, cx: &mut Context<Self>) -> AnyElement {
        let lending = self.session().lending();
        let shown = || self.tiles().iter().enumerate().filter(|(_, tile)| lending || !tile.action.cancels());
        let layout =
            Layout { lift: subtitle_lift(cx), ..layout(viewport.width, viewport.height, shown().count(), !lending) };
        let jobs = self.jobs(cx);
        let tiles: Vec<_> = shown()
            .map(|(index, tile)| {
                if tile.action.cancels() {
                    self.cancel_tile(t, &tile.action, layout)
                } else {
                    self.tile(t, index, tile.id, &tile.action, layout, jobs)
                }
            })
            .collect();
        let rows = grid(tiles, layout.columns, TILE_GAP).into_iter().map(|row| row.when(lending, |row| row.flex_1()));
        let regions = self.regions().clone();
        div()
            .flex_1()
            .min_h_0()
            .p(rem(GUTTER))
            .flex()
            .flex_col()
            .gap(rem(TILE_GAP))
            .when_some(layout.zone, |d, zone| {
                d.child(
                    self.drop_area(t, true, self.session().look().over == Some(Target::Shelf), cx)
                        .marked("drop-zone")
                        .relative()
                        .flex_shrink_0()
                        .m_0()
                        .h(rem(zone))
                        .child(probe(move |bounds| regions.set_zone(bounds))),
                )
            })
            .child(
                div()
                    .marked("drop-tiles")
                    .flex_shrink_0()
                    .when(lending, |d| d.flex_1())
                    .flex()
                    .flex_col()
                    .gap(rem(TILE_GAP))
                    .children(rows),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests;
