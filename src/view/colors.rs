use super::widgets::{Finder, Marked, hint, icon, icon_button, setting};
use crate::design::{glyph, radius, rem, scaled, size, snap, space};
use crate::i18n::tr;
use crate::shelf::{Paint, Picking, SafeListener, Shelf};
use crate::theme::{ACCENTS, BACKGROUNDS, Hex, Hsv, Theme, contrast, offered};
use gpui::{prelude::*, *};
use std::cell::Cell;
use std::rc::Rc;

const SWATCH: f32 = size::BUTTON;
const PICKER_WIDTH: f32 = 224.;
const FIELD_HEIGHT: f32 = 128.;
const HUE_HEIGHT: f32 = 16.;
const KNOB: f32 = 12.;
const KNOB_RING: f32 = 2.;
const HAIRLINE: f32 = 1.;
const SWATCH_GROUP: &str = "swatch";
const SECTORS: usize = 6;
const RIGHT: f32 = 90.;
const DOWN: f32 = 180.;

#[derive(Clone, Copy, PartialEq)]
enum Area {
    Shade,
    Hue,
}

#[derive(Clone)]
struct Scrub(Area);

fn paint_name(paint: Paint) -> &'static str {
    match paint {
        Paint::Accent => "accent",
        Paint::Background => "background",
        Paint::Text => "text",
    }
}

fn device(count: f32, window: &Window) -> Pixels {
    px(count / window.scale_factor())
}

fn snapped(bounds: Bounds<Pixels>, window: &Window) -> Bounds<Pixels> {
    let scale = window.scale_factor();
    let at = |value: Pixels| px(snap(f32::from(value), scale));
    Bounds::from_corners(point(at(bounds.left()), at(bounds.top())), point(at(bounds.right()), at(bounds.bottom())))
}

fn sample(color: Rgba, edge: Rgba) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let bounds = snapped(bounds, window);
            let round = bounds.size.width.min(bounds.size.height) / 2.;
            window.paint_quad(quad(bounds, round, color, device(HAIRLINE, window), edge, BorderStyle::Solid));
        },
    )
    .absolute()
    .inset_0()
}

fn swatch(t: Theme, id: impl Into<ElementId>, fill: Option<Rgba>, chosen: bool, tip: &'static str) -> Stateful<Div> {
    let color = fill.unwrap_or(Rgba { a: 0., ..t.border_strong });
    let layer = || div().absolute().inset_0();
    let face = div()
        .id(id)
        .group(SWATCH_GROUP)
        .relative()
        .size(rem(SWATCH))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .tooltip(hint(t, tip))
        .child(layer().group_hover(SWATCH_GROUP, |s| s.opacity(0.)).child(sample(color, t.border_strong)))
        .child(layer().opacity(0.).group_hover(SWATCH_GROUP, |s| s.opacity(1.)).child(sample(color, t.muted)));
    match fill {
        Some(color) => {
            let mark = if contrast(color, t.knob) >= contrast(color, t.code_ink) { t.knob } else { t.code_ink };
            face.when(chosen, |d| d.child(icon("check", mark, glyph::XS)))
        }
        None => face.child(icon("pipette", t.muted, glyph::XS)),
    }
}

fn cluster() -> Div {
    div().flex_shrink_0().flex().items_center().gap(rem(space::S))
}

impl Shelf {
    pub(super) fn color_rows(&self, t: Theme, query: Finder, cx: &mut Context<Self>) -> [Div; 3] {
        [
            setting(t, query, tr().accent_color, Some(tr().accent_hint)).child(self.accents(t, cx)),
            setting(t, query, tr().background_color, Some(tr().background_hint)).child(self.backgrounds(t, cx)),
            setting(t, query, tr().text_color, Some(tr().text_hint)).child(self.text_color(t, cx)),
        ]
    }

    fn accents(&self, t: Theme, cx: &mut Context<Self>) -> Div {
        let own = self.current_theme().map_or(t.accent, |entry| entry.own().accent);
        let chosen = self.current_colors().accent;
        let presets: Vec<(usize, Hex)> = offered(&ACCENTS, own).collect();
        let custom = chosen.filter(|color| presets.iter().all(|(_, preset)| preset != color));
        cluster()
            .marked("accent-colors")
            .child(
                swatch(t, "accent-theme", Some(own), chosen.is_none(), tr().color_theme)
                    .on_click(cx.safe_listener(|shelf, _, _, cx| shelf.set_accent(None, cx))),
            )
            .children(presets.into_iter().map(|(index, color)| {
                swatch(t, ("accent", index), Some(color.color()), chosen == Some(color), tr().color_preset)
                    .on_click(cx.safe_listener(move |shelf, _, _, cx| shelf.set_accent(Some(color), cx)))
            }))
            .child(self.custom_swatch(t, Paint::Accent, custom, cx))
    }

    fn backgrounds(&self, t: Theme, cx: &mut Context<Self>) -> Div {
        let own = self.current_theme().map_or(t.bg, |entry| entry.own().bg);
        let chosen = self.current_colors().background;
        let presets: Vec<(usize, Hex)> = offered(&BACKGROUNDS, own).collect();
        let custom = chosen.filter(|color| presets.iter().all(|(_, preset)| preset != color));
        cluster()
            .marked("background-colors")
            .child(
                swatch(t, "background-theme", Some(own), chosen.is_none(), tr().color_theme)
                    .on_click(cx.safe_listener(|shelf, _, _, cx| shelf.set_background(None, cx))),
            )
            .children(presets.into_iter().map(|(index, color)| {
                swatch(t, ("background", index), Some(color.color()), chosen == Some(color), tr().color_preset)
                    .on_click(cx.safe_listener(move |shelf, _, _, cx| shelf.set_background(Some(color), cx)))
            }))
            .child(self.custom_swatch(t, Paint::Background, custom, cx))
    }

    fn text_color(&self, t: Theme, cx: &mut Context<Self>) -> Div {
        let custom = self.current_colors().text.is_some();
        cluster()
            .marked("text-colors")
            .when(custom, |d| {
                d.child(
                    icon_button(t, "text-auto", "reset", tr().text_auto, false)
                        .on_click(cx.safe_listener(|shelf, _, _, cx| shelf.reset_text(cx))),
                )
            })
            .child(self.picker_anchor(
                t,
                Paint::Text,
                swatch(t, "text-custom", Some(t.text), false, tr().color_custom),
                cx,
            ))
    }

    fn custom_swatch(&self, t: Theme, paint: Paint, custom: Option<Hex>, cx: &mut Context<Self>) -> Div {
        let id = SharedString::from(format!("{}-custom", paint_name(paint)));
        let face = swatch(t, id, custom.map(Hex::color), custom.is_some(), tr().color_custom);
        self.picker_anchor(t, paint, face, cx)
    }

    fn picker_anchor(&self, t: Theme, paint: Paint, face: Stateful<Div>, cx: &mut Context<Self>) -> Div {
        let picking = self.themes().picking().filter(|picking| picking.paint == paint);
        div()
            .relative()
            .flex_shrink_0()
            .child(face.on_click(cx.safe_listener(move |shelf, _, _, cx| shelf.toggle_picker(paint, cx))))
            .when_some(picking, |d, picking| {
                d.child(
                    div().absolute().top(rem(SWATCH + space::XS)).right_0().child(
                        deferred(
                            anchored()
                                .anchor(gpui::Anchor::TopRight)
                                .snap_to_window_with_margin(px(space::XS))
                                .child(self.color_picker(t, picking, cx)),
                        )
                        .with_priority(1),
                    ),
                )
            })
    }

    fn color_picker(&self, t: Theme, picking: Picking, cx: &mut Context<Self>) -> Stateful<Div> {
        let hsv = picking.hsv;
        div()
            .id("color-picker")
            .marked("color-picker")
            .w(rem(PICKER_WIDTH))
            .p(rem(space::M))
            .flex()
            .flex_col()
            .gap(rem(space::M))
            .rounded(rem(radius::CONTROL))
            .border_1()
            .border_color(t.border_strong)
            .bg(t.surface)
            .occlude()
            .on_mouse_down_out(cx.safe_listener(|shelf, _, _, cx| shelf.dismiss_picker(cx)))
            .child(self.scrubbed(Area::Shade, FIELD_HEIGHT, cx).child(shade_canvas(t, hsv)))
            .child(self.scrubbed(Area::Hue, HUE_HEIGHT, cx).child(hue_canvas(t, hsv)))
            .child(div().marked("picker-hex").child(self.themes().hex().clone()))
    }

    fn scrubbed(&self, area: Area, height: f32, cx: &mut Context<Self>) -> Stateful<Div> {
        let measured: Rc<Cell<Option<Bounds<Pixels>>>> = Rc::default();
        let seen = measured.clone();
        let id = match area {
            Area::Shade => "picker-shade",
            Area::Hue => "picker-hue",
        };
        div()
            .id(id)
            .marked(id)
            .relative()
            .w_full()
            .h(rem(height))
            .cursor_pointer()
            .child(canvas(move |bounds, _, _| seen.set(Some(bounds)), |_, _, _, _| {}).absolute().inset_0())
            .on_mouse_down(
                MouseButton::Left,
                cx.safe_listener(move |shelf, event: &MouseDownEvent, window, cx| {
                    if let Some(bounds) = measured.get() {
                        shelf.scrub(area, event.position, bounds, window, cx);
                    }
                }),
            )
            .on_drag(Scrub(area), |_, _, _, cx| cx.new(|_| EmptyView))
            .on_drag_move(cx.safe_listener(move |shelf, event: &DragMoveEvent<Scrub>, window, cx| {
                if event.drag(cx).0 == area {
                    shelf.scrub(area, event.event.position, event.bounds, window, cx);
                }
            }))
    }

    fn scrub(
        &mut self,
        area: Area,
        at: Point<Pixels>,
        bounds: Bounds<Pixels>,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let Some(picking) = self.themes().picking() else { return };
        let inset = match area {
            Area::Shade => KNOB,
            Area::Hue => HUE_HEIGHT,
        };
        let (x, y) = fraction(at, bounds, scaled(inset / 2., window));
        let hsv = picking.hsv;
        let hsv = match area {
            Area::Shade => Hsv { saturation: x, value: 1. - y, ..hsv },
            Area::Hue => Hsv { hue: x.min(1.), ..hsv },
        };
        self.pick_shade(hsv, cx);
    }
}

fn fraction(at: Point<Pixels>, bounds: Bounds<Pixels>, inset: Pixels) -> (f32, f32) {
    let along = |offset: Pixels, length: Pixels| {
        let travel = length - inset * 2.;
        if travel <= px(0.) { 0. } else { ((offset - inset) / travel).clamp(0., 1.) }
    };
    (along(at.x - bounds.left(), bounds.size.width), along(at.y - bounds.top(), bounds.size.height))
}

fn along(start: Pixels, length: Pixels, inset: Pixels, share: f32) -> Pixels {
    start + inset + (length - inset * 2.) * share.clamp(0., 1.)
}

fn ramp(length: Pixels, inset: Pixels) -> (f32, f32) {
    let share = if length > px(0.) { inset / length } else { 0. };
    (share, 1. - share)
}

fn shade_canvas(t: Theme, hsv: Hsv) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let bounds = snapped(bounds, window);
            let corner = scaled(radius::SMALL, window);
            let inset = scaled(KNOB / 2., window);
            let white = Hsv { saturation: 0., value: 1., ..hsv }.color();
            let black = Hsv { value: 0., ..hsv }.color();
            let (left, right) = ramp(bounds.size.width, inset);
            let (top, bottom) = ramp(bounds.size.height, inset);
            let across =
                linear_gradient(RIGHT, linear_color_stop(white, left), linear_color_stop(Hsv::pure(hsv.hue), right));
            let down = linear_gradient(
                DOWN,
                linear_color_stop(Hsla::from(black).opacity(0.), top),
                linear_color_stop(black, bottom),
            );
            window.paint_quad(fill(bounds, across));
            window.paint_quad(fill(bounds, down));
            let mask = Rgba { a: 0., ..t.surface };
            window.paint_quad(quad(bounds.dilate(corner), corner * 2., mask, corner, t.surface, BorderStyle::Solid));
            let center = point(
                along(bounds.left(), bounds.size.width, inset, hsv.saturation),
                along(bounds.top(), bounds.size.height, inset, 1. - hsv.value),
            );
            knob(t, center, KNOB, hsv.color(), window);
        },
    )
    .absolute()
    .inset_0()
}

fn hue_canvas(t: Theme, hsv: Hsv) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let bounds = snapped(bounds, window);
            let scale = window.scale_factor();
            let inset = scaled(HUE_HEIGHT / 2., window);
            let round = bounds.size.height / 2.;
            let at = |sector: usize| along(bounds.left(), bounds.size.width, inset, sector as f32 / SECTORS as f32);
            let snapped_at = |sector: usize| px(snap(f32::from(at(sector)), scale));
            for sector in 0..SECTORS {
                let (first, last) = (sector == 0, sector + 1 == SECTORS);
                let start = if first { bounds.left() } else { snapped_at(sector) };
                let end = if last { bounds.right() } else { snapped_at(sector + 1) };
                let part = Bounds::from_corners(point(start, bounds.top()), point(end, bounds.bottom()));
                let corners = Corners {
                    top_left: if first { round } else { px(0.) },
                    bottom_left: if first { round } else { px(0.) },
                    top_right: if last { round } else { px(0.) },
                    bottom_right: if last { round } else { px(0.) },
                };
                let span = end - start;
                let gradient = linear_gradient(
                    RIGHT,
                    linear_color_stop(Hsv::pure(sector as f32 / SECTORS as f32), (at(sector) - start) / span),
                    linear_color_stop(Hsv::pure((sector + 1) as f32 / SECTORS as f32), (at(sector + 1) - start) / span),
                );
                window.paint_quad(fill(part, gradient).corner_radii(corners));
            }
            let center = point(along(bounds.left(), bounds.size.width, inset, hsv.hue), bounds.center().y);
            knob(t, center, HUE_HEIGHT, Hsv::pure(hsv.hue), window);
        },
    )
    .absolute()
    .inset_0()
}

fn knob(t: Theme, center: Point<Pixels>, diameter: f32, color: Rgba, window: &mut Window) {
    let scale = window.scale_factor();
    let side = scaled(diameter, window);
    let at = |value: Pixels| px(snap(f32::from(value), scale));
    let inner = Bounds::new(point(at(center.x - side / 2.), at(center.y - side / 2.)), size(side, side));
    let ring = device(KNOB_RING, window);
    let line = device(HAIRLINE, window);
    window.paint_quad(fill(inner.dilate(line), t.text).corner_radii(side / 2. + line));
    window.paint_quad(quad(inner, side / 2., color, ring, t.bg, BorderStyle::Solid));
}
