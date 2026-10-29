use super::*;
use crate::store::{Location, Meta, shorten_path};

impl Shelf {
    pub(super) fn open_tip(&self) -> Tip {
        Tip::keys(tr().open, &self.keys(Command::Open))
            .line(Tip::keys(tr().reveal_in_folder, &[tr().ctrl_click]).or(&self.keys(Command::Reveal)))
    }

    pub(super) fn title(&self, t: Theme, item: &Item) -> StyledText {
        highlighted(t, item.title.clone(), self.catalog().hits(&item.id))
    }

    fn item_title(&self, t: Theme, item: &Item, width: f32, size: f32, covered: f32, window: &Window) -> Stateful<Div> {
        let pointer = !self.hovered().keyboard();
        let lit = self.hovered().lit(&Hovered::Item(item.id.clone()));
        let title = faded(t, width, self.title(t, item), if lit { t.hover } else { t.bg }, pointer, covered)
            .text_size(rem(size))
            .id(SharedString::from(format!("item-title-{}", item.id)))
            .marked(format!("item-title:{}", item.id));
        if measure(window, &item.title, size) > width - covered {
            title.tooltip(hint(t, item.title.clone()))
        } else {
            title
        }
    }

    pub(super) fn item_events(&self, element: Stateful<Div>, item: &Item, cx: &mut Context<Self>) -> Stateful<Div> {
        let key = item.id.clone();
        let press_key = key.clone();
        let select_key = key.clone();
        let hover_key = key.clone();
        let point_key = key.clone();
        let reveal_key = key.clone();
        let quick_item = item.clone();
        let quick_id = copy_id(&key);
        element
            .cursor_pointer()
            .on_mouse_move(cx.safe_listener(move |shelf, _: &MouseMoveEvent, _, cx| shelf.point(&point_key, cx)))
            .on_mouse_down(
                MouseButton::Left,
                cx.safe_listener(move |shelf, event: &MouseDownEvent, window, cx| {
                    window.focus(shelf.focus_handle(), cx);
                    shelf.press(&press_key, event, window, cx);
                    cx.stop_propagation();
                }),
            )
            .on_hover(cx.safe_listener(move |shelf, hovered: &bool, _, _| {
                shelf.hover(Hovered::Item(hover_key.clone()), *hovered)
            }))
            .on_mouse_down(
                MouseButton::Right,
                cx.safe_listener(move |shelf, _: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    shelf.copy(vec![quick_item.clone()], quick_id.clone(), cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Middle,
                cx.safe_listener(move |shelf, _: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    shelf.open(&reveal_key, true, cx);
                }),
            )
            .on_click(cx.safe_listener(move |shelf, event: &ClickEvent, _, cx| {
                if event.click_count() < 2 {
                    shelf.select(&select_key, event.modifiers(), cx);
                }
            }))
    }

    pub(super) fn item_buttons(&self, t: Theme, item: &Item, lit: bool, cx: &mut Context<Self>) -> Stateful<Div> {
        let key = item.id.clone();
        let open_key = key.clone();
        let remove_key = key.clone();
        let preview_key = key.clone();
        let stick_key = key.clone();
        let copy_item = item.clone();
        let copy_id = copy_id(&key);
        let copied = self.copied(&copy_id, cx);
        let stick_tip = if item.pinned { tr().unpin_item } else { tr().pin_item };
        let buttons = vec![
            row_button(
                t,
                SharedString::from(format!("remove-{key}")),
                "close",
                Tip::keys(tr().remove, &self.keys(Command::Remove)),
            )
            .on_click(cx.safe_listener(move |shelf, _, _, cx| {
                cx.stop_propagation();
                shelf.remove(&remove_key, cx);
            })),
            copy_button(
                t,
                copy_id.clone(),
                copied,
                Tip::keys(tr().copy, &self.keys(Command::Copy)).or(&[tr().right_click]),
                t.bg,
            )
            .on_click(cx.safe_listener(move |shelf, _, _, cx| {
                cx.stop_propagation();
                shelf.copy(vec![copy_item.clone()], copy_id.clone(), cx);
            })),
            row_button(
                t,
                SharedString::from(format!("preview-{key}")),
                "eye",
                Tip::keys(tr().preview, &self.keys(Command::Preview)),
            )
            .on_click(cx.safe_listener(move |shelf, _, window, cx| {
                cx.stop_propagation();
                shelf.show_preview(preview_key.clone(), window, cx);
            })),
            row_button(t, SharedString::from(format!("open-{key}")), "open", self.open_tip()).on_click(
                cx.safe_listener(move |shelf, event: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    shelf.open(&open_key, event.modifiers().control, cx);
                }),
            ),
            bare_button(
                t,
                SharedString::from(format!("stick-{key}")),
                icon("pin", if item.pinned { t.accent } else { t.muted }, glyph::M),
                Tip::keys(stick_tip, &self.keys(Command::Stick)),
                false,
                t.bg,
            )
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.safe_listener(move |shelf, _, _, cx| {
                cx.stop_propagation();
                shelf.toggle_stuck(&stick_key, cx);
            })),
        ];
        div()
            .id("item-buttons")
            .marked(format!("item-buttons:{key}"))
            .flex()
            .gap(rem(BUTTON_GAP))
            .children(buttons)
            .flex_shrink_0()
            .when(!lit, |d| d.invisible())
            .when(!self.hovered().keyboard(), |d| d.group_hover("row", |s| s.visible()))
    }

    fn pin_mark(&self, t: Theme, item: &Item, lit: bool) -> Option<Stateful<Div>> {
        item.pinned.then(|| {
            div()
                .id(SharedString::from(format!("pin-mark-{}", item.id)))
                .marked(format!("pin-mark:{}", item.id))
                .absolute()
                .top_0()
                .right_0()
                .child(div().size(rem(size::BUTTON)).flex().items_center().justify_center().child(icon(
                    "pin",
                    t.accent,
                    glyph::M,
                )))
                .when(lit, |d| d.invisible())
                .when(!self.hovered().keyboard(), |d| d.group_hover("row", |s| s.invisible()))
        })
    }

    pub(super) fn item_state(&self, item: &Item, width: f32, window: &Window) -> (bool, bool, SharedString) {
        let selected = self.selection().contains(&item.id);
        let meta = self.catalog().meta(&item.id);
        let exists = meta.is_none_or(|m| m.exists);
        let detail: SharedString = match meta {
            Some(m) if !m.exists => tr().missing.into(),
            Some(m) => detail(m, width - FADE, window),
            None => SharedString::default(),
        };
        (selected, exists, detail)
    }

    pub(super) fn row(
        &self,
        t: Theme,
        item: &Item,
        text_width: f32,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let key = item.id.clone();
        let (selected, exists, detail) = self.item_state(item, text_width, window);
        let preview = if selected {
            badge_icon(t, "check", ICON_SIZE, true)
        } else if item.kind == Kind::Image && exists {
            badge(t, ICON_SIZE, false).child(img(item.path.clone()).size_full().object_fit(ObjectFit::Cover))
        } else {
            badge_icon(t, item.icon(), ICON_SIZE, false)
        };
        let pointer = !self.hovered().keyboard();
        let lit = self.hovered().lit(&Hovered::Item(key.clone()));
        let row = div()
            .id(SharedString::from(format!("row-{key}")))
            .marked(format!("item:{key}"))
            .group("row")
            .h(rem(ROW_HEIGHT))
            .flex_shrink_0()
            .px(rem(space::PAGE))
            .flex()
            .items_center()
            .gap(rem(space::L))
            .when(pointer, |d| d.hover(move |s| s.bg(t.hover)))
            .when(lit, |d| d.bg(t.hover));
        self.item_events(row, item, cx)
            .child(preview)
            .child(
                subtitled(
                    subtitle_lift(cx),
                    self.item_title(t, item, text_width, TEXT_SIZE, 0., window),
                    faded(t, text_width, detail, if lit { t.hover } else { t.bg }, pointer, 0.).text_color(if exists {
                        t.muted
                    } else {
                        t.danger
                    }),
                )
                .flex_1()
                .min_w_0()
                .overflow_hidden(),
            )
            .child(
                div()
                    .relative()
                    .flex_shrink_0()
                    .child(self.item_buttons(t, item, lit, cx))
                    .children(self.pin_mark(t, item, lit)),
            )
    }

    pub(super) fn cover(&self, t: Theme, item: &Item, exists: bool, width: f32, height: f32) -> Div {
        let frame = div()
            .relative()
            .w_full()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .rounded_t(rem(TILE_RADIUS - size::HAIRLINE))
            .bg(t.surface)
            .border_b_1()
            .border_color(t.border);
        if !exists {
            return frame.flex().items_center().justify_center().child(icon(item.icon(), t.danger, TILE_FALLBACK));
        }
        match item.kind {
            Kind::Image => frame.child(
                img(item.path.clone())
                    .size_full()
                    .rounded_t(rem(TILE_RADIUS - size::HAIRLINE))
                    .object_fit(ObjectFit::Cover),
            ),
            Kind::Text | Kind::Note => frame
                .child(
                    div()
                        .size_full()
                        .p(rem(space::M))
                        .overflow_hidden()
                        .text_size(rem(text::CAPTION))
                        .line_height(rem(line::CAPTION))
                        .text_color(t.muted)
                        .child(
                            item.shown_text().unwrap_or_default().trim().chars().take(TILE_TEXT).collect::<String>(),
                        ),
                )
                .child(div().absolute().left_0().right_0().bottom_0().h(rem(TILE_TEXT_FADE)).bg(fade(t.surface, 180.))),
            Kind::Link => frame.child(
                div().size_full().px(rem(TILE_PADDING)).flex().items_center().justify_center().child(
                    div()
                        .max_w_full()
                        .truncate()
                        .text_size(rem(text::SMALL))
                        .text_color(t.muted)
                        .child(store::host(item.text.as_deref().unwrap_or_default()).to_string()),
                ),
            ),
            Kind::File | Kind::Folder => {
                let frame = frame.flex().items_center().justify_center();
                match self.catalog().thumbnail(&item.path) {
                    Some(image) => frame.child(
                        img(image)
                            .w(rem((width - TILE_THUMB_MARGIN * 2.) * TILE_THUMB_SCALE))
                            .h(rem((height - TILE_THUMB_MARGIN * 2.) * TILE_THUMB_SCALE))
                            .object_fit(ObjectFit::Contain),
                    ),
                    None => frame.child(icon(item.icon(), t.faint, TILE_FALLBACK)),
                }
            }
        }
    }

    pub(super) fn card(
        &self,
        t: Theme,
        item: &Item,
        width: f32,
        height: f32,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let key = item.id.clone();
        let pointer = !self.hovered().keyboard();
        let lit = self.hovered().lit(&Hovered::Item(key.clone()));
        let scale = design::scale(window);
        let edge = design::device(size::HAIRLINE, window.scale_factor()) / scale;
        let text_width = (width - (design::snap(TILE_PADDING, scale) + edge) * 2.).max(20.);
        let (selected, exists, detail) = self.item_state(item, text_width, window);
        let border = if selected { t.accent } else { t.border };
        let hover_border = if selected { t.accent } else { t.border_strong };
        let tile = div()
            .id(SharedString::from(format!("tile-{key}")))
            .marked(format!("item:{key}"))
            .group("row")
            .relative()
            .w(rem(width))
            .h(rem(height))
            .flex_shrink_0()
            .flex()
            .flex_col()
            .rounded(rem(TILE_RADIUS))
            .border_1()
            .border_color(border)
            .bg(if lit { t.hover } else { t.bg })
            .overflow_hidden()
            .when(pointer, |d| d.hover(move |s| s.bg(t.hover).border_color(hover_border)));
        let cover = self.cover(t, item, exists, width - 2., height - TILE_FOOTER - 2.);
        let mark = selected
            .then(|| badge_icon(t, "check", TILE_BADGE, true).rounded(rem(TILE_OVERLAY_RADIUS - size::HAIRLINE)));
        let covered = if mark.is_some() { TILE_BADGE + TILE_BADGE_GAP } else { 0. };
        self.item_events(tile, item, cx)
            .child(cover)
            .child(
                div()
                    .h(rem(TILE_FOOTER))
                    .flex_shrink_0()
                    .px(rem(TILE_PADDING))
                    .py(rem(space::S))
                    .flex()
                    .flex_col()
                    .gap(rem(space::SUBTITLE))
                    .child(
                        self.item_title(t, item, text_width, CARD_TEXT_SIZE, covered, window)
                            .line_height(rem(line::SMALL)),
                    )
                    .child(
                        faded(t, text_width, detail, if lit { t.hover } else { t.bg }, pointer, covered)
                            .text_size(rem(text::CAPTION))
                            .line_height(rem(line::CAPTION))
                            .text_color(if exists { t.muted } else { t.danger }),
                    ),
            )
            .when_some(mark, |d, mark| {
                d.child(
                    mark.absolute()
                        .bottom(rem(TILE_BADGE_MARGIN))
                        .right(rem(TILE_PADDING))
                        .marked(format!("tile-badge:{key}")),
                )
            })
            .child(
                div()
                    .absolute()
                    .top(rem(TILE_INSET))
                    .right(rem(TILE_INSET))
                    .child(overlay(t, self.item_buttons(t, item, lit, cx)))
                    .children(self.pin_mark(t, item, lit).map(|mark| overlay(t, mark))),
            )
    }
}

fn detail(meta: &Meta, room: f32, window: &Window) -> SharedString {
    let with = |place: &str| format!("{} · {place}", meta.detail);
    match &meta.location {
        None => meta.detail.clone().into(),
        Some(Location::Storage) => with(tr().only_in_traydex).into(),
        Some(Location::Folder(folder)) => {
            with(&shorten_path(folder, |place| measure(window, &with(place), text::CAPTION) <= room)).into()
        }
    }
}

fn overlay<E: Styled>(t: Theme, element: E) -> E {
    element
        .p(rem(TILE_OVERLAY_PADDING))
        .rounded(rem(TILE_OVERLAY_RADIUS))
        .bg(t.hover)
        .border_1()
        .border_color(t.border_strong)
}
