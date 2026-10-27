use super::chips::{self, AreaEvent, Chip, Display, Peek, is_chip};
use super::layout::{Layout, Placed};
use super::{Edit, TextArea};
use crate::design::{glyph, radius, space};
use crate::input::pixel_bounds;
use crate::store::ItemId;
use crate::theme::Theme;
use gpui::*;
use std::collections::HashMap;
use std::ops::Range;

const ICON: f32 = glyph::XS;
const ICON_PAD: f32 = space::XS;
const CLICK_SLOP: f32 = 4.;

pub(super) struct Painted {
    bounds: Bounds<Pixels>,
    icon: Bounds<Pixels>,
    name: &'static str,
    missing: bool,
    hovered: bool,
    radius: Pixels,
}

pub(super) fn runs(
    display: &Display,
    marked: Option<Range<usize>>,
    style: &TextStyle,
    t: Theme,
    looks: &HashMap<ItemId, Chip>,
) -> Vec<TextRun> {
    let len = display.text.len();
    let mut cuts = vec![0, len];
    for span in &display.spans {
        cuts.extend([span.label.start, span.label.end]);
    }
    if let Some(marked) = &marked {
        cuts.extend([marked.start, marked.end]);
    }
    cuts.retain(|cut| *cut <= len);
    cuts.sort_unstable();
    cuts.dedup();
    cuts.windows(2)
        .filter_map(|pair| {
            let (from, to) = (*pair.first()?, *pair.get(1)?);
            let label = display.spans.iter().find(|span| span.label.contains(&from));
            let color: Hsla = match label {
                Some(span) if looks.get(&span.id).is_none_or(|chip| chip.missing) => t.faint.into(),
                _ => style.color,
            };
            let underline = marked.as_ref().is_some_and(|marked| marked.contains(&from));
            let mut run = run(to - from, style, color);
            run.underline = underline.then(|| UnderlineStyle { thickness: px(1.), color: Some(color), wavy: false });
            Some(run)
        })
        .collect()
}

pub(super) fn run(len: usize, style: &TextStyle, color: Hsla) -> TextRun {
    TextRun { len, font: style.font(), color, background_color: None, underline: None, strikethrough: None }
}

pub(super) fn place(
    layout: &mut Layout,
    origin: Point<Pixels>,
    looks: &HashMap<ItemId, Chip>,
    hovered: Option<&ItemId>,
) -> Vec<Painted> {
    let line_height = layout.line_height();
    let icon = layout.scaled(ICON);
    let pad = layout.scaled(ICON_PAD);
    let mut painted = Vec::new();
    let mut placed = Vec::new();
    for span in &layout.display.spans {
        let Some(row) = layout.row(span.display.start) else {
            continue;
        };
        let local = span.display.start - row.start;
        let visual = row.visual(local);
        let end = (span.display.end - row.start).min(row.edge(visual + 1));
        let (left, right) = (row.x_in(visual, local), row.x_in(visual, end));
        let top = origin.y + row.top + line_height * visual as f32;
        let bounds = pixel_bounds(
            Bounds::from_corners(point(origin.x + left, top), point(origin.x + right, top + line_height)),
            layout.scale,
        );
        let look = looks.get(&span.id);
        painted.push(Painted {
            bounds,
            icon: Bounds::new(point(bounds.left() + pad, top + (line_height - icon) / 2.), size(icon, icon)),
            name: look.map_or("file", |chip| chip.icon),
            missing: look.is_none_or(|chip| chip.missing),
            hovered: hovered == Some(&span.id),
            radius: layout.scaled(radius::SMALL),
        });
        placed.push(Placed { id: span.id.clone(), bounds });
    }
    layout.chips = placed;
    painted
}

pub(super) fn paint_backs(chips: &[Painted], t: Theme, window: &mut Window) {
    for chip in chips {
        let back = if chip.missing { t.hover } else { t.accent_tint };
        let border = if chip.hovered && !chip.missing { t.accent } else { back };
        window.paint_quad(quad(chip.bounds, chip.radius, back, px(1.), border, BorderStyle::Solid));
    }
}

pub(super) fn paint_icons(chips: &[Painted], t: Theme, window: &mut Window, cx: &App) {
    for chip in chips {
        let color = if chip.missing { t.faint } else { t.accent_text };
        let painted = window.paint_svg(chip.icon, chip.name.into(), TransformationMatrix::unit(), color.into(), cx);
        if let Err(error) = painted {
            log::warn!("Couldn't paint the {} icon of a mention: {error:#}", chip.name);
        }
    }
}

impl TextArea {
    pub fn set_looks(&mut self, looks: HashMap<ItemId, Chip>, cx: &mut Context<Self>) {
        self.looks = looks;
        self.version += 1;
        cx.notify();
    }

    pub fn referenced(&self) -> Vec<ItemId> {
        self.refs.ids().cloned().collect()
    }

    pub fn anchor(&self) -> Option<Bounds<Pixels>> {
        self.anchor
    }

    pub fn set_suggesting(&mut self, suggesting: bool) {
        self.suggesting = suggesting;
    }

    pub fn place_cursor(&mut self, offset: usize, cx: &mut Context<Self>) {
        let offset = offset.min(self.raw().len());
        if self.raw().is_char_boundary(offset) {
            self.move_to(offset, cx);
        }
    }

    pub fn reveal_mention(&mut self, id: &ItemId, cx: &mut Context<Self>) {
        let found = self.raw().char_indices().find(|(_, c)| self.refs.id(*c) == Some(id));
        if let Some((at, c)) = found {
            self.move_to(at + c.len_utf8(), cx);
        }
    }

    pub fn choose(&mut self, id: &ItemId, name: &str, cx: &mut Context<Self>) {
        let Some((at, _)) = self.asked.clone() else {
            return;
        };
        let Some(chip) = self.refs.add(id, name) else {
            return;
        };
        let range = at..self.cursor().max(at);
        self.write(range, &format!("{chip} "), Edit::Other, cx);
    }

    pub fn insert(&mut self, picks: &[(ItemId, String)], cx: &mut Context<Self>) {
        let chips: Vec<String> =
            picks.iter().filter_map(|(id, name)| self.refs.add(id, name)).map(String::from).collect();
        if chips.is_empty() {
            return;
        }
        let range = self.edit.selected();
        let glued = super::head(self.raw(), range.start).chars().next_back().is_some_and(|c| !c.is_whitespace());
        let text = format!("{}{} ", if glued { " " } else { "" }, chips.join(" "));
        self.write(range, &text, Edit::Other, cx);
    }

    pub fn insert_text(&mut self, text: &str, cx: &mut Context<Self>) {
        self.replace(self.edit.selected(), text, Edit::Other, cx);
    }

    pub fn dismiss(&mut self, cx: &mut Context<Self>) {
        let Some((at, _)) = self.asked.take() else {
            return;
        };
        self.dismissed = Some(at);
        cx.emit(AreaEvent::Suggest(None));
        cx.notify();
    }

    pub(super) fn suggestion_key(&mut self, keys: &Keystroke, cx: &mut Context<Self>) -> bool {
        let alt = keys.modifiers.alt && !keys.modifiers.control && !keys.modifiers.shift;
        match keys.key.as_str() {
            "up" => cx.emit(AreaEvent::Step(-1)),
            "down" => cx.emit(AreaEvent::Step(1)),
            "k" if alt => cx.emit(AreaEvent::Step(-1)),
            "j" if alt => cx.emit(AreaEvent::Step(1)),
            "enter" | "tab" => cx.emit(AreaEvent::Choose),
            "escape" => self.dismiss(cx),
            _ => return false,
        }
        true
    }

    pub(super) fn ask(&mut self, cx: &mut Context<Self>) {
        let found = if self.edit.selected().is_empty() { chips::query(self.raw(), self.cursor()) } else { None };
        let found = found.map(|(at, query)| (at, query.to_owned()));
        if found.as_ref().map(|(at, _)| *at) != self.dismissed {
            self.dismissed = None;
        }
        let asked = found.filter(|(at, _)| self.dismissed != Some(*at));
        if asked != self.asked {
            self.asked = asked;
            cx.emit(AreaEvent::Suggest(self.asked.as_ref().map(|(_, query)| query.clone())));
        }
    }

    pub(super) fn copy(&mut self, selected: Range<usize>, cx: &mut Context<Self>) {
        let Some(raw) = self.raw().get(selected) else {
            return;
        };
        let plain = self.refs.plain(raw);
        let item = if raw.chars().any(is_chip) {
            ClipboardItem::new_string_with_metadata(plain, self.refs.decode(raw))
        } else {
            ClipboardItem::new_string(plain)
        };
        cx.write_to_clipboard(item);
    }

    pub(super) fn paste(&mut self, selected: Range<usize>, cx: &mut Context<Self>) {
        let Some(item) = cx.read_from_clipboard() else {
            return cx.emit(AreaEvent::Paste);
        };
        let Some(text) = item.text() else {
            return cx.emit(AreaEvent::Paste);
        };
        let stored = item.metadata().filter(|stored| crate::store::mention::plain(stored) == text);
        match stored {
            Some(stored) => {
                let encoded = self.refs.encode(&super::clean(stored));
                self.write(selected, &encoded, Edit::Other, cx);
            }
            None => self.replace(selected, &text, Edit::Other, cx),
        }
    }

    pub(super) fn press_chip(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) -> bool {
        let Some(id) = self.current().and_then(|layout| layout.chip_at(position)).map(|chip| chip.id.clone()) else {
            return false;
        };
        self.pressed = Some((id.clone(), position));
        cx.emit(AreaEvent::Press { id, position });
        true
    }

    pub(super) fn release(&mut self, event: &MouseUpEvent, cx: &mut Context<Self>) {
        self.selecting = false;
        let Some((id, from)) = self.pressed.take() else {
            return;
        };
        let slop = self.current().map_or(px(CLICK_SLOP), |layout| layout.scaled(CLICK_SLOP));
        let still = (event.position.x - from.x).abs() <= slop && (event.position.y - from.y).abs() <= slop;
        let same = self.current().and_then(|layout| layout.chip_at(event.position)).is_some_and(|chip| chip.id == id);
        if still && same {
            cx.emit(AreaEvent::Open { id, reveal: event.modifiers.control });
        }
    }

    pub(super) fn hover_at(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let found = self
            .current()
            .and_then(|layout| layout.chip_at(position))
            .map(|chip| Peek { id: chip.id.clone(), bounds: chip.bounds });
        if found.as_ref().map(|peek| &peek.id) == self.hovered.as_ref() {
            return;
        }
        self.hovered = found.as_ref().map(|peek| peek.id.clone());
        cx.emit(AreaEvent::Hover(found));
        cx.notify();
    }

    pub(super) fn unhover(&mut self, cx: &mut Context<Self>) {
        if self.hovered.take().is_some() {
            cx.emit(AreaEvent::Hover(None));
            cx.notify();
        }
    }
}
