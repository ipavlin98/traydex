use super::actions::{group, labelled, options, segment};
use super::faded;
use super::widgets::{
    Look, Marked, choice, dimmed, grid, heading, heading_width, hint, page, skeleton_line, switch, text_button,
};
use crate::actions::{Extra, Fetch, HEIGHTS, Media, Mode, Pending, Quality};
use crate::design::{line, rem, size, space, text, units};
use crate::i18n::tr;
use crate::shelf::{Page, SafeListener, Shelf};
use crate::theme::Theme;
use gpui::{prelude::*, *};

const TITLE_SKELETON: f32 = 180.;
const INFO_SKELETON: f32 = 110.;
const QUALITY_COLUMNS: usize = 4;
const EXTRA_HEIGHT: f32 = 50.;
const HINT_SKELETON: f32 = 90.;

fn switch_id(extra: Extra) -> &'static str {
    match extra {
        Extra::Playlist => "media-playlist",
        Extra::Subtitles => "media-subtitles",
        Extra::Cover => "media-cover",
        Extra::Sponsors => "media-sponsors",
    }
}

fn known(text: Option<Div>, skeleton: f32, id: (&'static str, usize), t: Theme, animated: bool) -> AnyElement {
    match text {
        Some(text) => text.into_any_element(),
        None => div().w(rem(skeleton)).max_w_full().child(skeleton_line(t, id, animated)).into_any_element(),
    }
}

impl Shelf {
    pub(super) fn media_page(&self, t: Theme, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let (media, fetch) = match (self.fetching(), self.pending()) {
            (Some(fetching), _) => (Some(&fetching.media), fetching.fetch),
            (None, Some(Pending::Media(fetch))) => (None, *fetch),
            _ => return self.shelf_page(t, window, cx),
        };
        let content = page("media")
            .child(self.media_header(t, media, window))
            .child(self.media_modes(t, media.is_some(), fetch, cx))
            .child(self.media_qualities(t, media, fetch, cx))
            .child(self.media_extras(t, media, fetch, cx))
            .child(self.media_buttons(t, media.is_some(), cx));
        self.scroll_area(t, Page::Media, "media-page", content)
    }

    fn media_header(&self, t: Theme, media: Option<&Media>, window: &Window) -> Div {
        let animated = self.animated();
        let room = units(window.viewport_size().width, window) - (space::PAGE * 2. + size::PAGE_BADGE + space::XL);
        let title = media.map(|media| {
            let clipped = heading_width(window, &media.title) > room;
            div().child(
                div()
                    .id("media-title-text")
                    .child(faded(t, room, media.title.clone(), t.bg, false, 0.))
                    .when(clipped, |d| d.tooltip(hint(t, media.title.clone()))),
            )
        });
        let info = media.map(|media| div().w(rem(room)).truncate().child(media.detail()));
        heading(
            t,
            "media",
            "video",
            known(title, TITLE_SKELETON, ("media-title", 0), t, animated),
            known(info, INFO_SKELETON, ("media-info", 0), t, animated),
        )
    }

    fn media_modes(&self, t: Theme, ready: bool, fetch: Fetch, cx: &mut Context<Self>) -> Div {
        let modes = [(Mode::Video, "media-video", tr().media_video), (Mode::Audio, "media-audio", tr().media_audio)];
        let segments = modes
            .into_iter()
            .map(|(mode, id, text)| {
                segment(t, id, text, fetch.mode == mode).when(ready, |d| {
                    d.on_click(cx.safe_listener(move |shelf, _, _, cx| shelf.set_media_mode(mode, cx)))
                })
            })
            .collect();
        options(t, tr().media_mode, segments).marked("media-mode").flex_shrink_0()
    }

    fn media_qualities(&self, t: Theme, media: Option<&Media>, fetch: Fetch, cx: &mut Context<Self>) -> Div {
        let current = media.map_or(fetch.quality, |media| media.quality(&fetch));
        let video = fetch.mode == Mode::Video;
        let choices = [Quality::Best].into_iter().chain(HEIGHTS.map(Quality::Height));
        let cells: Vec<_> = choices
            .enumerate()
            .map(|(index, quality)| {
                let (text, allowed) = match quality {
                    Quality::Best => (SharedString::from(tr().quality_best), true),
                    Quality::Height(height) => (format!("{height}p").into(), media.is_none_or(|m| m.allows(height))),
                };
                let enabled = media.is_some() && video && allowed;
                let active = video && allowed && quality == current;
                choice(t, ("media-quality", index), text, active, enabled)
                    .marked(format!("media-quality:{index}"))
                    .when(enabled && !active, |d| {
                        d.on_click(cx.safe_listener(move |shelf, _, _, cx| shelf.set_quality(quality, cx)))
                    })
            })
            .collect();
        labelled(t, tr().media_quality)
            .marked("media-quality")
            .flex_shrink_0()
            .child(div().flex().flex_col().gap(rem(space::S)).children(grid(cells, QUALITY_COLUMNS, space::S)))
    }

    fn media_extras(&self, t: Theme, media: Option<&Media>, fetch: Fetch, cx: &mut Context<Self>) -> Div {
        let animated = self.animated();
        let rows = Extra::ALL.into_iter().enumerate().map(|(index, extra)| {
            let enabled = media.is_some_and(|media| media.offers(extra));
            let on = media.map_or(fetch.get(extra) && extra != Extra::Playlist, |media| media.applies(&fetch, extra));
            let note = media.map(|media| div().truncate().child(media.hint(extra)));
            div()
                .marked(format!("media-extra:{index}"))
                .h(rem(EXTRA_HEIGHT))
                .px(rem(space::GROUP))
                .flex()
                .items_center()
                .gap(rem(space::GROUP))
                .when(index > 0, |d| d.border_t_1().border_color(t.border))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(rem(space::SUBTITLE))
                        .child(div().h(rem(line::BODY)).truncate().child(extra.title()))
                        .child(
                            div()
                                .h(rem(line::SMALL))
                                .flex()
                                .items_center()
                                .text_size(rem(text::SMALL))
                                .text_color(t.muted)
                                .child(known(note, HINT_SKELETON, ("media-hint", index), t, animated)),
                        ),
                )
                .child(switch(t, switch_id(extra), on).when(!enabled, dimmed).when(enabled, |d| {
                    d.on_click(cx.safe_listener(move |shelf, _, _, cx| shelf.toggle_extra(extra, cx)))
                }))
        });
        group(t).marked("media-extras").children(rows)
    }

    fn media_buttons(&self, t: Theme, ready: bool, cx: &mut Context<Self>) -> Div {
        div()
            .marked("media-buttons")
            .flex_shrink_0()
            .flex()
            .justify_between()
            .gap(rem(space::M))
            .child(
                text_button(t, "media-cancel", tr().cancel, Look::Plain)
                    .marked("media-cancel")
                    .on_click(cx.safe_listener(|shelf, _, _, cx| shelf.toggle_page(Page::Shelf, cx))),
            )
            .child(
                text_button(t, "media-download", tr().download, Look::primary_when(ready))
                    .marked("media-download")
                    .when(ready, |d| d.on_click(cx.safe_listener(|shelf, _, _, cx| shelf.download_media(cx)))),
            )
    }
}
