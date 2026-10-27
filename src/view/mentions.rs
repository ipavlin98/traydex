use super::*;
use crate::store::ItemId;
use crate::textarea::Peek;

const WIDTH: f32 = 300.;
const VISIBLE_ROWS: f32 = 7.;
const ROW: f32 = size::CONTROL;
const LIST_HEIGHT: f32 = ROW * VISIBLE_ROWS;
const MENU_PADDING: f32 = radius::LARGE - size::HAIRLINE - radius::SMALL;
const PICK_BADGE: f32 = glyph::XL;
const DETAIL_WIDTH: f32 = 96.;
const PEEK_IMAGE: f32 = 168.;
const GAP: f32 = space::XS;
const SNIPPET_LINES: f32 = 4.;
const SNIPPET_CHARS: usize = 400;
const CITE_WIDTH: f32 = 160.;

fn floating(t: Theme) -> Div {
    div()
        .w(rem(WIDTH))
        .flex()
        .flex_col()
        .rounded(rem(radius::LARGE))
        .border_1()
        .border_color(t.border_strong)
        .bg(t.surface)
}

fn place(around: Bounds<Pixels>, height: f32, window: &Window) -> (Point<Pixels>, gpui::Corner) {
    let gap = scaled(GAP, window);
    let below = around.bottom() + gap;
    if below + scaled(height + GAP, window) <= window.viewport_size().height {
        (point(around.left(), below), gpui::Corner::TopLeft)
    } else {
        (point(around.left(), around.top() - gap), gpui::Corner::BottomLeft)
    }
}

fn floated(element: impl IntoElement, at: (Point<Pixels>, gpui::Corner)) -> Deferred {
    deferred(anchored().position(at.0).anchor(at.1).snap_to_window_with_margin(px(space::XS)).child(element))
        .with_priority(1)
}

impl Shelf {
    pub(super) fn note_overlays(&self, t: Theme, window: &Window, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let Some(note) = self.preview().and_then(|preview| preview.note.as_deref()) else {
            return Vec::new();
        };
        let mut overlays = Vec::new();
        if let (Some(picker), Some(anchor)) = (&note.picker, note.area.read(cx).anchor()) {
            let rows = picker.hits.len().clamp(1, VISIBLE_ROWS as usize) as f32;
            let height = ROW * rows + MENU_PADDING * 2.;
            let menu = self.mention_menu(t, &picker.hits, picker.chosen, &picker.scroll, cx);
            overlays.push(floated(menu, place(anchor, height, window)).into_any_element());
        } else if let Some(peek) = &note.peek {
            overlays.push(self.peek_card(t, peek, window, cx));
        }
        overlays
    }

    fn mention_menu(
        &self,
        t: Theme,
        hits: &[ItemId],
        chosen: usize,
        scroll: &ScrollHandle,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let rows: Vec<AnyElement> = hits
            .iter()
            .enumerate()
            .filter_map(|(index, id)| Some(self.mention_row(t, self.store().item(id)?, index == chosen, index, cx)))
            .collect();
        let list = if rows.is_empty() {
            div()
                .id("mention-list")
                .h(rem(ROW))
                .px(rem(space::M))
                .flex()
                .items_center()
                .text_size(rem(text::SMALL))
                .text_color(t.faint)
                .child(tr().mention_none)
        } else {
            div()
                .id("mention-list")
                .marked("mention-list")
                .max_h(rem(LIST_HEIGHT))
                .flex()
                .flex_col()
                .overflow_y_scroll()
                .track_scroll(scroll)
                .children(rows)
        };
        floating(t).id("mention-menu").marked("mention-menu").p(rem(MENU_PADDING)).occlude().child(list).enter(
            "mention-menu-enter",
            Motion::Drop,
            self.animated(),
        )
    }

    fn mention_row(&self, t: Theme, item: &Item, chosen: bool, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let id = item.id.clone();
        let detail = self.catalog().meta(&item.id).map(|meta| meta.detail.clone()).unwrap_or_default();
        div()
            .id(("mention-pick", index))
            .marked(format!("mention-pick:{index}"))
            .h(rem(ROW))
            .flex_shrink_0()
            .px(rem(space::XS))
            .flex()
            .items_center()
            .gap(rem(space::M))
            .rounded(rem(radius::SMALL))
            .cursor_pointer()
            .text_size(rem(text::SMALL))
            .text_color(if chosen { t.text } else { t.muted })
            .when(chosen, |d| d.bg(t.hover))
            .child(self.mention_badge(t, item, PICK_BADGE))
            .child(div().flex_1().min_w_0().truncate().child(item.title.clone()))
            .child(div().flex_shrink_0().max_w(rem(DETAIL_WIDTH)).truncate().text_color(t.faint).child(detail))
            .on_mouse_move(cx.safe_listener(move |shelf, _: &MouseMoveEvent, _, cx| shelf.point_pick(index, cx)))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.safe_listener(move |shelf, _, _, cx| shelf.pick_mention(&id, cx)))
            .into_any_element()
    }

    fn mention_badge(&self, t: Theme, item: &Item, side: f32) -> Div {
        if item.kind == Kind::Image && self.catalog().exists(&item.id) {
            badge(t, side, false).child(img(item.path.clone()).size_full().object_fit(ObjectFit::Cover))
        } else {
            badge_icon(t, item.icon(), side, false)
        }
    }

    fn peek_card(&self, t: Theme, peek: &Peek, window: &Window, cx: &App) -> AnyElement {
        let card = floating(t).id("mention-peek").marked("mention-peek").p(rem(space::M)).gap(rem(space::M));
        let Some(item) = self.store().item(&peek.id) else {
            let gone = div().text_size(rem(text::SMALL)).text_color(t.faint).child(tr().mention_gone);
            return floated(card.child(gone), place(peek.bounds, size::CONTROL, window)).into_any_element();
        };
        let exists = self.catalog().exists(&item.id);
        let detail = match self.catalog().meta(&item.id) {
            Some(meta) if exists => meta.detail.clone(),
            _ => tr().missing.to_string(),
        };
        let visual = self.peek_visual(t, item, exists);
        let height = if visual.is_some() { PEEK_IMAGE + space::M } else { 0. } + size::ROW_BADGE + line::SMALL * 2.;
        let head =
            div().flex().items_center().gap(rem(space::L)).child(self.mention_badge(t, item, size::ROW_BADGE)).child(
                subtitled(
                    subtitle_lift(cx),
                    div().truncate().text_color(t.text).child(item.title.clone()),
                    div().truncate().text_color(if exists { t.muted } else { t.danger }).child(detail),
                )
                .flex_1()
                .min_w_0(),
            );
        let hint = div()
            .text_size(rem(text::SMALL))
            .line_height(rem(line::SMALL))
            .text_color(t.faint)
            .child(tr().mention_hint);
        let card =
            card.children(visual).child(head).child(hint).enter("mention-peek-enter", Motion::Drop, self.animated());
        floated(card, place(peek.bounds, height, window)).into_any_element()
    }

    fn peek_visual(&self, t: Theme, item: &Item, exists: bool) -> Option<AnyElement> {
        if !exists {
            return None;
        }
        let frame = || {
            div()
                .w_full()
                .h(rem(PEEK_IMAGE))
                .flex()
                .items_center()
                .justify_center()
                .rounded(rem(radius::SMALL))
                .bg(t.bg)
        };
        match item.kind {
            Kind::Image => Some(
                frame().child(img(item.path.clone()).size_full().object_fit(ObjectFit::Contain)).into_any_element(),
            ),
            Kind::File | Kind::Folder => self
                .catalog()
                .thumbnail(&item.path)
                .map(|image| frame().child(img(image).size_full().object_fit(ObjectFit::Contain)).into_any_element()),
            Kind::Text | Kind::Note | Kind::Link => {
                let text: String = item.shown_text().unwrap_or_default().trim().chars().take(SNIPPET_CHARS).collect();
                (!text.is_empty()).then(|| {
                    div()
                        .max_h(rem(line::SMALL * SNIPPET_LINES))
                        .overflow_hidden()
                        .text_size(rem(text::SMALL))
                        .line_height(rem(line::SMALL))
                        .text_color(t.muted)
                        .child(text)
                        .into_any_element()
                })
            }
        }
    }

    pub(super) fn cite_link(&self, t: Theme, item: &Item, cx: &mut Context<Self>) -> Option<AnyElement> {
        let notes: Vec<&Item> =
            self.catalog().citing(&item.id).iter().filter_map(|note| self.store().item(note)).collect();
        let shown = self.preview().map_or(0, |preview| preview.cited).min(notes.len().saturating_sub(1));
        let note = notes.get(shown)?;
        let names: Vec<&str> = notes.iter().map(|note| note.title.as_str()).collect();
        let position = (notes.len() > 1).then(|| format!("{} / {}", shown + 1, notes.len()));
        let key = item.id.clone();
        Some(
            div()
                .id("preview-cited")
                .marked("preview-cited")
                .flex_shrink_0()
                .max_w(rem(CITE_WIDTH))
                .flex()
                .items_center()
                .gap(rem(space::XS))
                .cursor_pointer()
                .text_color(t.muted)
                .hover(move |s| s.text_color(t.text))
                .child(icon("note", t.muted, glyph::S))
                .child(div().min_w_0().truncate().child(note.title.clone()))
                .children(position.map(|position| div().flex_shrink_0().text_color(t.faint).child(position)))
                .tooltip(hint(t, fill(tr().cited_in, names.join(", "))))
                .on_scroll_wheel(cx.safe_listener(|shelf, event: &ScrollWheelEvent, _, cx| {
                    let delta = event.delta.pixel_delta(px(1.)).y;
                    if delta != px(0.) {
                        shelf.turn_cited(if delta < px(0.) { 1 } else { -1 }, cx);
                    }
                    cx.stop_propagation();
                }))
                .on_click(cx.safe_listener(move |shelf, _, window, cx| shelf.open_citing(&key, window, cx)))
                .into_any_element(),
        )
    }
}
