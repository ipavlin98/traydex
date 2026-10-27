use crate::actions::Action;
use crate::design::{glyph, line, radius, rem, scale, scaled, size, snap, space, text, timing, units};
use crate::diagnostics;
use crate::i18n;
use crate::search::{Grade, Located, Query};
use crate::shelf::Regions;
use crate::theme::Theme;
use gpui::{prelude::*, *};
use std::cell::Cell;
use std::ops::Range;
use std::time::Duration;

const NOTICE_TEXT: f32 = text::BODY;
const NOTICE_LINE: f32 = line::BODY;
const NOTICE_MARK: f32 = glyph::S;
const NOTICE_GAP: f32 = space::S;
const NOTICE_TIER_GAP: f32 = space::M;
const HINT_WIDTH: f32 = 320.;
const HINT_TEXT: f32 = text::SMALL;
const HINT_PADDING: f32 = space::M;
const SKELETON_HEIGHT: f32 = 8.;
const TITLE_TYPE: (f32, f32) = (text::BODY, line::BODY);
const SUBTITLE_TYPE: (f32, f32) = (text::CAPTION, line::CAPTION);
const SWITCH_WIDTH: f32 = 30.;
const SWITCH_HEIGHT: f32 = 18.;
const SETTING_HEIGHT: f32 = 42.;
const ICON_VIEWBOX: f32 = 24.;
const SKELETON_IN: Duration = Duration::from_millis(400);
const SKELETON_HOLD: f32 = 0.5;

fn run(len: usize, weight: FontWeight) -> TextRun {
    TextRun {
        len,
        font: Font { weight, ..font(i18n::current().font()) },
        color: Hsla::default(),
        background_color: None,
        underline: None,
        strikethrough: None,
    }
}

fn reveal(progress: f32, animated: bool) -> f32 {
    if progress < SKELETON_HOLD {
        0.
    } else if animated {
        (progress - SKELETON_HOLD) / (1. - SKELETON_HOLD)
    } else {
        1.
    }
}

pub(super) fn skeleton(t: Theme, id: impl Into<ElementId>, shape: Div, animated: bool) -> AnyElement {
    shape
        .bg(t.active)
        .with_animation(
            id,
            Animation::new(SKELETON_IN).with_easing(move |progress| reveal(progress, animated)),
            |shape, delta| shape.opacity(delta),
        )
        .into_any_element()
}

pub(super) fn skeleton_line(t: Theme, id: impl Into<ElementId>, animated: bool) -> AnyElement {
    div()
        .relative()
        .child(" ")
        .child(div().absolute().inset_0().flex().items_center().child(skeleton(
            t,
            id,
            div().w_full().h(rem(SKELETON_HEIGHT)).rounded_full(),
            animated,
        )))
        .into_any_element()
}

pub(super) fn measure(window: &Window, text: &str, size: f32) -> f32 {
    measure_weighted(window, text, size, FontWeight::NORMAL)
}

pub(super) fn measure_weighted(window: &Window, text: &str, size: f32, weight: FontWeight) -> f32 {
    let shaped = window.text_system().shape_line(
        SharedString::from(text.to_string()),
        scaled(size, window),
        &[run(text.len(), weight)],
        None,
    );
    units(shaped.width, window)
}

fn wrap(window: &Window, text: &str, size: f32, width: f32) -> Vec<Range<usize>> {
    let shaped = window.text_system().shape_text(
        SharedString::from(text.to_string()),
        scaled(size, window),
        &[run(text.len(), FontWeight::NORMAL)],
        Some(scaled(width, window)),
        None,
    );
    let mut rows = Vec::new();
    let mut offset = 0;
    for line in shaped.iter().flatten() {
        let mut start = 0;
        let breaks = line
            .wrap_boundaries()
            .iter()
            .filter_map(|wrap| line.runs().get(wrap.run_ix)?.glyphs.get(wrap.glyph_ix).map(|glyph| glyph.index));
        for end in breaks.chain([line.text.len()]) {
            let row = line.text.get(start..end).unwrap_or_default().trim_end();
            if !row.is_empty() {
                rows.push(offset + start..offset + start + row.len());
            }
            start = end;
        }
        offset += line.text.len() + 1;
    }
    if rows.is_empty() {
        rows.push(0..0);
    }
    rows
}

pub(super) fn spinner(color: Rgba, size: f32) -> impl IntoElement {
    icon("loader", color, size).with_animation("spinner", Animation::new(timing::SPIN).repeat(), |svg, delta| {
        svg.with_transformation(Transformation::rotate(percentage(delta)))
    })
}

pub(super) fn icon(name: impl Into<SharedString>, color: Rgba, size: f32) -> Svg {
    svg().path(name).size(rem(size)).flex_shrink_0().text_color(color)
}

pub(super) fn action_icon(action: &Action) -> SharedString {
    action.picture().map_or_else(|| action.icon().into(), SharedString::from)
}

pub(super) fn action_glyph(action: &Action, color: Rgba, size: f32) -> Svg {
    icon(action_icon(action), color, size)
}

pub(super) fn inset(name: &'static str, size: f32) -> f32 {
    crate::icons::inset(name) * size / ICON_VIEWBOX
}

pub(super) fn leading_icon(name: &'static str, color: Rgba, size: f32) -> Svg {
    icon(name, color, size).relative().left(rem(-inset(name, size)))
}

pub(super) fn badge(t: Theme, size: f32, active: bool) -> Div {
    div()
        .size(rem(size))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .rounded(rem(size * radius::BADGE_SHARE))
        .bg(if active { t.accent } else { t.active })
        .overflow_hidden()
}

pub(super) fn badge_icon(t: Theme, name: impl Into<SharedString>, size: f32, active: bool) -> Div {
    badge(t, size, active).child(icon(name, if active { t.on_accent } else { t.text }, size * glyph::BADGE_SHARE))
}

pub(super) fn subtitle_lift(cx: &App) -> f32 {
    let text = cx.text_system();
    let font = text.resolve_font(&font(i18n::current().font()));
    let baseline = |(size, line): (f32, f32)| f32::from(text.baseline_offset(font, px(size), px(line)));
    let cap_top = baseline(TITLE_TYPE) - f32::from(text.cap_height(font, px(TITLE_TYPE.0)));
    let last_baseline = TITLE_TYPE.1 + space::SUBTITLE + baseline(SUBTITLE_TYPE);
    optical_lift(cap_top, last_baseline, TITLE_TYPE.1 + space::SUBTITLE + SUBTITLE_TYPE.1)
}

fn optical_lift(cap_top: f32, last_baseline: f32, height: f32) -> f32 {
    (cap_top + last_baseline) / 2. - height / 2.
}

pub(super) fn subtitled(lift: f32, title: impl IntoElement, subtitle: impl IntoElement) -> Div {
    let (title_size, title_line) = TITLE_TYPE;
    let (subtitle_size, subtitle_line) = SUBTITLE_TYPE;
    div()
        .relative()
        .top(rem(-lift))
        .flex()
        .flex_col()
        .gap(rem(space::SUBTITLE))
        .child(div().text_size(rem(title_size)).line_height(rem(title_line)).child(title))
        .child(div().text_size(rem(subtitle_size)).line_height(rem(subtitle_line)).child(subtitle))
}

pub(super) fn action_badge(t: Theme, action: &Action, size: f32) -> Div {
    badge_icon(t, action_icon(action), size, false)
}

pub(super) const KEYS_SEARCH: &[&str] = &["/"];

#[derive(Clone, Copy, PartialEq)]
enum Tone {
    Normal,
    Muted,
}

#[derive(Clone)]
pub(super) struct Tip {
    text: SharedString,
    tone: Tone,
    shortcuts: Vec<String>,
    more: Vec<Tip>,
}

impl Tip {
    pub(super) fn plain(text: impl Into<SharedString>) -> Self {
        Self { text: text.into(), tone: Tone::Normal, shortcuts: Vec::new(), more: Vec::new() }
    }

    pub(super) fn muted(text: impl Into<SharedString>) -> Self {
        Self { tone: Tone::Muted, ..Self::plain(text) }
    }

    pub(super) fn keys(text: impl Into<SharedString>, keys: &[impl AsRef<str>]) -> Self {
        Self::plain(text).or(keys)
    }

    pub(super) fn or(mut self, keys: &[impl AsRef<str>]) -> Self {
        if !keys.is_empty() {
            self.shortcuts.push(keys.iter().map(AsRef::as_ref).collect::<Vec<_>>().join("+"));
        }
        self
    }

    pub(super) fn line(mut self, tip: Tip) -> Self {
        self.more.push(tip);
        self
    }

    fn lines(&self) -> impl Iterator<Item = &Tip> {
        std::iter::once(self).chain(&self.more)
    }

    fn shortcuts(&self) -> Option<String> {
        if self.shortcuts.is_empty() {
            return None;
        }
        Some(format!("· {}", self.shortcuts.join(" / ")))
    }

    fn full_text(&self) -> (String, Option<Range<usize>>) {
        let mut text = self.text.to_string();
        if self.tone == Tone::Muted {
            let all = 0..text.len();
            return (text, Some(all));
        }
        let keys = self.shortcuts().map(|keys| {
            text.push(' ');
            let start = text.len();
            text.push_str(&keys);
            start..text.len()
        });
        (text, keys)
    }
}

impl From<&'static str> for Tip {
    fn from(text: &'static str) -> Self {
        Self::plain(text)
    }
}

impl From<String> for Tip {
    fn from(text: String) -> Self {
        Self::plain(text)
    }
}

impl From<SharedString> for Tip {
    fn from(text: SharedString) -> Self {
        Self::plain(text)
    }
}

struct Hint {
    tip: Tip,
    theme: Theme,
}

impl Render for Hint {
    fn render(&mut self, window: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        hint_card(self.theme, &self.tip, window)
    }
}

pub(super) fn hint_card(t: Theme, tip: &Tip, window: &Window) -> Div {
    let inner = HINT_WIDTH - 2. * HINT_PADDING - 2.;
    let lines: Vec<_> = tip
        .lines()
        .map(|line| {
            let (text, keys) = line.full_text();
            let rows = wrap(window, &text, HINT_TEXT, inner);
            (text, keys, rows)
        })
        .collect();
    let widest = lines
        .iter()
        .flat_map(|(text, _, rows)| {
            rows.iter().map(|row| measure(window, text.get(row.clone()).unwrap_or_default(), HINT_TEXT))
        })
        .fold(0., f32::max);
    let muted = HighlightStyle { color: Some(t.muted.into()), ..Default::default() };
    div()
        .marked("hint")
        .font_family(i18n::current().font())
        .text_size(rem(HINT_TEXT))
        .w(rem(snap(widest + 2. * HINT_PADDING + 2., scale(window)).min(HINT_WIDTH)))
        .px(rem(HINT_PADDING))
        .py(rem(space::XS))
        .rounded(rem(radius::SMALL))
        .border_1()
        .border_color(t.border_strong)
        .bg(t.surface)
        .text_color(t.text)
        .flex()
        .flex_col()
        .gap(rem(space::X2S))
        .children(lines.into_iter().enumerate().map(|(index, (text, keys, rows))| {
            div().marked(format!("hint:{index}")).flex().flex_col().items_start().children(
                rows.into_iter().enumerate().map(|(n, row)| {
                    let keys = keys.as_ref().and_then(|keys| {
                        let (start, end) = (keys.start.max(row.start), keys.end.min(row.end));
                        (start < end).then(|| (start - row.start..end - row.start, muted))
                    });
                    div()
                        .marked(format!("hint:{index}:{n}"))
                        .whitespace_nowrap()
                        .child(StyledText::new(text.get(row).unwrap_or_default().to_string()).with_highlights(keys))
                }),
            )
        }))
}

pub(super) fn hint(t: Theme, tip: impl Into<Tip>) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    let tip = tip.into();
    move |_, cx| cx.new(|_| Hint { tip: tip.clone(), theme: t }).into()
}

pub(super) fn notice(
    name: &str,
    mark: impl IntoElement,
    text: impl Into<SharedString>,
    trailing: impl IntoElement,
) -> Div {
    let indent = NOTICE_MARK + NOTICE_GAP;
    let mark = div()
        .h(rem(NOTICE_LINE))
        .w(rem(NOTICE_MARK))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .child(mark);
    let text = div()
        .marked(format!("{name}:text"))
        .flex_grow()
        .min_w_0()
        .text_size(rem(NOTICE_TEXT))
        .line_height(rem(NOTICE_LINE))
        .child(text.into());
    let lead =
        div().ml(rem(-indent)).flex_grow().min_w_0().flex().items_start().gap(rem(NOTICE_GAP)).child(mark).child(text);
    let trailing = div()
        .marked(format!("{name}:action"))
        .flex_shrink_0()
        .flex()
        .items_center()
        .gap(rem(NOTICE_GAP))
        .child(trailing);
    div()
        .w_full()
        .pl(rem(indent))
        .flex()
        .flex_wrap()
        .items_center()
        .gap_x(rem(NOTICE_GAP))
        .gap_y(rem(NOTICE_TIER_GAP))
        .child(lead)
        .child(trailing)
}

pub(super) fn kbd(t: Theme, keys: &[impl AsRef<str>]) -> Div {
    div().flex().flex_shrink_0().gap(rem(space::XS)).children(keys.iter().map(|key| {
        div()
            .h(rem(size::KEY))
            .min_w(rem(size::KEY))
            .px(rem(space::S))
            .flex()
            .items_center()
            .justify_center()
            .rounded(rem(radius::SMALL))
            .border_1()
            .border_color(t.border_strong)
            .bg(t.surface)
            .text_size(rem(text::CAPTION))
            .text_color(t.muted)
            .child(SharedString::from(key.as_ref().to_owned()))
    }))
}

pub(super) fn switch(t: Theme, id: impl Into<SharedString>, on: bool) -> Stateful<Div> {
    let (track, knob) = if on { (t.accent, t.on_accent) } else { (t.border_strong, t.knob) };
    let id: SharedString = id.into();
    div()
        .id(id.clone())
        .marked(format!("switch:{id}"))
        .w(rem(SWITCH_WIDTH))
        .h(rem(SWITCH_HEIGHT))
        .flex_shrink_0()
        .p(rem(space::X2S))
        .flex()
        .when(on, |d| d.justify_end())
        .rounded_full()
        .cursor_pointer()
        .bg(track)
        .child(div().h_full().flex_shrink_0().rounded_full().bg(knob).map(|mut knob| {
            knob.style().aspect_ratio = Some(1.);
            knob
        }))
}

pub(super) fn section(t: Theme, title: &'static str) -> Div {
    div().flex().flex_col().child(
        div()
            .pb(rem(space::XS))
            .text_size(rem(text::CAPTION))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(t.faint)
            .child(title),
    )
}

#[derive(Clone, Copy)]
enum Pass<'a> {
    Survey(&'a Cell<Option<Grade>>),
    Show(Grade, &'a Regions),
}

#[derive(Clone, Copy)]
pub(super) struct Finder<'a> {
    query: Option<&'a Query>,
    pass: Pass<'a>,
}

pub(super) fn searched<R>(query: Option<&Query>, regions: &Regions, mut build: impl FnMut(Finder) -> R) -> R {
    let grade = query.map_or(Grade::Word, |query| {
        let best = Cell::new(None);
        drop(build(Finder { query: Some(query), pass: Pass::Survey(&best) }));
        best.get().unwrap_or(Grade::Word)
    });
    build(Finder { query, pass: Pass::Show(grade, regions) })
}

impl Finder<'_> {
    pub(super) fn locate(self, title: &str, detail: &str) -> Option<Located> {
        let Some(query) = self.query else {
            return Some(Located::default());
        };
        match self.pass {
            Pass::Survey(best) => {
                if let Some(grade) = query.grade(title, detail, Grade::Fragment) {
                    best.set(Some(best.get().map_or(grade, |known| known.min(grade))));
                }
                None
            }
            Pass::Show(grade, _) => query.locate(title, detail, grade),
        }
    }

    pub(super) fn hit(self, found: bool) -> Option<impl IntoElement> {
        let Pass::Show(_, regions) = self.pass else {
            return None;
        };
        let regions = regions.clone();
        (found && self.query.is_some()).then(|| {
            canvas(move |bounds, _, _| regions.add_hit(bounds), |_, _, _, _| {}).absolute().top_0().left_0().size_full()
        })
    }
}

pub(super) fn highlighted(t: Theme, text: impl Into<SharedString>, ranges: &[Range<usize>]) -> StyledText {
    let style = HighlightStyle {
        color: Some(t.accent_text.into()),
        background_color: Some(t.selection.into()),
        ..Default::default()
    };
    StyledText::new(text).with_highlights(ranges.iter().map(|range| (range.clone(), style)))
}

pub(super) fn setting(t: Theme, finder: Finder, label: &'static str, description: Option<&'static str>) -> Div {
    let found = finder.locate(label, description.unwrap_or_default());
    let missed = found.is_none();
    let (title, detail) = found.unwrap_or_default();
    div()
        .relative()
        .when(missed, unmatched)
        .children(finder.hit(!missed))
        .min_h(rem(SETTING_HEIGHT))
        .py(rem(space::M))
        .flex()
        .items_center()
        .justify_between()
        .gap(rem(space::APART))
        .border_b_1()
        .border_color(t.border)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(rem(space::SUBTITLE))
                .child(highlighted(t, label, &title))
                .when_some(description, |d, text| {
                    d.child(div().text_size(rem(text::SMALL)).text_color(t.muted).child(highlighted(t, text, &detail)))
                }),
        )
}

pub(super) fn grid<E: IntoElement>(items: Vec<E>, columns: usize, gap: f32) -> Vec<Div> {
    let mut items = items.into_iter().peekable();
    let mut rows = Vec::new();
    while items.peek().is_some() {
        let row: Vec<E> = items.by_ref().take(columns).collect();
        let padding = columns - row.len();
        let cell = || div().flex_1().min_w_0().flex();
        rows.push(
            div()
                .flex()
                .gap(rem(gap))
                .children(row.into_iter().map(|item| cell().child(item)))
                .children((0..padding).map(|_| cell())),
        );
    }
    rows
}

pub(super) fn mark(name: SharedString) -> Option<impl IntoElement> {
    diagnostics::enabled().then(|| super::tiles::probe(move |bounds| diagnostics::record(name.to_string(), bounds)))
}

pub(super) trait Marked: InteractiveElement + ParentElement + Sized {
    fn marked(self, name: impl Into<SharedString>) -> Self {
        let name: SharedString = name.into();
        let selector = name.clone();
        self.debug_selector(move || selector.to_string()).children(mark(name))
    }
}

impl<E: InteractiveElement + ParentElement> Marked for E {}

mod controls;

pub(super) use controls::{
    Look, bare_button, choice, copy_button, dimmed, disabled_button, edge_reach, heading, heading_width, icon_button,
    page, picked, row_button, segment_base, segments, text_button, unmatched, zoned_row_button,
};

#[cfg(test)]
mod tests;
