use super::*;
use crate::design::Raster;
use crate::shelf::Rows;
use std::ops::Range;

const LIST_PADDING: f32 = space::S;
const GRID_TOP: f32 = space::L;
const OVERSCAN_ROWS: f32 = 2.;
const LAYOUT_ICON_ALPHA: f32 = 0.7;
const SELECTION_BUTTONS: f32 = 3.;

fn selection_span(tabs: &[(f32, f32)], selection: f32, raster: Raster) -> f32 {
    let tabs = tabs
        .iter()
        .map(|(label, count)| raster.text(*label) + raster.rem(COUNT_GAP) + raster.text(*count))
        .sum::<f32>()
        + raster.rem(TAB_GAP) * tabs.len().saturating_sub(1) as f32;
    let bar = raster.text(selection)
        + raster.rem(SELECTION_LABEL_GAP)
        + raster.rem(BAR_BUTTON) * SELECTION_BUTTONS
        + raster.rem(BUTTON_GAP) * (SELECTION_BUTTONS - 1.);
    raster.rem(STRIP_PADDING) * 2. + tabs + raster.rem(SELECTION_MARGIN) + bar
}

impl Shelf {
    pub(super) fn empty_buttons(&self, t: Theme, cx: &mut Context<Self>) -> Div {
        div()
            .mt(rem(space::XL))
            .px(rem(space::PAGE))
            .flex()
            .flex_wrap()
            .justify_center()
            .gap(rem(space::M))
            .child(
                self.shortcut_button(t, "paste", Command::Paste, tr().paste_from_clipboard)
                    .on_click(cx.safe_listener(|shelf, _, _, cx| shelf.paste(cx))),
            )
            .child(
                self.shortcut_button(t, "empty-note", Command::Note, tr().create_note)
                    .on_click(cx.safe_listener(|shelf, _, window, cx| shelf.new_note(window, cx))),
            )
    }

    fn empty_list(&self, t: Theme, cx: &mut Context<Self>) -> Div {
        let notes = self.filter() == Filter::Notes && !self.catalog().searching();
        let text = match (self.catalog().searching(), notes) {
            (true, _) => tr().nothing_found,
            (false, true) => tr().no_notes,
            (false, false) => tr().empty_category,
        };
        div()
            .marked("empty-list")
            .flex_1()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(rem(space::XL))
            .text_color(t.faint)
            .child(text)
            .when(notes, |d| {
                d.child(
                    self.shortcut_button(t, "first-note", Command::Note, tr().create_note)
                        .on_click(cx.safe_listener(|shelf, _, window, cx| shelf.new_note(window, cx))),
                )
            })
    }

    pub(super) fn drag_all(&self, t: Theme, count: usize, cx: &mut Context<Self>) -> Stateful<Div> {
        let lit = self.hovered().is(&Hovered::DragAll);
        let color = if lit { t.text } else { t.muted };
        bottom_bar(t)
            .id("drag-all")
            .marked("drag-all")
            .text_color(color)
            .cursor(CursorStyle::OpenHand)
            .hover(move |s| s.bg(t.hover))
            .on_hover(cx.safe_listener(|shelf, hovered: &bool, _, cx| {
                shelf.hover(Hovered::DragAll, *hovered);
                cx.notify();
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.safe_listener(|shelf, event: &MouseDownEvent, window, cx| {
                    window.focus(shelf.focus_handle(), cx);
                    shelf.press_all(event);
                    cx.stop_propagation();
                }),
            )
            .child(leading_icon("upload", color, glyph::M))
            .child(tr().drag_all)
            .child(div().flex_1().text_color(t.faint).child(count.to_string()))
            .child(div().size(rem(BAR_BUTTON)).flex().items_center().justify_center().child(icon(
                "grip",
                if lit { t.muted } else { t.faint },
                16.,
            )))
    }

    pub(super) fn tabs(&self, t: Theme, selection: bool, cx: &mut Context<Self>) -> Div {
        let active = self.shown_tabs().position(|(_, filter)| filter == self.filter());
        strip(t)
            .marked("tabs")
            .justify_between()
            .gap(rem(TAB_GAP))
            .child(
                div()
                    .marked("tab-area")
                    .relative()
                    .h_full()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .child(
                        div()
                            .id("tab-strip")
                            .size_full()
                            .flex()
                            .gap(rem(TAB_GAP))
                            .overflow_x_scroll()
                            .track_scroll(self.lane(Lane::Tabs))
                            .children(self.shown_tabs().map(|(index, filter)| {
                                let active = self.filter() == filter;
                                let count = self.count(filter);
                                let line: Hsla = if active { t.text.into() } else { transparent_black() };
                                div()
                                    .id(("tab", index))
                                    .marked(format!("tab-{index}"))
                                    .relative()
                                    .h_full()
                                    .flex()
                                    .flex_shrink_0()
                                    .items_center()
                                    .gap(rem(COUNT_GAP))
                                    .cursor_pointer()
                                    .text_color(if active { t.text } else { t.muted })
                                    .hover(move |s| s.text_color(t.text))
                                    .child(filter.label())
                                    .child(div().text_color(t.faint).child(count.to_string()))
                                    .child(
                                        div().absolute().left_0().right_0().bottom_0().h(px(size::HAIRLINE)).bg(line),
                                    )
                                    .on_click(cx.safe_listener(move |shelf, _, _, cx| shelf.set_filter(filter, cx)))
                            })),
                    )
                    .children(self.lane_covers(t, Lane::Tabs, active, cx)),
            )
            .child(if selection { self.selection_bar(t, false, cx) } else { self.shelf_buttons(t, cx) })
    }

    pub(super) fn shelf_buttons(&self, t: Theme, cx: &mut Context<Self>) -> Div {
        let (glyph, tip) = match self.store().settings().layout {
            Layout::List => ("grid", tr().layout_grid),
            Layout::Grid => ("list", tr().layout_list),
        };
        div()
            .marked("shelf-buttons")
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(rem(BUTTON_GAP))
            .child(
                bare_button(
                    t,
                    "layout",
                    icon(glyph, Rgba { a: LAYOUT_ICON_ALPHA, ..t.faint }, glyph::M),
                    Tip::keys(tip, &self.keys(Command::Layout)),
                    false,
                    t.hover,
                )
                .marked("layout")
                .on_click(cx.safe_listener(|shelf, _, _, cx| shelf.toggle_layout(cx))),
            )
            .child(
                icon_button(t, "new-note", "plus", Tip::keys(tr().new_note, &self.keys(Command::Note)), false)
                    .marked("new-note")
                    .on_click(cx.safe_listener(|shelf, _, window, cx| shelf.new_note(window, cx))),
            )
            .child(
                icon_button(t, "storage", "folder", Tip::keys(tr().storage, &self.keys(Command::Storage)), false)
                    .on_click(cx.safe_listener(|shelf, _, _, _| shelf.open_storage())),
            )
            .child(
                icon_button(t, "clear", "trash", Tip::keys(tr().clear_hint, &self.keys(Command::Clear)), false)
                    .on_click(cx.safe_listener(|shelf, _, _, cx| shelf.ask_clear(cx))),
            )
    }

    pub(super) fn selection_bar(&self, t: Theme, spread: bool, cx: &mut Context<Self>) -> Div {
        div()
            .marked("selection-bar")
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(rem(SELECTION_LABEL_GAP))
            .when(spread, |d| d.w_full().justify_between())
            .child(div().text_color(t.muted).child(fill(tr().selected, self.selection().len())))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(rem(BUTTON_GAP))
                    .child(
                        copy_button(
                            t,
                            COPY_SELECTED,
                            self.copied(COPY_SELECTED, cx),
                            Tip::keys(tr().copy, &self.keys(Command::Copy)),
                            t.hover,
                        )
                        .on_click(cx.safe_listener(|shelf, _, _, cx| shelf.copy_selection(cx))),
                    )
                    .child(
                        icon_button(
                            t,
                            "remove-selected",
                            "trash",
                            Tip::keys(tr().remove, &self.keys(Command::Remove)),
                            false,
                        )
                        .on_click(cx.safe_listener(|shelf, _, _, cx| shelf.remove_selected(cx))),
                    )
                    .child(
                        icon_button(t, "deselect", "close", Tip::keys(tr().deselect_hint, &[tr().escape_key]), false)
                            .on_click(cx.safe_listener(|shelf, _, _, cx| shelf.clear_selection(cx))),
                    ),
            )
    }

    pub(super) fn count(&self, filter: Filter) -> usize {
        self.store().items().iter().filter(|i| self.shows(filter, i) && self.found(i)).count()
    }

    pub(super) fn selection_fits(&self, window: &Window) -> bool {
        let tabs: Vec<(f32, f32)> = self
            .shown_tabs()
            .map(|(_, filter)| {
                (text_width(window, filter.label()), text_width(window, &self.count(filter).to_string()))
            })
            .collect();
        let selection = text_width(window, &fill(tr().selected, self.selection().len()));
        let raster = Raster::of(window);
        selection_span(&tabs, selection, raster) <= raster.px(window.viewport_size().width.into())
    }

    pub(super) fn shelf_page(&self, t: Theme, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let viewport = window.viewport_size();
        if self.dropping() {
            return self.drop_targets(t, viewport.map(|side| units(side, window)), cx);
        }
        let launcher = !self.tiles().is_empty();
        if self.store().items().is_empty() {
            return div()
                .flex_1()
                .min_h_0()
                .flex()
                .flex_col()
                .child(self.drop_area(t, false, self.session().look().hovering, cx))
                .when(launcher, |d| d.child(self.tile_strip(t, window, cx).mb(rem(space::PAGE))))
                .into_any_element();
        }
        let items = self.visible_items();
        let empty = items.is_empty();
        let count = items.len();
        let fits = self.selection().is_empty() || self.selection_fits(window);
        let (inline, stacked) = (!self.selection().is_empty() && fits, !fits);
        let grid_mode = self.store().settings().layout == Layout::Grid;
        let rows =
            if grid_mode { self.grid_rows(t, &items, window, cx) } else { self.list_rows(t, &items, window, cx) };
        let list = div()
            .id("items")
            .py(rem(LIST_PADDING))
            .flex()
            .flex_col()
            .when(grid_mode && !empty, |d| d.px(rem(STRIP_PADDING)).pt(rem(GRID_TOP)).gap(rem(TILE_GAP)))
            .on_click(cx.safe_listener(|shelf, _, _, cx| shelf.clear_selection(cx)))
            .when(empty, |d| d.child(self.empty_list(t, cx)))
            .children(rows);
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(
                self.drop_area(t, true, self.session().look().hovering, cx).mt(rem(space::PAGE)).mb(rem(LIST_PADDING)),
            )
            .when(launcher, |d| d.child(self.tile_strip(t, window, cx).mb(rem(LIST_PADDING))))
            .child(if stacked { strip(t).child(self.selection_bar(t, true, cx)) } else { self.tabs(t, inline, cx) })
            .child(self.scroll_area(t, Page::Shelf, "items", list))
            .when(!empty, |d| d.child(self.drag_all(t, count, cx)))
            .into_any_element()
    }
}

impl Shelf {
    fn list_rows(&self, t: Theme, items: &[&Item], window: &Window, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let viewport = window.viewport_size();
        let text_width = (units(viewport.width, window) - ROW_CHROME).max(40.);
        let (top, height) = (scaled(LIST_PADDING, window), scaled(ROW_HEIGHT, window));
        let rows = Rows::snapped(1, top.into(), height.into(), 0., window.scale_factor());
        self.record_rows(rows);
        let shown = self.rendered_rows(viewport.height, rows, items.len());
        let built = items.get(shown.clone()).unwrap_or_default().iter();
        spaced(rows, shown, items.len(), built.map(|item| self.row(t, item, text_width, window, cx).into_any_element()))
    }

    fn grid_rows(&self, t: Theme, items: &[&Item], window: &Window, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let viewport = window.viewport_size();
        let scale = design::scale(window);
        let device = |logical: f32| design::device(logical, scale);
        let available = device(units(viewport.width, window)) - device(STRIP_PADDING) * 2.;
        let gap = device(TILE_GAP);
        let least = device(TILE_SIZE).min(available);
        let columns = (((available + gap) / (least + gap)).floor() as usize).max(2);
        let room = available - gap * (columns - 1) as f32;
        let fill = (room / columns as f32).floor();
        let (base, wider) = if self.store().items().len() >= columns {
            (fill, (room - fill * columns as f32) as usize)
        } else {
            (least.min(fill), 0)
        };
        let height = base / scale;
        let real = |design: f32| f32::from(scaled(design, window));
        let rows = Rows::snapped(columns, real(GRID_TOP), real(height), real(TILE_GAP), window.scale_factor());
        self.record_rows(rows);
        let total = items.len().div_ceil(columns);
        let shown = self.rendered_rows(viewport.height, rows, total);
        let built = items.chunks(columns).skip(shown.start).take(shown.len()).map(|chunk| {
            let cards = chunk.iter().enumerate().map(|(column, item)| {
                let width = base + if column < wider { 1. } else { 0. };
                self.card(t, item, width / scale, height, window, cx)
            });
            div().flex().gap(rem(TILE_GAP)).children(cards).into_any_element()
        });
        spaced(rows, shown, total, built)
    }

    fn rendered_rows(&self, fallback: Pixels, rows: Rows, total: usize) -> Range<usize> {
        let scroll = self.scroll(Page::Shelf);
        let height = match f32::from(scroll.bounds().size.height) {
            height if height > 0. => height,
            _ => fallback.into(),
        };
        let scrolled = -f32::from(scroll.offset().y);
        let first = ((scrolled - rows.top) / rows.pitch - OVERSCAN_ROWS).floor().max(0.) as usize;
        let last = ((scrolled - rows.top + height) / rows.pitch + OVERSCAN_ROWS).ceil().max(0.) as usize;
        first.min(total)..last.min(total)
    }
}

fn spaced(rows: Rows, shown: Range<usize>, total: usize, built: impl Iterator<Item = AnyElement>) -> Vec<AnyElement> {
    let spacer = |count: usize| {
        (count > 0).then(|| div().h(px(count as f32 * rows.pitch - rows.gap)).flex_shrink_0().into_any_element())
    };
    spacer(shown.start).into_iter().chain(built).chain(spacer(total.saturating_sub(shown.end))).collect()
}

#[cfg(test)]
mod tests;
