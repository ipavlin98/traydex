use super::widgets::{Look, Marked, icon, measure, page, picked, skeleton_line, text_button};
use crate::actions::{Pending, Rule};
use crate::design::{glyph, line, radius, rem, space, text};
use crate::i18n::{fill, tr};
use crate::shelf::{Page, SafeListener, Shelf};
use crate::theme::Theme;
use gpui::{prelude::*, *};
use std::path::Path;

const PREVIEW_ROWS: usize = 5;
const PREVIEW_LINE: f32 = line::BODY;
const RADIO: f32 = glyph::S;
const RADIO_DOT: f32 = 6.;
const PREVIEW_TEXT: f32 = text::SMALL;

fn name(path: &Path) -> String {
    path.file_name().unwrap_or(path.as_os_str()).to_string_lossy().into_owned()
}

impl Shelf {
    pub(super) fn rename_page(&self, t: Theme, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let animated = self.animated();
        let ready = self.renaming().map(|renaming| (renaming.rule, &renaming.plan, renaming.ready));
        let entries: Vec<(&Path, Option<&Path>)> = match (ready, self.pending()) {
            (Some((_, plan, _)), _) => plan.iter().map(|(from, to)| (from.as_path(), Some(to.as_path()))).collect(),
            (None, Some(Pending::Rename(files))) => files.iter().map(|file| (file.as_path(), None)).collect(),
            _ => return self.shelf_page(t, window, cx),
        };
        let active = ready.map_or(Rule::Date, |(rule, _, _)| rule);
        let interactive = ready.is_some();
        let rules = Rule::ALL
            .into_iter()
            .enumerate()
            .map(|(index, rule)| self.rename_rule(t, index, rule, active == rule, interactive, cx));
        let changed = entries.iter().filter(|(from, to)| to.as_ref().is_some_and(|to| to != from)).count();
        let can_apply = ready.is_some_and(|(_, _, ready)| ready) && changed > 0;
        let content = page("rename")
            .child(
                div()
                    .marked("rename-title")
                    .flex_shrink_0()
                    .truncate()
                    .text_size(rem(text::TITLE))
                    .line_height(rem(line::TITLE))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(fill(tr().rename_title, entries.len())),
            )
            .child(div().marked("rename-rules").flex_shrink_0().flex().flex_col().gap(rem(space::S)).children(rules))
            .when_some(self.renaming().filter(|renaming| renaming.rule == Rule::Name), |d, renaming| {
                d.child(div().marked("rename-name").flex_shrink_0().child(renaming.name.clone()))
            })
            .child(preview(t, &entries, window, animated))
            .child(self.rename_buttons(t, can_apply, cx));
        self.scroll_area(t, Page::Rename, "rename-page", content)
    }

    fn rename_rule(
        &self,
        t: Theme,
        index: usize,
        rule: Rule,
        selected: bool,
        interactive: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        div()
            .id(("rename-rule", index))
            .marked(format!("rename-rule:{index}"))
            .flex_shrink_0()
            .px(rem(space::L))
            .py(rem(space::M))
            .flex()
            .items_start()
            .gap(rem(space::L))
            .rounded(rem(radius::CONTROL))
            .border_1()
            .when(selected, |d| picked(d, t))
            .when(!selected, |d| d.border_color(t.border))
            .when(interactive, |d| {
                d.cursor_pointer()
                    .when(!selected, |d| d.hover(move |s| s.bg(t.hover)))
                    .on_click(cx.safe_listener(move |shelf, _, window, cx| shelf.set_rule(rule, window, cx)))
            })
            .child(
                div()
                    .mt(rem(space::X2S))
                    .size(rem(RADIO))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .border_1()
                    .border_color(if selected { t.accent } else { t.border_strong })
                    .when(selected, |d| d.child(div().size(rem(RADIO_DOT)).rounded_full().bg(t.accent))),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(rem(space::SUBTITLE))
                    .child(rule.title())
                    .child(div().text_size(rem(text::SMALL)).text_color(t.muted).child(rule.hint())),
            )
    }

    fn rename_buttons(&self, t: Theme, can_apply: bool, cx: &mut Context<Self>) -> Div {
        div()
            .marked("rename-buttons")
            .flex_shrink_0()
            .flex()
            .justify_between()
            .gap(rem(space::M))
            .child(
                text_button(t, "rename-cancel", tr().cancel, Look::Plain)
                    .marked("rename-cancel")
                    .on_click(cx.safe_listener(|shelf, _, _, cx| shelf.toggle_page(Page::Shelf, cx))),
            )
            .child(
                text_button(t, "rename-apply", tr().add_rename, Look::primary_when(can_apply))
                    .marked("rename-apply")
                    .when(can_apply, |d| d.on_click(cx.safe_listener(|shelf, _, _, cx| shelf.rename_files(cx)))),
            )
    }
}

fn preview(t: Theme, entries: &[(&Path, Option<&Path>)], window: &Window, animated: bool) -> Div {
    let column = entries
        .iter()
        .take(PREVIEW_ROWS)
        .map(|(from, _)| measure(window, &name(from), PREVIEW_TEXT))
        .fold(0., f32::max)
        .ceil()
        + 1.;
    let rows = entries
        .iter()
        .take(PREVIEW_ROWS)
        .enumerate()
        .map(|(index, (from, to))| preview_row(t, index, from, *to, column, animated));
    let more = entries.len().saturating_sub(PREVIEW_ROWS);
    div()
        .marked("rename-preview")
        .flex_shrink_0()
        .flex()
        .flex_col()
        .gap(rem(space::S))
        .children(rows)
        .when(more > 0, |d| {
            d.child(div().text_size(rem(text::SMALL)).text_color(t.faint).child(fill(tr().and_more, more)))
        })
}

fn preview_row(t: Theme, index: usize, from: &Path, to: Option<&Path>, column: f32, animated: bool) -> Div {
    div()
        .marked(format!("rename-preview:{index}"))
        .h(rem(PREVIEW_LINE))
        .flex()
        .items_center()
        .gap(rem(space::S))
        .text_size(rem(PREVIEW_TEXT))
        .child(
            div().w(rem(column)).max_w(relative(0.5)).flex_shrink_0().truncate().text_color(t.muted).child(name(from)),
        )
        .child(icon("chevron-right", t.faint, glyph::XS))
        .child(match to {
            Some(to) => div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_color(if to == from { t.muted } else { t.text })
                .child(name(to))
                .into_any_element(),
            None => {
                div().flex_1().min_w_0().child(skeleton_line(t, ("rename-target", index), animated)).into_any_element()
            }
        })
}
