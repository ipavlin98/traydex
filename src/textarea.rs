use crate::design::{self, line, rem, space};
use crate::input::{Changed, caret_bounds, pixel_bounds};
use crate::theme::{Theme, ThemeMode};
use gpui::{prelude::*, *};
use std::cell::Cell;
use std::ops::Range;
use std::rc::Rc;

const LINE_HEIGHT: f32 = line::BODY;
const PADDING_X: f32 = space::PAGE;
const PADDING_Y: f32 = space::M;
const CARET_INSET: f32 = 2.;
const NEWLINE_WIDTH: f32 = 4.;
const HISTORY: usize = 200;

mod buffer;
mod handler;
mod layout;
mod words;

pub(crate) use buffer::EditBuffer;
use layout::{Layout, Row, breaks};
use words::{clean, previous};
pub(crate) use words::{head, step, tail, word_around};

#[derive(Clone, Copy, PartialEq)]
enum Edit {
    Typing,
    Erasing,
    Other,
}

struct Snapshot {
    text: String,
    selected: Range<usize>,
}

pub struct TextArea {
    focus: FocusHandle,
    edit: EditBuffer,
    placeholder: SharedString,
    version: usize,
    goal: Option<Pixels>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    last: Option<Edit>,
    layout: Option<Layout>,
    scroll: Pixels,
    reveal: bool,
    selecting: bool,
    extent: Rc<Cell<(Pixels, Pixels)>>,
}

impl EventEmitter<Changed> for TextArea {}

struct Frame {
    layout: Layout,
    selections: Vec<Bounds<Pixels>>,
    cursor: Option<Bounds<Pixels>>,
    placeholder: Option<ShapedLine>,
}

impl TextArea {
    pub fn new(text: &str, placeholder: impl Into<SharedString>, cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle(),
            edit: EditBuffer::new(clean(text), 0),
            placeholder: placeholder.into(),
            version: 0,
            goal: None,
            undo: Vec::new(),
            redo: Vec::new(),
            last: None,
            layout: None,
            scroll: px(0.),
            reveal: false,
            selecting: false,
            extent: Rc::default(),
        }
    }

    pub fn text(&self) -> &str {
        self.edit.text()
    }

    pub fn focus(&self, window: &mut Window) {
        window.focus(&self.focus);
    }

    pub fn focused(&self, window: &Window) -> bool {
        self.focus.is_focused(window)
    }

    pub fn extent(&self) -> Rc<Cell<(Pixels, Pixels)>> {
        self.extent.clone()
    }

    fn current(&self) -> Option<&Layout> {
        self.layout.as_ref().filter(|layout| layout.version == self.version)
    }

    fn cursor(&self) -> usize {
        self.edit.cursor()
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.edit.move_to(offset);
        self.settle(cx);
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.edit.select_to(offset);
        self.settle(cx);
    }

    fn select(&mut self, range: Range<usize>, cx: &mut Context<Self>) {
        self.edit.select(range);
        self.settle(cx);
    }

    fn settle(&mut self, cx: &mut Context<Self>) {
        self.last = None;
        self.goal = None;
        self.reveal = true;
        cx.notify();
    }

    fn travel(&mut self, offset: usize, extend: bool, cx: &mut Context<Self>) {
        if extend {
            self.select_to(offset, cx);
        } else {
            self.move_to(offset, cx);
        }
    }

    fn remember(&mut self, edit: Edit) {
        if self.last == Some(edit) && edit != Edit::Other {
            return;
        }
        self.undo.push(Snapshot { text: self.text().to_string(), selected: self.edit.selected() });
        if self.undo.len() > HISTORY {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.last = Some(edit);
    }

    fn restore(&mut self, redo: bool, cx: &mut Context<Self>) {
        let (from, to) = if redo { (&mut self.redo, &mut self.undo) } else { (&mut self.undo, &mut self.redo) };
        let Some(snapshot) = from.pop() else {
            return;
        };
        let selected = self.edit.selected();
        to.push(Snapshot { text: self.edit.reset(snapshot.text, snapshot.selected), selected });
        self.version += 1;
        self.last = None;
        self.goal = None;
        self.reveal = true;
        cx.emit(Changed);
        cx.notify();
    }

    fn replace(&mut self, range: Range<usize>, text: &str, edit: Edit, cx: &mut Context<Self>) {
        let text = clean(text);
        if range.is_empty() && text.is_empty() {
            return;
        }
        self.remember(edit);
        self.edit.replace(range, &text);
        self.version += 1;
        self.goal = None;
        self.reveal = true;
        cx.emit(Changed);
        cx.notify();
    }

    fn erase(&mut self, forward: bool, words: bool, cx: &mut Context<Self>) {
        let mut range = self.edit.selected();
        if range.is_empty() {
            let cursor = self.cursor();
            let text = self.text();
            let edge = step(text, cursor, forward, words);
            range = cursor.min(edge)..cursor.max(edge);
        }
        self.replace(range, "", Edit::Erasing, cx);
    }

    fn vertical(&mut self, rows: isize, extend: bool, cx: &mut Context<Self>) {
        let Some(layout) = self.current() else {
            return;
        };
        let at = layout.point(self.cursor());
        let x = self.goal.unwrap_or(at.x);
        let y = at.y + layout.line_height() * (rows as f32 + 0.5);
        let target = if y < px(0.) {
            0
        } else if y >= layout.height {
            self.text().len()
        } else {
            layout.index(point(x, y), self.text())
        };
        self.travel(target, extend, cx);
        self.goal = Some(x);
    }

    fn page(&self) -> isize {
        self.layout.as_ref().map_or(1, |layout| ((layout.view() / layout.line_height()) as isize - 1).max(1))
    }

    fn row_edge(&self, end: bool) -> usize {
        let cursor = self.cursor();
        let Some(row) = self.current().and_then(|layout| layout.row(cursor)) else {
            return if end { self.text().len() } else { 0 };
        };
        let local = cursor - row.start;
        let visual = row.visual(local);
        if !end {
            return row.start + row.edge(visual);
        }
        let edge = row.edge(visual + 1);
        if visual + 1 == row.rows() {
            row.start + edge
        } else {
            row.start + previous(tail(self.text(), row.start), edge)
        }
    }

    fn key(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let keys = &event.keystroke;
        let (control, shift) = (keys.modifiers.control, keys.modifiers.shift);
        let cursor = self.cursor();
        let selected = self.edit.selected();
        let collapsed = selected.is_empty();
        let end = self.text().len();
        match keys.key.as_str() {
            "backspace" => self.erase(false, control, cx),
            "delete" => self.erase(true, control, cx),
            "left" if control || shift || collapsed => {
                self.travel(step(self.text(), cursor, false, control), shift, cx)
            }
            "left" => self.move_to(selected.start, cx),
            "right" if control || shift || collapsed => {
                self.travel(step(self.text(), cursor, true, control), shift, cx)
            }
            "right" => self.move_to(selected.end, cx),
            "up" => self.vertical(-1, shift, cx),
            "down" => self.vertical(1, shift, cx),
            "pageup" => self.vertical(-self.page(), shift, cx),
            "pagedown" => self.vertical(self.page(), shift, cx),
            "home" if control => self.travel(0, shift, cx),
            "end" if control => self.travel(end, shift, cx),
            "home" => self.travel(self.row_edge(false), shift, cx),
            "end" => self.travel(self.row_edge(true), shift, cx),
            "enter" if self.edit.marked().is_none() => self.replace(selected, "\n", Edit::Other, cx),
            "a" if control => self.select(0..end, cx),
            "c" | "x" if control => {
                let Some(text) = self.text().get(selected.clone()).filter(|_| !collapsed).map(str::to_string) else {
                    return;
                };
                cx.write_to_clipboard(ClipboardItem::new_string(text));
                if keys.key == "x" {
                    self.replace(selected, "", Edit::Other, cx);
                }
            }
            "v" if control => {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    self.replace(selected, &text, Edit::Other, cx);
                }
            }
            "z" if control && shift => self.restore(true, cx),
            "z" if control => self.restore(false, cx),
            "y" if control => self.restore(true, cx),
            "enter" => {}
            _ => return,
        }
        cx.stop_propagation();
    }

    fn index_at(&self, position: Point<Pixels>) -> usize {
        let Some(layout) = self.current() else {
            return self.cursor();
        };
        layout.index(position - layout.origin(), self.text())
    }

    fn mouse_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus);
        self.selecting = true;
        let index = self.index_at(event.position);
        match event.click_count {
            2 => self.select(word_around(self.text(), index), cx),
            count if count > 2 => {
                let start = head(self.text(), index).rfind('\n').map_or(0, |at| at + 1);
                let end = tail(self.text(), index).find('\n').map_or(self.text().len(), |at| index + at);
                self.select(start..end, cx);
            }
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

    fn scroll_by(&mut self, event: &ScrollWheelEvent, window: &mut Window, cx: &mut Context<Self>) {
        let delta = event.delta.pixel_delta(design::scaled(LINE_HEIGHT, window));
        self.scroll -= delta.y;
        cx.notify();
        cx.stop_propagation();
    }

    fn frame(&self, bounds: Bounds<Pixels>, focused: bool, t: Theme, window: &mut Window) -> Frame {
        let style = window.text_style();
        let font_size = style.font_size.to_pixels(window.rem_size());
        let line_height = design::scaled(LINE_HEIGHT, window);
        let width = (bounds.size.width - design::scaled(PADDING_X * 2., window)).max(px(10.));
        let shown: SharedString = self.text().replace('\t', " ").into();
        let run = |len: usize, color: Hsla, underline: bool| TextRun {
            len,
            font: style.font(),
            color,
            background_color: None,
            underline: underline.then(|| UnderlineStyle { thickness: px(1.), color: Some(color), wavy: false }),
            strikethrough: None,
        };
        let runs = match self.edit.marked() {
            Some(marked) => vec![
                run(marked.start, style.color, false),
                run(marked.len(), style.color, true),
                run(shown.len() - marked.end, style.color, false),
            ],
            None => vec![run(shown.len(), style.color, false)],
        };
        let lines =
            window.text_system().shape_text(shown.clone(), font_size, &runs, Some(width), None).unwrap_or_default();
        let mut rows = Vec::new();
        let mut start = 0;
        let mut top = px(0.);
        for line in lines {
            let breaks = breaks(&line);
            let count = breaks.len() - 1;
            let len = line.len();
            rows.push(Row { start, line, breaks, top });
            top += line_height * count as f32;
            start += len + 1;
        }
        if rows.is_empty() {
            rows.push(Row { start: 0, line: WrappedLine::default(), breaks: vec![0, 0], top: px(0.) });
            top = line_height;
        }
        let height = top;
        let view = (bounds.size.height - design::scaled(PADDING_Y * 2., window)).max(line_height);
        let scale = window.scale_factor();
        let rem_size = window.rem_size();
        let mut layout =
            Layout { version: self.version, rows, bounds, scroll: self.scroll, height, scale, rem: rem_size };
        let caret = layout.point(self.cursor());
        let mut scroll = self.scroll;
        if self.reveal {
            if caret.y < scroll {
                scroll = caret.y;
            }
            if caret.y + line_height > scroll + view {
                scroll = caret.y + line_height - view;
            }
        }
        layout.scroll = scroll.min((height - view).max(px(0.))).max(px(0.));
        let origin = layout.origin();
        let collapsed = self.edit.selected().is_empty();
        let selections = if focused && !collapsed { self.selection_bounds(&layout, origin) } else { Vec::new() };
        let cursor = (focused && collapsed).then(|| {
            let at = layout.point(self.cursor());
            caret_bounds(
                point(origin.x + at.x, origin.y + at.y + layout.scaled(CARET_INSET)),
                line_height - layout.scaled(CARET_INSET * 2.),
                scale,
            )
        });
        let placeholder = self.text().is_empty().then(|| {
            let text = self.placeholder.clone();
            window.text_system().shape_line(text.clone(), font_size, &[run(text.len(), t.faint.into(), false)], None)
        });
        Frame { layout, selections, cursor, placeholder }
    }

    fn selection_bounds(&self, layout: &Layout, origin: Point<Pixels>) -> Vec<Bounds<Pixels>> {
        let line_height = layout.line_height();
        let Range { start: from, end: to } = self.edit.selected();
        let mut rects = Vec::new();
        for row in &layout.rows {
            for visual in 0..row.rows() {
                let (a, b) = (row.start + row.edge(visual), row.start + row.edge(visual + 1));
                let last = visual + 1 == row.rows();
                let beyond = to > b;
                if from > b || to < a || (from == b && !last) {
                    continue;
                }
                let (left, right) = (from.max(a), to.min(b));
                if left == right && !beyond {
                    continue;
                }
                let x1 = row.x_in(visual, left - row.start);
                let x2 = match (beyond, last) {
                    (true, false) => row.x_in(visual, b - row.start),
                    (true, true) => row.x_in(visual, b - row.start) + layout.scaled(NEWLINE_WIDTH),
                    (false, _) => row.x_in(visual, right - row.start),
                };
                let y = origin.y + row.top + line_height * visual as f32;
                let corners = Bounds::from_corners(point(origin.x + x1, y), point(origin.x + x2, y + line_height));
                rects.push(pixel_bounds(corners, layout.scale));
            }
        }
        rects
    }
}

impl Render for TextArea {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t =
            cx.try_global::<Theme>().copied().unwrap_or_else(|| Theme::resolve(ThemeMode::System, window.appearance()));
        let focused = self.focus.is_focused(window);
        let entity = cx.entity();
        let painter = entity.clone();
        let focus = self.focus.clone();
        div()
            .track_focus(&self.focus)
            .size_full()
            .cursor(CursorStyle::IBeam)
            .line_height(rem(LINE_HEIGHT))
            .on_key_down(cx.listener(Self::key))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::mouse_down))
            .on_scroll_wheel(cx.listener(Self::scroll_by))
            .child(
                canvas(
                    move |bounds, window, cx| entity.read(cx).frame(bounds, focused, t, window),
                    move |bounds, frame, window, cx| {
                        window.handle_input(&focus, ElementInputHandler::new(bounds, painter.clone()), cx);
                        let line_height = frame.layout.line_height();
                        let origin = frame.layout.origin();
                        window.with_content_mask(Some(ContentMask { bounds }), |window| {
                            for selection in &frame.selections {
                                window.paint_quad(fill(*selection, t.selection));
                            }
                            if let Some(placeholder) = &frame.placeholder {
                                let _ = placeholder.paint(origin, line_height, window, cx);
                            }
                            for row in &frame.layout.rows {
                                let top = origin.y + row.top;
                                let bottom = top + line_height * row.rows() as f32;
                                if bottom < bounds.top() || top > bounds.bottom() {
                                    continue;
                                }
                                let _ = row.line.paint(
                                    point(origin.x, top),
                                    line_height,
                                    TextAlign::Left,
                                    None,
                                    window,
                                    cx,
                                );
                            }
                            if let Some(cursor) = frame.cursor {
                                window.paint_quad(fill(cursor, t.accent));
                            }
                        });
                        let dragged = painter.clone();
                        window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                            if phase == DispatchPhase::Bubble && event.pressed_button == Some(MouseButton::Left) {
                                dragged.update(cx, |area, cx| area.drag_to(event.position, cx));
                            }
                        });
                        let released = painter.clone();
                        window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                            if phase == DispatchPhase::Bubble && event.button == MouseButton::Left {
                                released.update(cx, |area, _| area.selecting = false);
                            }
                        });
                        let max = (frame.layout.height - frame.layout.view()).max(px(0.));
                        painter.update(cx, |area, _| {
                            area.extent.set((frame.layout.scroll, max));
                            area.scroll = frame.layout.scroll;
                            area.reveal = false;
                            area.layout = Some(frame.layout);
                        });
                    },
                )
                .size_full(),
            )
    }
}

#[cfg(test)]
mod tests;
