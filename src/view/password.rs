use super::actions::labelled;
use super::faded;
use super::widgets::{Look, Marked, heading, heading_width, hint, page, text_button};
use crate::design::{line, rem, size, space, text, units};
use crate::i18n::{fill, tr};
use crate::shelf::{Attempt, Page, SafeListener, Shelf, Unlocking};
use crate::theme::Theme;
use gpui::{prelude::*, *};

impl Shelf {
    pub(super) fn password_page(&self, t: Theme, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(unlocking) = self.unlocking() else {
            return self.shelf_page(t, window, cx);
        };
        let wrong = unlocking.attempt == Attempt::Wrong;
        let trying = matches!(unlocking.attempt, Attempt::Trying(_));
        let ready = !trying && !unlocking.password.read(cx).text().is_empty();
        let content = page("password")
            .child(password_header(t, unlocking, window))
            .child(
                labelled(t, tr().password_label)
                    .marked("password-field")
                    .flex_shrink_0()
                    .child(unlocking.password.clone())
                    .child(
                        div()
                            .marked("password-error")
                            .h(rem(line::SMALL))
                            .text_size(rem(text::SMALL))
                            .text_color(t.danger)
                            .when(wrong, |d| d.child(tr().wrong_password)),
                    ),
            )
            .child(self.password_buttons(t, ready, cx));
        self.scroll_area(t, Page::Password, "password-page", content)
    }

    fn password_buttons(&self, t: Theme, ready: bool, cx: &mut Context<Self>) -> Div {
        div()
            .marked("password-buttons")
            .flex_shrink_0()
            .flex()
            .justify_between()
            .gap(rem(space::M))
            .child(
                text_button(t, "password-cancel", tr().cancel, Look::Plain)
                    .marked("password-cancel")
                    .on_click(cx.safe_listener(|shelf, _, _, cx| shelf.toggle_page(Page::Shelf, cx))),
            )
            .child(
                text_button(t, "password-unpack", tr().unpack_button, Look::primary_when(ready))
                    .marked("password-unpack")
                    .when(ready, |d| d.on_click(cx.safe_listener(|shelf, _, _, cx| shelf.unlock(cx)))),
            )
    }
}

fn password_header(t: Theme, unlocking: &Unlocking, window: &Window) -> Div {
    let name = unlocking
        .archives
        .first()
        .and_then(|archive| archive.file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let info = match unlocking.archives.len() {
        1 => tr().password_one.to_string(),
        count => fill(tr().password_many, count),
    };
    let room = units(window.viewport_size().width, window) - (space::PAGE * 2. + size::PAGE_BADGE + space::XL);
    let clipped = heading_width(window, &name) > room;
    let title = div()
        .id("password-title-text")
        .child(faded(t, room, name.clone(), t.bg, false, 0.))
        .when(clipped, |d| d.tooltip(hint(t, name)));
    heading(t, "password", "unpack", title, div().w(rem(room)).truncate().child(info))
}
