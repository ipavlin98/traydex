use super::{JobId, Preview, Shelf};
use crate::actions::{Batch, Digest, Fetch, Field, Media, Pending, Rule, TileId};
use crate::input::TextInput;
use crate::store::ItemId;
use crate::system::Server;
use crate::update::Release;
use gpui::*;
use qrcodegen::QrCode;
use std::collections::HashSet;
use std::path::PathBuf;

#[derive(Clone, PartialEq, Debug)]
pub enum Confirm {
    Clear,
    Reset,
    Uninstall(String),
    Update(Release),
    Remove(HashSet<ItemId>),
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Page {
    Shelf,
    Actions,
    Edit,
    Settings,
    Preview,
    Share,
    Hashes,
    Qr,
    Rename,
    Media,
    Password,
}

impl Page {
    pub fn name(self) -> &'static str {
        match self {
            Page::Shelf => "shelf",
            Page::Actions => "actions",
            Page::Edit => "edit",
            Page::Settings => "settings",
            Page::Preview => "preview",
            Page::Share => "share",
            Page::Hashes => "hashes",
            Page::Qr => "qr",
            Page::Rename => "rename",
            Page::Media => "media",
            Page::Password => "password",
        }
    }
}

pub enum Screen {
    Shelf,
    Actions,
    Settings,
    Edit(Editor),
    Preview(Preview),
    Share(Share),
    Hashes(Vec<Digest>),
    Qr(Qr),
    Rename(Renaming),
    Media(Fetching),
    Password(Unlocking),
    Waiting(Waiting),
}

pub struct Waiting {
    pub job: JobId,
    pub pending: Pending,
}

impl Screen {
    pub fn page(&self) -> Page {
        match self {
            Screen::Shelf => Page::Shelf,
            Screen::Actions => Page::Actions,
            Screen::Settings => Page::Settings,
            Screen::Edit(_) => Page::Edit,
            Screen::Preview(_) => Page::Preview,
            Screen::Share(_) => Page::Share,
            Screen::Hashes(_) => Page::Hashes,
            Screen::Qr(_) => Page::Qr,
            Screen::Rename(_) => Page::Rename,
            Screen::Media(_) => Page::Media,
            Screen::Password(_) => Page::Password,
            Screen::Waiting(waiting) => match waiting.pending {
                Pending::Share(_) => Page::Share,
                Pending::Hashes { .. } => Page::Hashes,
                Pending::Qr => Page::Qr,
                Pending::Rename(_) => Page::Rename,
                Pending::Media(_) => Page::Media,
            },
        }
    }
}

pub struct Share {
    pub server: Server,
    pub code: QrCode,
}

pub struct Qr {
    pub text: String,
    pub code: QrCode,
    pub(super) image: Option<PathBuf>,
}

pub struct Renaming {
    pub batch: Batch,
    pub rule: Rule,
    pub name: Entity<TextInput>,
    pub plan: Vec<(PathBuf, PathBuf)>,
    pub ready: bool,
    pub(super) _subscriptions: Vec<Subscription>,
}

pub struct Fetching {
    pub media: Media,
    pub fetch: Fetch,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Attempt {
    Ready,
    Trying(JobId),
    Wrong,
}

pub struct Unlocking {
    pub archives: Vec<PathBuf>,
    pub password: Entity<TextInput>,
    pub attempt: Attempt,
    pub(super) _subscriptions: Vec<Subscription>,
}

pub struct Editor {
    pub tile: TileId,
    pub inputs: Vec<(Field, Entity<TextInput>)>,
    pub settings: Vec<(String, Entity<TextInput>)>,
    pub languages: Vec<(String, String)>,
    pub(super) _subscriptions: Vec<Subscription>,
}

impl Shelf {
    pub fn page(&self) -> Page {
        self.screen.page()
    }

    pub(crate) fn scroll(&self, page: Page) -> &ScrollHandle {
        let [shelf, actions, edit, settings, preview, share, hashes, qr, rename, media, password] = &self.scrolls;
        match page {
            Page::Shelf => shelf,
            Page::Actions => actions,
            Page::Edit => edit,
            Page::Settings => settings,
            Page::Preview => preview,
            Page::Share => share,
            Page::Hashes => hashes,
            Page::Qr => qr,
            Page::Rename => rename,
            Page::Media => media,
            Page::Password => password,
        }
    }

    pub fn preview(&self) -> Option<&Preview> {
        match &self.screen {
            Screen::Preview(preview) => Some(preview),
            _ => None,
        }
    }

    pub(super) fn preview_mut(&mut self) -> Option<&mut Preview> {
        match &mut self.screen {
            Screen::Preview(preview) => Some(preview),
            _ => None,
        }
    }

    pub fn editor(&self) -> Option<&Editor> {
        match &self.screen {
            Screen::Edit(editor) => Some(editor),
            _ => None,
        }
    }

    pub fn share(&self) -> Option<&Share> {
        match &self.screen {
            Screen::Share(share) => Some(share),
            _ => None,
        }
    }

    pub fn hashes(&self) -> Option<&[Digest]> {
        match &self.screen {
            Screen::Hashes(digests) => Some(digests),
            _ => None,
        }
    }

    pub fn pending(&self) -> Option<&Pending> {
        match &self.screen {
            Screen::Waiting(waiting) => Some(&waiting.pending),
            _ => None,
        }
    }

    pub(super) fn waiting_for(&self, job: JobId) -> bool {
        matches!(&self.screen, Screen::Waiting(waiting) if waiting.job == job)
    }

    pub fn qr(&self) -> Option<&Qr> {
        match &self.screen {
            Screen::Qr(qr) => Some(qr),
            _ => None,
        }
    }

    pub fn fetching(&self) -> Option<&Fetching> {
        match &self.screen {
            Screen::Media(fetching) => Some(fetching),
            _ => None,
        }
    }

    pub fn renaming(&self) -> Option<&Renaming> {
        match &self.screen {
            Screen::Rename(renaming) => Some(renaming),
            _ => None,
        }
    }

    pub fn unlocking(&self) -> Option<&Unlocking> {
        match &self.screen {
            Screen::Password(unlocking) => Some(unlocking),
            _ => None,
        }
    }

    pub(super) fn navigate(&mut self, screen: Screen, cx: &mut Context<Self>) {
        let previous = std::mem::replace(&mut self.screen, screen);
        let (from, to) = (previous.page(), self.screen.page());
        if from != to {
            self.notices.update(cx, |notices, cx| notices.clear(cx));
        }
        if from != to && !matches!((from, to), (Page::Actions, Page::Edit) | (Page::Edit, Page::Actions)) {
            self.clear_page_search(cx);
        }
        if let Screen::Preview(preview) = previous {
            self.close_preview(preview, cx);
        }
        self.load_thumbnails(cx);
        cx.notify();
    }

    pub(super) fn show(&mut self, screen: Screen, cx: &mut Context<Self>) {
        self.confirming = None;
        self.navigate(screen, cx);
    }
}
