use super::TITLE_HEIGHT;
use super::widgets::{Look, Marked, Tip, copy_button, disabled_button, measure, skeleton, skeleton_line, text_button};
use crate::actions::Pending;
use crate::design::{self, line, radius, rem, space, text, units};
use crate::i18n::{fill, tr};
use crate::keys::Command;
use crate::shelf::{Page, QR_COPY, SHARE_COPY, SafeListener, Shelf};
use crate::theme::Theme;
use gpui::{prelude::*, *};
use qrcodegen::QrCode;
use std::path::PathBuf;

const GROUP_GAP: f32 = space::X3L;
const ITEM_GAP: f32 = space::XS;
const TEXT_LINE: f32 = line::BODY;
const URL_TEXT: f32 = text::SMALL;
const QUIET_ZONE: i32 = 4;
const CODE_MIN: f32 = 120.;
const CODE_MAX: f32 = 240.;
const CODE_ROOM: f32 = 210.;
const QR_MAX: f32 = 320.;
const QR_ROOM: f32 = 140.;
const NAME_SKELETON: f32 = 160.;
const URL_SAMPLE: &str = "http://192.168.100.100:50000/0000000000000000000000000";

fn code(t: Theme, qr: QrCode, side: f32) -> impl IntoElement {
    let count = qr.size() + QUIET_ZONE * 2;
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            window.paint_quad(gpui::fill(bounds, t.code_paper));
            let scale = window.scale_factor();
            let side = f32::from(bounds.size.width);
            let module = (side * scale / count as f32).floor().max(1.) / scale;
            let inset = (side - module * count as f32) / 2.;
            let snap = |value: Pixels| px(design::snap(value.into(), scale));
            let origin = point(snap(bounds.origin.x + px(inset)), snap(bounds.origin.y + px(inset)));
            let cell = |x: i32, y: i32| {
                point(origin.x + px((x + QUIET_ZONE) as f32 * module), origin.y + px((y + QUIET_ZONE) as f32 * module))
            };
            for y in 0..qr.size() {
                let mut x = 0;
                while x < qr.size() {
                    let start = x;
                    while x < qr.size() && qr.get_module(x, y) {
                        x += 1;
                    }
                    if x > start {
                        let run = size(px((x - start) as f32 * module), px(module));
                        window.paint_quad(gpui::fill(Bounds::new(cell(start, y), run), t.code_ink));
                    }
                    x += 1;
                }
            }
        },
    )
    .size(rem(side))
    .flex_shrink_0()
}

fn code_skeleton(t: Theme, id: &'static str, side: f32, animated: bool) -> AnyElement {
    skeleton(t, id, div().size(rem(side)).flex_shrink_0().rounded(rem(radius::SMALL)), animated)
}

fn line_skeleton(t: Theme, id: &'static str, width: f32, animated: bool) -> Div {
    div().w(rem(width)).h(rem(TEXT_LINE)).flex().items_center().child(skeleton_line(t, id, animated))
}

fn shared_files(files: &[PathBuf]) -> String {
    match files {
        [file] => file.file_name().unwrap_or(file.as_os_str()).to_string_lossy().into_owned(),
        files => fill(tr().phone_files, files.len()),
    }
}

fn centered(content: Div) -> Div {
    div().flex().flex_col().child(div().flex_1()).child(content).child(div().flex_1())
}

struct ShareParts {
    code: AnyElement,
    files: AnyElement,
    url: AnyElement,
    copy: Stateful<Div>,
}

impl Shelf {
    pub(super) fn share_page(&self, t: Theme, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let viewport = window.viewport_size();
        let width = units(viewport.width, window) - space::PAGE * 2.;
        let side = (units(viewport.height, window) - TITLE_HEIGHT - CODE_ROOM).clamp(CODE_MIN, CODE_MAX).min(width);
        let Some(parts) = self.share_parts(t, width, side, window, cx) else {
            return self.shelf_page(t, window, cx);
        };
        let block = self.share_block(t, width, parts, cx);
        self.scroll_area(t, Page::Share, "share-page", centered(block).id("share"))
    }

    fn share_parts(
        &self,
        t: Theme,
        width: f32,
        side: f32,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<ShareParts> {
        let animated = self.animated();
        match (self.share(), self.pending()) {
            (Some(share), _) => Some(ShareParts {
                code: code(t, share.code.clone(), side).into_any_element(),
                files: shared_files(&share.server.files).into_any_element(),
                url: div()
                    .min_w_0()
                    .truncate()
                    .text_size(rem(URL_TEXT))
                    .text_color(t.muted)
                    .child(share.server.url.clone())
                    .into_any_element(),
                copy: copy_button(
                    t,
                    SHARE_COPY,
                    self.copied(SHARE_COPY, cx),
                    Tip::keys(tr().copy, &self.keys(Command::Copy)),
                    t.hover,
                )
                .on_click(cx.safe_listener(|shelf, _, _, cx| shelf.copy_share_link(cx))),
            }),
            (None, Some(Pending::Share(files))) => Some(ShareParts {
                code: code_skeleton(t, "share-code-skeleton", side, animated),
                files: match files {
                    Some(files) => shared_files(files).into_any_element(),
                    None => line_skeleton(t, "share-files-skeleton", NAME_SKELETON, animated).into_any_element(),
                },
                url: line_skeleton(
                    t,
                    "share-url-skeleton",
                    measure(window, URL_SAMPLE, URL_TEXT).min(width - TEXT_LINE),
                    animated,
                )
                .into_any_element(),
                copy: disabled_button(t, SHARE_COPY, "copy"),
            }),
            _ => None,
        }
    }

    fn share_block(&self, t: Theme, width: f32, parts: ShareParts, cx: &mut Context<Self>) -> Div {
        let ShareParts { code, files, url, copy } = parts;
        div()
            .w_full()
            .flex_shrink_0()
            .px(rem(space::PAGE))
            .py(rem(GROUP_GAP))
            .flex()
            .flex_col()
            .items_center()
            .gap(rem(GROUP_GAP))
            .child(div().marked("share-code").flex_shrink_0().child(code))
            .child(
                div()
                    .flex_shrink_0()
                    .max_w(rem(width))
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(rem(ITEM_GAP))
                    .child(
                        div()
                            .marked("share-files")
                            .max_w(rem(width))
                            .truncate()
                            .line_height(rem(TEXT_LINE))
                            .font_weight(FontWeight::MEDIUM)
                            .child(files),
                    )
                    .child(
                        div()
                            .marked("share-url")
                            .max_w(rem(width))
                            .flex()
                            .items_center()
                            .gap(rem(ITEM_GAP))
                            .child(url)
                            .child(copy.marked(SHARE_COPY).flex_shrink_0()),
                    ),
            )
            .child(
                div()
                    .marked("share-hint")
                    .flex_shrink_0()
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap(rem(ITEM_GAP))
                    .text_center()
                    .text_size(rem(text::SMALL))
                    .line_height(rem(TEXT_LINE))
                    .text_color(t.muted)
                    .child(tr().phone_hint)
                    .child(tr().phone_firewall),
            )
            .child(
                text_button(t, "share-stop", tr().phone_stop, Look::Plain)
                    .marked("share-stop")
                    .on_click(cx.safe_listener(|shelf, _, _, cx| shelf.stop_share(cx))),
            )
    }

    pub(super) fn qr_page(&self, t: Theme, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let viewport = window.viewport_size();
        let width = units(viewport.width, window) - space::PAGE * 2.;
        let side = (units(viewport.height, window) - TITLE_HEIGHT - QR_ROOM).clamp(CODE_MIN, QR_MAX).min(width);
        let animated = self.animated();
        let (code, text, ready) = match (self.qr(), self.pending()) {
            (Some(qr), _) => (
                code(t, qr.code.clone(), side).into_any_element(),
                qr.text.split_whitespace().collect::<Vec<_>>().join(" ").into_any_element(),
                true,
            ),
            (None, Some(Pending::Qr)) => (
                code_skeleton(t, "qr-code-skeleton", side, animated),
                line_skeleton(t, "qr-text-skeleton", NAME_SKELETON.min(width), animated).into_any_element(),
                false,
            ),
            _ => return self.shelf_page(t, window, cx),
        };
        let copied = self.copied(QR_COPY, cx);
        let block = div()
            .w_full()
            .flex_shrink_0()
            .px(rem(space::PAGE))
            .py(rem(GROUP_GAP))
            .flex()
            .flex_col()
            .items_center()
            .gap(rem(GROUP_GAP))
            .child(div().marked("qr-code").flex_shrink_0().child(code))
            .child(
                div()
                    .marked("qr-text")
                    .max_w(rem(width))
                    .truncate()
                    .text_size(rem(text::SMALL))
                    .line_height(rem(TEXT_LINE))
                    .text_color(t.muted)
                    .child(text),
            )
            .child(
                div()
                    .marked("qr-buttons")
                    .max_w(rem(width))
                    .flex()
                    .flex_wrap()
                    .justify_center()
                    .gap(rem(space::S))
                    .child(
                        text_button(
                            t,
                            QR_COPY,
                            if copied { tr().copied } else { tr().qr_copy },
                            Look::plain_when(ready),
                        )
                        .marked(QR_COPY)
                        .when(ready, |d| d.on_click(cx.safe_listener(|shelf, _, _, cx| shelf.copy_qr(cx)))),
                    )
                    .child(
                        text_button(t, "qr-keep", tr().context_menu_verb, Look::plain_when(ready))
                            .marked("qr-keep")
                            .when(ready, |d| d.on_click(cx.safe_listener(|shelf, _, _, cx| shelf.keep_qr(cx)))),
                    )
                    .child(
                        text_button(t, "qr-done", tr().done, Look::Plain)
                            .marked("qr-done")
                            .on_click(cx.safe_listener(|shelf, _, _, cx| shelf.toggle_page(Page::Shelf, cx))),
                    ),
            );
        self.scroll_area(t, Page::Qr, "qr-page", centered(block).id("qr"))
    }
}
