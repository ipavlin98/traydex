use crate::design::{self, line, radius, rem, size, space};
use crate::textarea::{EditBuffer, head, step, word_around};
use crate::theme::{Theme, ThemeMode, fade};
use gpui::{prelude::*, *};
use std::ops::Range;

const SECRET: char = '•';
const HEIGHT: f32 = size::CONTROL;
const LINE_HEIGHT: f32 = line::BODY;
const CARET_HEIGHT: f32 = 14.;
const CARET_WIDTH: f32 = 1.5;
const FADE: f32 = 24.;
const OVERDRAW: f32 = 4.;

pub struct Changed;

pub(crate) fn pixel_bounds(bounds: Bounds<Pixels>, scale: f32) -> Bounds<Pixels> {
    let snap = |value: Pixels| px(design::snap(value.into(), scale));
    Bounds::from_corners(
        point(snap(bounds.left()), snap(bounds.top())),
        point(snap(bounds.right()), snap(bounds.bottom())),
    )
}

pub(crate) fn caret_bounds(at: Point<Pixels>, height: Pixels, scale: f32) -> Bounds<Pixels> {
    let width = design::device(CARET_WIDTH, scale).max(1.) / scale;
    let edges = pixel_bounds(Bounds::new(at, size(px(0.), height)), scale);
    Bounds::new(edges.origin, size(px(width), edges.size.height))
}

pub struct Submitted;

pub struct TextInput {
    focus: FocusHandle,
    edit: EditBuffer,
    placeholder: SharedString,
    secret: bool,
    bare: bool,
    layout: Option<ShapedLine>,
    bounds: Option<Bounds<Pixels>>,
    scroll: Pixels,
    selecting: bool,
    backdrop: Option<fn(&Theme) -> Rgba>,
}

impl EventEmitter<Changed> for TextInput {}

impl EventEmitter<Submitted> for TextInput {}

struct Frame {
    line: ShapedLine,
    selection: Option<Bounds<Pixels>>,
    cursor: Option<Bounds<Pixels>>,
    scroll: Pixels,
    lift: Pixels,
    fades: (bool, bool),
}

fn lift(line: &ShapedLine, style: &TextStyle, font_size: Pixels, height: Pixels, window: &Window) -> Pixels {
    let font = window.text_system().resolve_font(&style.font());
    let cap_height = window.text_system().cap_height(font, font_size);
    let baseline = (height - line.ascent - line.descent) / 2. + line.ascent;
    baseline - cap_height / 2. - height / 2.
}

fn overdrawn(bounds: Bounds<Pixels>) -> Bounds<Pixels> {
    Bounds::new(
        point(bounds.left(), bounds.top() - px(OVERDRAW)),
        size(bounds.size.width, bounds.size.height + px(OVERDRAW * 2.)),
    )
}

fn paint_fades(area: Bounds<Pixels>, (left, right): (bool, bool), color: Rgba, window: &mut Window) {
    let width = design::scaled(FADE, window).min(area.size.width / 2.);
    let edge = size(width, area.size.height);
    if left {
        window.paint_quad(fill(Bounds::new(area.origin, edge), fade(color, 270.)));
    }
    if right {
        window.paint_quad(fill(Bounds::new(point(area.right() - width, area.top()), edge), fade(color, 90.)));
    }
}

pub fn label(text: SharedString, backdrop: Rgba) -> impl IntoElement {
    div().w_full().h(rem(HEIGHT)).flex().items_center().child(
        canvas(
            move |bounds, window, _| {
                let style = window.text_style();
                let font_size = style.font_size.to_pixels(window.rem_size());
                let run = TextRun {
                    len: text.len(),
                    font: style.font(),
                    color: style.color,
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                let line = window.text_system().shape_line(text, font_size, &[run], None);
                let lift = lift(&line, &style, font_size, bounds.size.height, window);
                (line, lift)
            },
            move |bounds, (line, lift), window, cx| {
                let area = overdrawn(bounds);
                window.with_content_mask(Some(ContentMask { bounds: area }), |window| {
                    let _ = line.paint(point(bounds.left(), bounds.top() - lift), bounds.size.height, window, cx);
                    paint_fades(area, (false, line.width > bounds.size.width), backdrop, window);
                });
            },
        )
        .w_full()
        .h(rem(LINE_HEIGHT)),
    )
}

impl TextInput {
    pub fn new(text: String, placeholder: impl Into<SharedString>, secret: bool, cx: &mut Context<Self>) -> Self {
        let end = text.len();
        Self {
            focus: cx.focus_handle(),
            edit: EditBuffer::new(text, end),
            placeholder: placeholder.into(),
            secret,
            bare: false,
            layout: None,
            bounds: None,
            scroll: px(0.),
            selecting: false,
            backdrop: None,
        }
    }

    pub fn bare(mut self) -> Self {
        self.bare = true;
        self
    }

    pub fn fading(mut self, backdrop: fn(&Theme) -> Rgba) -> Self {
        self.backdrop = Some(backdrop);
        self
    }

    pub fn set_placeholder(&mut self, placeholder: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.placeholder = placeholder.into();
        cx.notify();
    }

    #[cfg(test)]
    pub fn placeholder(&self) -> &str {
        &self.placeholder
    }

    pub fn focused(&self, window: &Window) -> bool {
        self.focus.is_focused(window)
    }

    pub fn text(&self) -> &str {
        self.edit.text()
    }

    pub fn set_text(&mut self, text: String, cx: &mut Context<Self>) {
        let end = text.len();
        self.edit.reset(text, end..end);
        cx.notify();
    }

    pub fn focus(&self, window: &mut Window) {
        window.focus(&self.focus);
    }

    pub fn select_all(&mut self, cx: &mut Context<Self>) {
        self.select(0..self.text().len(), cx);
    }

    fn select(&mut self, range: Range<usize>, cx: &mut Context<Self>) {
        self.edit.select(range);
        cx.notify();
    }

    fn display(&self, offset: usize) -> usize {
        if self.secret { head(self.text(), offset).chars().count() * SECRET.len_utf8() } else { offset }
    }

    fn text_offset(&self, offset: usize) -> usize {
        if !self.secret {
            return offset;
        }
        let count = offset / SECRET.len_utf8();
        self.text().char_indices().nth(count).map_or(self.text().len(), |(index, _)| index)
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.edit.move_to(offset);
        cx.notify();
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.edit.select_to(offset);
        cx.notify();
    }

    fn replace(&mut self, range: Range<usize>, text: &str, cx: &mut Context<Self>) {
        let text: String = text.chars().filter(|c| !c.is_control()).collect();
        self.edit.replace(range, &text);
        cx.emit(Changed);
        cx.notify();
    }

    fn erase(&mut self, forward: bool, words: bool, cx: &mut Context<Self>) {
        if self.edit.selected().is_empty() {
            let cursor = self.edit.cursor();
            let edge = step(self.text(), cursor, forward, words);
            self.select_to(edge, cx);
        }
        self.replace(self.edit.selected(), "", cx);
    }

    fn index_at(&self, position: Point<Pixels>) -> usize {
        let (Some(layout), Some(bounds)) = (&self.layout, self.bounds) else {
            return 0;
        };
        if self.text().is_empty() {
            return 0;
        }
        let x = position.x - bounds.left() + self.scroll;
        self.text_offset(layout.closest_index_for_x(x))
    }

    fn key(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let keys = &event.keystroke;
        let (control, shift) = (keys.modifiers.control, keys.modifiers.shift);
        let cursor = self.edit.cursor();
        let selected = self.edit.selected();
        let end = self.text().len();
        let words = control && !self.secret;
        match keys.key.as_str() {
            "backspace" => self.erase(false, words, cx),
            "delete" => self.erase(true, words, cx),
            "left" if shift => self.select_to(step(self.text(), cursor, false, words), cx),
            "right" if shift => self.select_to(step(self.text(), cursor, true, words), cx),
            "home" if shift => self.select_to(0, cx),
            "end" if shift => self.select_to(end, cx),
            "left" if selected.is_empty() || words => self.move_to(step(self.text(), cursor, false, words), cx),
            "left" => self.move_to(selected.start, cx),
            "right" if selected.is_empty() || words => self.move_to(step(self.text(), cursor, true, words), cx),
            "right" => self.move_to(selected.end, cx),
            "home" => self.move_to(0, cx),
            "end" => self.move_to(end, cx),
            "a" if control => self.select_all(cx),
            "c" | "x" if control => {
                if !self.secret
                    && let Some(text) = self.text().get(selected.clone()).filter(|text| !text.is_empty())
                {
                    let text = text.to_string();
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                    if keys.key == "x" {
                        self.replace(selected, "", cx);
                    }
                }
            }
            "v" if control => {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    self.replace(selected, text.trim(), cx);
                }
            }
            "enter" if self.edit.marked().is_none() => cx.emit(Submitted),
            "enter" => {}
            "tab" if self.edit.marked().is_some() => {}
            _ => return,
        }
        cx.stop_propagation();
    }

    fn mouse_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus);
        self.selecting = true;
        let index = self.index_at(event.position);
        match event.click_count {
            2 => self.select(word_around(self.text(), index), cx),
            count if count > 2 => self.select_all(cx),
            _ if event.modifiers.shift => self.select_to(index, cx),
            _ => self.move_to(index, cx),
        }
        cx.stop_propagation();
    }

    fn drag_to(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        if self.selecting {
            self.select_to(self.index_at(position), cx);
        }
    }

    fn shown(&self) -> String {
        if self.secret { SECRET.to_string().repeat(self.text().chars().count()) } else { self.text().to_string() }
    }

    fn frame(&self, bounds: Bounds<Pixels>, focused: bool, t: Theme, window: &mut Window) -> Frame {
        let style = window.text_style();
        let font_size = style.font_size.to_pixels(window.rem_size());
        let empty = self.text().is_empty();
        let (text, color): (SharedString, Hsla) =
            if empty { (self.placeholder.clone(), t.faint.into()) } else { (self.shown().into(), style.color) };
        let run = |len: usize, underline: bool| TextRun {
            len,
            font: style.font(),
            color,
            background_color: None,
            underline: underline.then(|| UnderlineStyle { thickness: px(1.), color: Some(color), wavy: false }),
            strikethrough: None,
        };
        let runs = match self.edit.marked().filter(|_| !empty) {
            Some(marked) => {
                let (start, end) = (self.display(marked.start), self.display(marked.end));
                vec![
                    run(start, false),
                    run(end.saturating_sub(start), true),
                    run(text.len().saturating_sub(end), false),
                ]
                .into_iter()
                .filter(|r| r.len > 0)
                .collect()
            }
            None => vec![run(text.len(), false)],
        };
        let line = window.text_system().shape_line(text, font_size, &runs, None);
        let scale = window.scale_factor();
        let lift = px(design::snap(lift(&line, &style, font_size, bounds.size.height, window).into(), scale));
        let x = |offset: usize| {
            if empty { px(0.) } else { line.x_for_index(self.display(offset)) }
        };
        let width = bounds.size.width;
        let caret = x(self.edit.cursor());
        let mut scroll = px(0.);
        if focused {
            scroll = self.scroll;
            if caret - scroll > width - px(2.) {
                scroll = caret - width + px(2.);
            }
            if caret < scroll {
                scroll = caret;
            }
        }
        let content = if empty { px(0.) } else { line.width };
        scroll = px(design::snap(scroll.min((content - width + px(2.)).max(px(0.))).max(px(0.)).into(), scale));
        let fades = (scroll > px(0.5), line.width - scroll > width + px(0.5));
        let left = bounds.left() - scroll;
        let selected = self.edit.selected();
        let selection = (focused && !selected.is_empty()).then(|| {
            let corners = Bounds::from_corners(
                point(left + x(selected.start), bounds.top() - lift),
                point(left + x(selected.end), bounds.bottom() - lift),
            );
            pixel_bounds(corners, scale)
        });
        let cursor = (focused && selected.is_empty()).then(|| {
            let caret_height = design::scaled(CARET_HEIGHT, window);
            let top = bounds.top() + (bounds.size.height - caret_height) / 2.;
            caret_bounds(point(left + caret, top), caret_height, scale)
        });
        Frame { line, selection, cursor, scroll, lift, fades }
    }
}

impl EntityInputHandler for TextInput {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        self.edit.text_for_range(&range, adjusted)
    }

    fn selected_text_range(&mut self, _: bool, _: &mut Window, _: &mut Context<Self>) -> Option<UTF16Selection> {
        Some(self.edit.selection_utf16())
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.edit.marked_utf16()
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.edit.unmark();
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = self.edit.target(range.as_ref());
        self.replace(range, text, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = self.edit.target(range.as_ref());
        self.edit.mark(range, text, selected);
        cx.emit(Changed);
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let layout = self.layout.as_ref()?;
        let range = self.edit.range_from_utf16(&range);
        let left = bounds.left() - self.scroll;
        Some(Bounds::from_corners(
            point(left + layout.x_for_index(self.display(range.start)), bounds.top()),
            point(left + layout.x_for_index(self.display(range.end)), bounds.bottom()),
        ))
    }

    fn character_index_for_point(
        &mut self,
        position: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        Some(self.edit.offset_to_utf16(self.index_at(position)))
    }
}

impl Render for TextInput {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t =
            cx.try_global::<Theme>().copied().unwrap_or_else(|| Theme::resolve(ThemeMode::System, window.appearance()));
        let focused = self.focus.is_focused(window);
        let entity = cx.entity();
        let painter = entity.clone();
        let focus = self.focus.clone();
        let backdrop = self.backdrop;
        div()
            .track_focus(&self.focus)
            .cursor(CursorStyle::IBeam)
            .on_key_down(cx.listener(Self::key))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::mouse_down))
            .w_full()
            .h(rem(HEIGHT))
            .flex()
            .items_center()
            .when(!self.bare, |d| {
                d.px(rem(space::M))
                    .rounded(rem(radius::CONTROL))
                    .border_1()
                    .border_color(if focused { t.accent } else { t.border_strong })
                    .bg(t.bg)
            })
            .child(
                canvas(
                    move |bounds, window, cx| entity.read(cx).frame(bounds, focused, t, window),
                    move |bounds, frame, window, cx| {
                        window.handle_input(&focus, ElementInputHandler::new(bounds, painter.clone()), cx);
                        let area = overdrawn(bounds);
                        window.with_content_mask(Some(ContentMask { bounds: area }), |window| {
                            if let Some(selection) = frame.selection {
                                window.paint_quad(fill(selection, t.selection));
                            }
                            let origin = point(bounds.left() - frame.scroll, bounds.top() - frame.lift);
                            let _ = frame.line.paint(origin, bounds.size.height, window, cx);
                            if let Some(backdrop) = backdrop {
                                paint_fades(area, frame.fades, backdrop(&t), window);
                            }
                            if let Some(cursor) = frame.cursor {
                                window.paint_quad(fill(cursor, t.accent));
                            }
                        });
                        let dragged = painter.clone();
                        window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                            if phase == DispatchPhase::Bubble && event.pressed_button == Some(MouseButton::Left) {
                                dragged.update(cx, |input, cx| input.drag_to(event.position, cx));
                            }
                        });
                        let released = painter.clone();
                        window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                            if phase == DispatchPhase::Bubble && event.button == MouseButton::Left {
                                released.update(cx, |input, _| input.selecting = false);
                            }
                        });
                        painter.update(cx, |input, _| {
                            input.layout = Some(frame.line);
                            input.bounds = Some(bounds);
                            input.scroll = frame.scroll;
                        });
                    },
                )
                .w_full()
                .h(rem(LINE_HEIGHT)),
            )
    }
}

#[cfg(test)]
mod tests;
