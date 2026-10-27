use super::faded;
use super::widgets::{
    Look, Marked, Tip, copy_button, disabled_button, hint, icon, measure, page, skeleton_line, text_button,
};
use crate::actions::{Pending, Verdict};
use crate::design::{glyph, line, radius, rem, size, space, text, units};
use crate::i18n::tr;
use crate::shelf::{Page, SafeListener, Shelf};
use crate::theme::Theme;
use gpui::{prelude::*, *};
use std::path::Path;

const CARD_PADDING: f32 = space::GROUP;
const CARD_BORDER: f32 = size::HAIRLINE;
const LABEL_WIDTH: f32 = 64.;
const LINE: f32 = line::BODY;
const LINE_INSET: f32 = (size::BUTTON - LINE) / 2.;
const SECTION_GAP: f32 = space::L;
const ROW_GAP: f32 = space::M;
const VERDICT_TEXT: f32 = text::SMALL;
const REPORT_GAP: f32 = space::XS;
const REPORT_ICON: f32 = glyph::XS;
const TITLE_ICON: f32 = glyph::M;
const VERDICT_ICON: f32 = TITLE_ICON;
const HASH_LINE: usize = 32;
const HASH_TEXT: f32 = text::SMALL;
const SKELETON_WIDTH: f32 = 212.;
const NAME_SKELETON: f32 = 160.;
const MONOSPACE: &str = "Consolas";

fn hash_row(t: Theme, label: &'static str, value: impl IntoElement, button: Stateful<Div>) -> Div {
    div()
        .flex()
        .items_start()
        .gap(rem(ROW_GAP))
        .child(
            div()
                .w(rem(LABEL_WIDTH))
                .flex_shrink_0()
                .pt(rem(LINE_INSET))
                .line_height(rem(LINE))
                .text_size(rem(text::SMALL))
                .text_color(t.muted)
                .child(label),
        )
        .child(div().flex_1().min_w_0().pt(rem(LINE_INSET)).line_height(rem(LINE)).flex().flex_col().child(value))
        .child(button)
}

fn computed_row(
    t: Theme,
    label: &'static str,
    value: &str,
    id: SharedString,
    copied: bool,
    cx: &mut Context<Shelf>,
) -> Div {
    let lines: Vec<String> =
        value.as_bytes().chunks(HASH_LINE).map(|chunk| String::from_utf8_lossy(chunk).into_owned()).collect();
    let text = value.to_string();
    let origin = id.clone();
    let value = div()
        .flex()
        .flex_col()
        .font_family(MONOSPACE)
        .text_size(rem(HASH_TEXT))
        .children(lines.into_iter().map(|line| div().truncate().child(line)));
    hash_row(
        t,
        label,
        value,
        copy_button(t, id.clone(), copied, Tip::from(tr().copy), t.hover)
            .marked(id)
            .on_click(cx.safe_listener(move |shelf, _, _, cx| shelf.copy_text(text.clone(), origin.clone(), cx))),
    )
}

fn pending_row(t: Theme, label: &'static str, lines: usize, id: SharedString, animated: bool) -> Div {
    let bars = div().flex().flex_col().children(
        (0..lines).map(|line| div().max_w(rem(SKELETON_WIDTH)).child(skeleton_line(t, (id.clone(), line), animated))),
    );
    hash_row(t, label, bars, disabled_button(t, id.clone(), "copy").marked(id))
}

fn card(t: Theme, index: usize, name: AnyElement) -> Div {
    div()
        .marked(format!("hash-card:{index}"))
        .flex_shrink_0()
        .p(rem(CARD_PADDING))
        .flex()
        .flex_col()
        .gap(rem(SECTION_GAP))
        .rounded(rem(radius::LARGE))
        .border_1()
        .border_color(t.border)
        .bg(t.surface)
        .child(
            div()
                .h(rem(LINE))
                .flex()
                .items_center()
                .gap(rem(ROW_GAP))
                .child(icon("file", t.muted, TITLE_ICON))
                .child(div().flex_1().min_w_0().truncate().font_weight(FontWeight::MEDIUM).child(name)),
        )
}

fn file_name(path: &Path) -> AnyElement {
    path.file_name().unwrap_or(path.as_os_str()).to_string_lossy().into_owned().into_any_element()
}

fn verdict_glyph(name: &'static str, color: Rgba) -> Div {
    div().w(rem(TITLE_ICON)).h(rem(LINE)).flex_shrink_0().flex().items_center().justify_center().child(icon(
        name,
        color,
        VERDICT_ICON,
    ))
}

impl Shelf {
    fn verdict_row(
        &self,
        t: Theme,
        index: usize,
        verdict: Option<&Verdict>,
        report: String,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let row = div()
            .marked(format!("hash-verdict:{index}"))
            .flex()
            .items_start()
            .gap(rem(ROW_GAP))
            .pt(rem(SECTION_GAP))
            .border_t_1()
            .border_color(t.border)
            .text_size(rem(VERDICT_TEXT))
            .line_height(rem(LINE));
        let room = units(window.viewport_size().width, window)
            - (space::PAGE * 2. + CARD_PADDING * 2. + CARD_BORDER * 2. + TITLE_ICON + ROW_GAP);
        let Some(verdict) = verdict else {
            let text = tr().vt_off;
            let clipped = measure(window, text, VERDICT_TEXT) > room;
            return row.child(verdict_glyph("shield", t.faint)).child(
                div()
                    .id(("hash-setup", index))
                    .text_color(t.muted)
                    .cursor_pointer()
                    .hover(|s| s.underline())
                    .child(faded(t, room, text, t.surface, false, 0.))
                    .when(clipped, |d| d.tooltip(hint(t, text)))
                    .on_click(cx.safe_listener(|shelf, _, window, cx| shelf.hash_settings(window, cx))),
            );
        };
        let (glyph, color, text_color) = match verdict {
            Verdict::Clean { .. } => ("shield", t.success, t.text),
            Verdict::Flagged { .. } => ("alert", t.danger, t.danger),
            Verdict::Checking | Verdict::Unknown | Verdict::Failed(_) => ("shield", t.faint, t.muted),
        };
        let reported = verdict.reported();
        let link = measure(window, tr().vt_report, VERDICT_TEXT) + REPORT_GAP + REPORT_ICON + ROW_GAP;
        let room = if reported { room - link.ceil() } else { room };
        let summary = verdict.summary();
        let clipped = measure(window, &summary, VERDICT_TEXT) > room;
        row.child(verdict_glyph(glyph, color))
            .child(
                div()
                    .id(("hash-summary", index))
                    .text_color(text_color)
                    .child(faded(t, room, summary.clone(), t.surface, false, 0.))
                    .when(clipped, |d| d.tooltip(hint(t, summary))),
            )
            .when(reported, |d| {
                d.child(
                    div()
                        .id(("hash-report", index))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .gap(rem(REPORT_GAP))
                        .text_color(t.accent_text)
                        .cursor_pointer()
                        .hover(|s| s.underline())
                        .child(tr().vt_report)
                        .child(icon("open", t.accent_text, REPORT_ICON))
                        .on_click(cx.safe_listener(move |shelf, _, _, _| shelf.open_link(report.clone()))),
                )
            })
    }

    pub(super) fn hashes_page(&self, t: Theme, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let ids = |index: usize| {
            (SharedString::from(format!("copy-sha256:{index}")), SharedString::from(format!("copy-md5:{index}")))
        };
        let animated = self.animated();
        let cards: Vec<Div> = match (self.hashes(), self.pending()) {
            (Some(digests), _) => {
                let copied = self.notices(cx).copied().cloned();
                digests
                    .iter()
                    .enumerate()
                    .map(|(index, digest)| {
                        let (sha, md5) = ids(index);
                        let (sha_copied, md5_copied) = (copied.as_ref() == Some(&sha), copied.as_ref() == Some(&md5));
                        card(t, index, file_name(&digest.path))
                            .child(computed_row(t, "SHA-256", &digest.sha256, sha, sha_copied, cx))
                            .child(computed_row(t, "MD5", &digest.md5, md5, md5_copied, cx))
                            .child(self.verdict_row(t, index, digest.verdict.as_ref(), digest.report(), window, cx))
                    })
                    .collect()
            }
            (None, Some(Pending::Hashes { files, checking })) => {
                let names: Vec<AnyElement> = match files {
                    Some(files) => files.iter().map(|file| file_name(file)).collect(),
                    None => vec![
                        div()
                            .h(rem(LINE))
                            .max_w(rem(NAME_SKELETON))
                            .flex()
                            .items_center()
                            .child(skeleton_line(t, "hash-name", animated))
                            .into_any_element(),
                    ],
                };
                let verdict = checking.then_some(Verdict::Checking);
                names
                    .into_iter()
                    .enumerate()
                    .map(|(index, name)| {
                        let (sha, md5) = ids(index);
                        card(t, index, name)
                            .child(pending_row(t, "SHA-256", 2, sha, animated))
                            .child(pending_row(t, "MD5", 1, md5, animated))
                            .child(self.verdict_row(t, index, verdict.as_ref(), String::new(), window, cx))
                    })
                    .collect()
            }
            _ => return self.shelf_page(t, window, cx),
        };
        let content = page("hashes").children(cards).child(
            div().flex_shrink_0().flex().justify_end().child(
                text_button(t, "hashes-done", tr().done, Look::Plain)
                    .marked("hashes-done")
                    .on_click(cx.safe_listener(|shelf, _, _, cx| shelf.toggle_page(Page::Shelf, cx))),
            ),
        );
        self.scroll_area(t, Page::Hashes, "hashes-page", content)
    }
}
