use gpui::*;
use std::time::{Duration, Instant};

const TITLE_TIME: Duration = Duration::from_secs(10);
const BOTTOM_TIME: Duration = Duration::from_secs(5);
const COPIED_TIME: Duration = Duration::from_millis(1500);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Notice {
    Alert,
    Toast,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Place {
    Title,
    Bottom,
}

impl Place {
    fn time(self) -> Duration {
        match self {
            Place::Title => TITLE_TIME,
            Place::Bottom => BOTTOM_TIME,
        }
    }
}

struct Shown {
    text: SharedString,
    left: Duration,
    since: Instant,
    task: Option<Task<()>>,
}

#[derive(Default)]
pub struct Notices {
    alert: Option<Shown>,
    toast: Option<Shown>,
    held: Option<Notice>,
    copied: Option<SharedString>,
    copied_task: Option<Task<()>>,
}

fn expire(cx: &mut Context<Notices>, notice: Notice, after: Duration) -> Task<()> {
    cx.spawn(async move |this, cx| {
        cx.background_executor().timer(after).await;
        let _ = this.update(cx, |notices, cx| notices.dismiss(notice, cx));
    })
}

impl Notices {
    fn slot(&mut self, notice: Notice) -> &mut Option<Shown> {
        match notice {
            Notice::Alert => &mut self.alert,
            Notice::Toast => &mut self.toast,
        }
    }

    pub fn alert(&self) -> Option<&SharedString> {
        self.alert.as_ref().map(|shown| &shown.text)
    }

    pub fn toast(&self) -> Option<&SharedString> {
        self.toast.as_ref().map(|shown| &shown.text)
    }

    pub fn copied(&self) -> Option<&SharedString> {
        self.copied.as_ref()
    }

    pub fn is_copied(&self, origin: &str) -> bool {
        self.copied.as_ref().is_some_and(|copied| copied == origin)
    }

    pub fn show(&mut self, notice: Notice, text: SharedString, place: Place, cx: &mut Context<Self>) {
        let left = place.time();
        let task = (self.held != Some(notice)).then(|| expire(cx, notice, left));
        *self.slot(notice) = Some(Shown { text, left, since: cx.background_executor().now(), task });
        cx.notify();
    }

    pub fn hold(&mut self, notice: Notice, held: bool, cx: &mut Context<Self>) {
        if held == (self.held == Some(notice)) {
            return;
        }
        self.held = held.then_some(notice);
        let now = cx.background_executor().now();
        let Some(shown) = self.slot(notice).as_mut() else {
            return;
        };
        if held {
            shown.left = shown.left.saturating_sub(now.saturating_duration_since(shown.since));
            shown.task = None;
        } else {
            shown.since = now;
            shown.task = Some(expire(cx, notice, shown.left));
        }
    }

    pub(super) fn dismiss(&mut self, notice: Notice, cx: &mut Context<Self>) {
        *self.slot(notice) = None;
        if self.held == Some(notice) {
            self.held = None;
        }
        cx.notify();
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.alert = None;
        self.toast = None;
        self.held = None;
        self.copied = None;
        self.copied_task = None;
        cx.notify();
    }

    pub(super) fn flash(&mut self, origin: SharedString, cx: &mut Context<Self>) {
        self.copied = Some(origin);
        self.copied_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(COPIED_TIME).await;
            let _ = this.update(cx, |notices, cx| {
                notices.copied = None;
                cx.notify();
            });
        }));
        cx.notify();
    }
}
