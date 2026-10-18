use super::{Ended, Fetching, JobKind, Screen, Shelf};
use crate::actions::{self, Action, Extra, Fetch, Kind, Media, Mode, Quality, Single, Slot, TileId};
use gpui::*;

const SLOT: Slot = Slot::Single(Single::Ytdlp);

impl Shelf {
    fn media_tile(&self) -> Option<TileId> {
        self.tiles.find(&SLOT)
    }

    fn remembered_fetch(&self) -> Fetch {
        let action = self.media_tile().and_then(|tile| self.tiles.get(tile));
        match action.map(|action| &action.kind) {
            Some(Kind::Ytdlp(fetch)) => *fetch,
            _ => Fetch::default(),
        }
    }

    pub(super) fn open_media(&mut self, media: Media, cx: &mut Context<Self>) {
        let fetch = self.remembered_fetch();
        self.show(Screen::Media(Fetching { media, fetch }), cx);
    }

    fn change_fetch(&mut self, cx: &mut Context<Self>, change: impl FnOnce(&Media, &mut Fetch)) {
        if let Screen::Media(fetching) = &mut self.screen {
            change(&fetching.media, &mut fetching.fetch);
            cx.notify();
        }
    }

    pub fn set_media_mode(&mut self, mode: Mode, cx: &mut Context<Self>) {
        self.change_fetch(cx, |_, fetch| fetch.mode = mode);
    }

    pub fn set_quality(&mut self, quality: Quality, cx: &mut Context<Self>) {
        self.change_fetch(cx, |media, fetch| {
            let allowed = match quality {
                Quality::Best => true,
                Quality::Height(height) => media.allows(height),
            };
            if allowed {
                fetch.quality = quality;
            }
        });
    }

    pub fn toggle_extra(&mut self, extra: Extra, cx: &mut Context<Self>) {
        self.change_fetch(cx, |media, fetch| {
            if media.offers(extra) {
                fetch.set(extra, !fetch.get(extra));
            }
        });
    }

    pub fn download_media(&mut self, cx: &mut Context<Self>) {
        let Some(Fetching { media, fetch }) = self.fetching() else {
            return;
        };
        let (media, fetch) = (media.clone(), *fetch);
        let tile = self.media_tile();
        if let Some(tile) = tile {
            self.edit_kind(tile, cx, |kind| {
                if let Kind::Ytdlp(saved) = kind {
                    *saved = Fetch { playlist: false, ..fetch };
                }
            });
        }
        let action = tile.and_then(|tile| self.tiles.get(tile)).cloned();
        let action = action.unwrap_or_else(|| Action::new(Kind::Ytdlp(fetch)));
        let (title, label) = (action.title(), action.job_label());
        let subject = Some(media.title.clone());
        log::info!("Downloading {}", media.url);
        self.navigate(Screen::Shelf, cx);
        let storage = self.store.storage().clone();
        let services = self.services.clone();
        self.run_job(
            cx,
            JobKind::Page(SLOT),
            label,
            subject,
            move |progress| actions::download_media(&media, &fetch, progress, &storage, &*services),
            move |shelf, id, ended, cx| match ended {
                Ended::Finished(result) => shelf.finish(title, id, false, result, cx),
                Ended::Stopped => log::info!("Download stopped"),
            },
        );
    }
}
