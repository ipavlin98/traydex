use super::{Marked, Tip, badge_icon, hint, icon, measure_weighted};
use crate::design::{glyph, line, opacity, radius, rem, size, space, text};
use crate::i18n::tr;
use crate::theme::Theme;
use gpui::{prelude::*, *};

const BUTTON_PADDING: f32 = space::L;
const BUTTON_TEXT: f32 = text::SMALL;
const SAFE_ZONE: &str = "safe-zone";
const HEADING_WEIGHT: FontWeight = FontWeight::SEMIBOLD;

#[derive(Clone, Copy)]
enum Lit {
    Own,
    Zone,
}

pub(in crate::view) fn icon_button(
    t: Theme,
    id: impl Into<ElementId>,
    name: &'static str,
    tip: impl Into<Tip>,
    active: bool,
) -> Stateful<Div> {
    bare_button(t, id, icon(name, if active { t.text } else { t.muted }, glyph::M), tip, active, t.hover)
}

pub(in crate::view) fn bare_button(
    t: Theme,
    id: impl Into<ElementId>,
    glyph: Svg,
    tip: impl Into<Tip>,
    active: bool,
    hover: Rgba,
) -> Stateful<Div> {
    button_face(t, id, glyph, active, hover, Lit::Own).cursor_pointer().tooltip(hint(t, tip))
}

fn button_face(t: Theme, id: impl Into<ElementId>, glyph: Svg, active: bool, hover: Rgba, lit: Lit) -> Stateful<Div> {
    let face = div()
        .id(id)
        .size(rem(size::BUTTON))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .rounded(rem(radius::SMALL))
        .when(active, |d| d.bg(t.active))
        .child(glyph);
    match lit {
        Lit::Own => face.hover(move |s| s.bg(hover)),
        Lit::Zone => face.group_hover(SAFE_ZONE, move |s| s.bg(hover)),
    }
}

pub(in crate::view) fn safe_zone(
    t: Theme,
    id: impl Into<ElementId>,
    reach: Edges<f32>,
    tip: impl Into<Tip>,
) -> Stateful<Div> {
    div()
        .id(id)
        .group(SAFE_ZONE)
        .flex()
        .flex_shrink_0()
        .mt(rem(-reach.top))
        .pt(rem(reach.top))
        .mr(rem(-reach.right))
        .pr(rem(reach.right))
        .mb(rem(-reach.bottom))
        .pb(rem(reach.bottom))
        .ml(rem(-reach.left))
        .pl(rem(reach.left))
        .cursor_pointer()
        .tooltip(hint(t, tip))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
}

pub(in crate::view) const fn edge_reach(height: f32, start: f32, end: f32) -> Edges<f32> {
    let vertical = (height - size::BUTTON) / 2.;
    Edges { top: vertical, right: end + size::HAIRLINE, bottom: vertical, left: start }
}

pub(in crate::view) fn zoned_row_button(
    t: Theme,
    id: impl Into<ElementId>,
    name: &'static str,
    tip: impl Into<Tip>,
    reach: Edges<f32>,
) -> Stateful<Div> {
    safe_zone(t, id, reach, tip).child(button_face(t, "face", icon(name, t.muted, glyph::M), false, t.bg, Lit::Zone))
}

pub(in crate::view) fn disabled_button(t: Theme, id: impl Into<ElementId>, name: &'static str) -> Stateful<Div> {
    div()
        .id(id)
        .size(rem(size::BUTTON))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .rounded(rem(radius::SMALL))
        .cursor(CursorStyle::OperationNotAllowed)
        .child(icon(name, t.border_strong, glyph::M))
}

pub(in crate::view) fn copy_button(
    t: Theme,
    id: impl Into<ElementId>,
    copied: bool,
    tip: Tip,
    hover: Rgba,
) -> Stateful<Div> {
    let (glyph, tip) = if copied {
        (icon("check", t.success, glyph::M), Tip::from(tr().copied))
    } else {
        (icon("copy", t.muted, glyph::M), tip)
    };
    bare_button(t, id, glyph, tip, false, hover).on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
}

pub(in crate::view) fn row_button(
    t: Theme,
    id: impl Into<ElementId>,
    name: &'static str,
    tip: impl Into<Tip>,
) -> Stateful<Div> {
    bare_button(t, id, icon(name, t.muted, glyph::M), tip, false, t.bg)
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
}

#[derive(Clone, Copy, PartialEq)]
pub(in crate::view) enum Look {
    Plain,
    Primary,
    Danger,
    Disabled,
}

impl Look {
    pub(in crate::view) fn primary_when(ready: bool) -> Self {
        if ready { Look::Primary } else { Look::Disabled }
    }

    pub(in crate::view) fn plain_when(ready: bool) -> Self {
        if ready { Look::Plain } else { Look::Disabled }
    }
}

pub(in crate::view) fn text_button(
    t: Theme,
    id: impl Into<ElementId>,
    label: &'static str,
    look: Look,
) -> Stateful<Div> {
    let (border, color) = match look {
        Look::Plain => (t.border_strong, t.text),
        Look::Primary => (t.accent, t.accent_text),
        Look::Danger => (t.danger, t.danger),
        Look::Disabled => (t.border_strong, t.faint),
    };
    div()
        .id(id)
        .h(rem(size::CONTROL))
        .px(rem(BUTTON_PADDING))
        .flex_shrink_0()
        .flex()
        .items_center()
        .rounded(rem(radius::SMALL))
        .border_1()
        .border_color(border)
        .text_color(color)
        .text_size(rem(BUTTON_TEXT))
        .map(
            |d| {
                if look == Look::Disabled {
                    d.cursor_default()
                } else {
                    d.cursor_pointer().hover(move |s| s.bg(t.hover))
                }
            },
        )
        .child(label)
}

pub(in crate::view) fn picked<E: Styled>(element: E, t: Theme) -> E {
    element.border_color(t.accent).bg(t.accent_tint)
}

pub(in crate::view) fn choice(
    t: Theme,
    id: impl Into<ElementId>,
    label: impl IntoElement,
    active: bool,
    enabled: bool,
) -> Stateful<Div> {
    let cell = div()
        .id(id)
        .flex_1()
        .min_w_0()
        .h(rem(size::CONTROL))
        .px(rem(space::M))
        .flex()
        .items_center()
        .justify_center()
        .rounded(rem(radius::CONTROL))
        .border_1()
        .text_size(rem(text::SMALL))
        .child(div().truncate().child(label));
    if active {
        return picked(cell, t).text_color(t.text);
    }
    cell.border_color(t.border)
        .text_color(if enabled { t.muted } else { t.faint })
        .when(enabled, |d| d.cursor_pointer().hover(move |s| s.bg(t.hover).text_color(t.text)))
}

pub(in crate::view) fn segments(t: Theme) -> Div {
    div()
        .flex()
        .p(rem(space::X2S))
        .gap(rem(space::X2S))
        .rounded(rem(radius::CONTROL))
        .border_1()
        .border_color(t.border_strong)
}

pub(in crate::view) fn segment_base(t: Theme, id: impl Into<ElementId>, active: bool) -> Stateful<Div> {
    div()
        .id(id)
        .h(rem(size::BUTTON))
        .flex()
        .items_center()
        .rounded(rem(radius::CONTROL - space::X2S))
        .cursor_pointer()
        .text_size(rem(text::SMALL))
        .text_color(if active { t.text } else { t.muted })
        .when(active, |d| d.bg(t.active))
        .when(!active, |d| d.hover(move |s| s.bg(t.hover)))
}

pub(in crate::view) fn dimmed<E: Styled>(element: E) -> E {
    element.opacity(opacity::DISABLED).cursor(CursorStyle::OperationNotAllowed)
}

pub(in crate::view) fn unmatched<E: Styled>(element: E) -> E {
    element.opacity(opacity::UNMATCHED)
}

pub(in crate::view) fn heading(
    t: Theme,
    mark: &'static str,
    icon: impl Into<SharedString>,
    title: impl IntoElement,
    subtitle: impl IntoElement,
) -> Div {
    let line = |height: f32| div().h(rem(height)).min_w_0().flex().items_center();
    div()
        .marked(format!("{mark}-header"))
        .flex_shrink_0()
        .flex()
        .items_center()
        .gap(rem(space::XL))
        .child(badge_icon(t, icon, size::PAGE_BADGE, false))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(rem(space::SUBTITLE))
                .child(
                    line(line::TITLE)
                        .marked(format!("{mark}-title"))
                        .text_size(rem(text::TITLE))
                        .font_weight(HEADING_WEIGHT)
                        .child(title),
                )
                .child(
                    line(line::SMALL)
                        .marked(format!("{mark}-info"))
                        .text_size(rem(text::SMALL))
                        .text_color(t.muted)
                        .child(subtitle),
                ),
        )
}

pub(in crate::view) fn page(id: &'static str) -> Stateful<Div> {
    div().id(id).px(rem(space::PAGE)).py(rem(space::XL)).flex().flex_col().gap(rem(space::BLOCK))
}

pub(in crate::view) fn heading_width(window: &Window, title: &str) -> f32 {
    measure_weighted(window, title, text::TITLE, HEADING_WEIGHT)
}
