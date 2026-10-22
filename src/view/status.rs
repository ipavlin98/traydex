use super::*;

impl Shelf {
    pub(super) fn status_visible(&self) -> bool {
        if self.page() == Page::Shelf && self.dropping() {
            return false;
        }
        self.searchable() || self.confirming().is_some()
    }

    pub(super) fn notice_bar(&self, t: Theme, window: &Window, cx: &mut Context<Self>) -> Option<Div> {
        if self.notice_place() == Place::Title {
            return None;
        }
        let notices = self.notices(cx);
        let (glyph, color, text, notice) = match notices.alert() {
            Some(alert) => ("alert", t.danger, alert.clone(), Notice::Alert),
            None => ("check", t.success, notices.toast()?.clone(), Notice::Toast),
        };
        let width = units(window.viewport_size().width, window) - (BAR_PADDING * 2. + BAR_ICON + BAR_GAP);
        let clipped = measure(window, &text, BAR_TEXT) > width;
        let above = if self.status_visible() { STATUS_HEIGHT } else { 0. };
        Some(
            div().absolute().left_0().right_0().bottom(rem(above)).child(
                bottom_bar(t)
                    .id("notice")
                    .marked("notice")
                    .bg(t.surface)
                    .occlude()
                    .child(div().w(rem(BAR_ICON)).flex().justify_center().child(leading_icon(glyph, color, BAR_ICON)))
                    .child(faded(t, width, text.clone(), t.surface, false, 0.))
                    .when(clipped, |d| d.tooltip(hint(t, text.clone())))
                    .on_hover(
                        cx.safe_listener(move |shelf, hovered: &bool, _, cx| shelf.hold_notice(notice, *hovered, cx)),
                    )
                    .when(notice == Notice::Alert, |d| {
                        d.cursor_pointer()
                            .on_click(cx.safe_listener(|shelf, _, _, cx| shelf.dismiss_notice(Notice::Alert, cx)))
                    })
                    .enter(text, Motion::Rise, self.animated()),
            ),
        )
    }

    pub(super) fn focus_search(
        &self,
        cx: &mut Context<Self>,
    ) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static {
        cx.safe_listener(|shelf, _: &MouseDownEvent, window, cx| {
            shelf.search_input().read(cx).focus(window);
            window.prevent_default();
            cx.stop_propagation();
            cx.notify();
        })
    }

    pub(super) fn search_field(&self, t: Theme, window: &Window, cx: &mut Context<Self>) -> Div {
        let input = self.search_input();
        let focused = input.read(cx).focused(window);
        div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .items_center()
            .gap(rem(BAR_GAP))
            .cursor(CursorStyle::IBeam)
            .on_mouse_down(MouseButton::Left, self.focus_search(cx))
            .child(div().w(rem(BAR_ICON)).flex().justify_center().child(leading_icon(
                "search",
                if focused { t.text } else { t.faint },
                BAR_ICON,
            )))
            .child(div().flex_1().min_w_0().child(input.clone()))
    }

    pub(super) fn search_trailing(&self, t: Theme, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let input = self.search_input().read(cx);
        if !input.text().is_empty() {
            return Some(
                icon_button(t, "clear-search", "close", Tip::keys(tr().clear_search, &[tr().escape_key]), false)
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.safe_listener(|shelf, _, window, cx| {
                        shelf.clear_current_search(cx);
                        shelf.search_input().read(cx).focus(window);
                    }))
                    .into_any_element(),
            );
        }
        if input.focused(window) {
            return None;
        }
        Some(
            div()
                .h(rem(BAR_BUTTON))
                .flex()
                .items_center()
                .cursor_pointer()
                .on_mouse_down(MouseButton::Left, self.focus_search(cx))
                .child(kbd(t, KEYS_SEARCH))
                .into_any_element(),
        )
    }

    pub(super) fn status_bar(&self, t: Theme, window: &Window, cx: &mut Context<Self>) -> Div {
        let bar = bottom_bar(t).marked("status-bar").justify_between().bg(t.surface);
        if let Some(confirm) = &self.confirming() {
            let (question, action, key): (SharedString, _, usize) = match confirm {
                Confirm::Clear if self.clear_loses_files() => (tr().remove_confirm.into(), tr().delete, 0),
                Confirm::Clear if self.keeps_pinned() => (tr().clear_confirm_pinned.into(), tr().clear, 0),
                Confirm::Clear => (tr().clear_confirm.into(), tr().clear, 0),
                Confirm::Reset => (tr().reset_all_confirm.into(), tr().reset, 1),
                Confirm::Uninstall(id) => {
                    let name = self.plugin(id).map_or("", |plugin| plugin.name());
                    (fill(tr().plugin_uninstall_confirm, name).into(), tr().uninstall, 2)
                }
                Confirm::Update(release) => {
                    (fill(tr().update_available, &release.version).into(), tr().update_install, 3)
                }
                Confirm::Remove(_) => (tr().remove_confirm.into(), tr().delete, 4),
            };
            let (cancel, look) = match confirm {
                Confirm::Update(_) => (tr().later, Look::Primary),
                Confirm::Clear | Confirm::Reset | Confirm::Uninstall(_) | Confirm::Remove(_) => {
                    (tr().cancel, Look::Danger)
                }
            };
            let animated = self.animated();
            return bar
                .child(div().marked("confirm-question").min_w_0().truncate().child(question).enter(
                    ("confirm-question", key),
                    Motion::Fade,
                    animated,
                ))
                .child(
                    div()
                        .flex()
                        .gap(rem(space::S))
                        .child(
                            text_button(t, "cancel-clear", cancel, Look::Plain)
                                .marked("cancel-clear")
                                .on_click(cx.safe_listener(|shelf, _, _, cx| shelf.cancel_confirm(cx))),
                        )
                        .child(
                            text_button(t, "confirm-action", action, look)
                                .marked("confirm-action")
                                .on_click(cx.safe_listener(|shelf, _, window, cx| shelf.confirm(window, cx))),
                        )
                        .enter(("confirm-buttons", key), Motion::Fade, animated),
                );
        }
        bar.child(self.search_field(t, window, cx)).child(
            div()
                .flex()
                .items_center()
                .gap(rem(BUTTON_GAP))
                .when_some(self.search_trailing(t, window, cx), |d, trailing| d.child(trailing)),
        )
    }
}
