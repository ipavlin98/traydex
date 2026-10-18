use super::Shelf;
use crate::i18n::{fill, tr};
use crate::store::{Snapshot, Unsaved};
use gpui::*;
use std::time::Duration;

const TYPING_PAUSE: Duration = Duration::from_millis(500);

#[derive(Default)]
pub(super) struct Saving {
    writing: bool,
    queued: Option<Snapshot>,
    later: Option<Task<()>>,
    quitting: bool,
}

fn merged(older: Option<Snapshot>, newer: Snapshot) -> Snapshot {
    match older {
        Some(older) => older.merge(newer),
        None => newer,
    }
}

impl Saving {
    fn submit(&mut self, snapshot: Snapshot) -> Option<Snapshot> {
        let snapshot = merged(self.queued.take(), snapshot);
        if self.writing {
            self.queued = Some(snapshot);
            return None;
        }
        self.writing = true;
        Some(snapshot)
    }

    fn finished(&mut self) -> Option<Snapshot> {
        self.writing = false;
        let queued = self.queued.take()?;
        self.submit(queued)
    }
}

impl Shelf {
    pub(super) fn commit(&mut self, cx: &mut Context<Self>) {
        match self.store.take_unsaved() {
            Some(Unsaved::Now) => self.save(cx),
            Some(Unsaved::Soon) => self.save_soon(cx),
            None => {}
        }
    }

    pub(super) fn save(&mut self, cx: &mut Context<Self>) {
        self.saving.later = None;
        match self.store.snapshot() {
            Ok(snapshot) => {
                if let Some(snapshot) = self.saving.submit(snapshot) {
                    self.write(snapshot, cx);
                }
            }
            Err(error) => self.fail(fill(tr().save_failed, &error), cx),
        }
    }

    fn save_soon(&mut self, cx: &mut Context<Self>) {
        self.saving.later = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(TYPING_PAUSE).await;
            let _ = this.update(cx, |shelf, cx| shelf.guarded(cx, |shelf, cx| shelf.save(cx)));
        }));
    }

    pub(super) fn flush_typing(&mut self, cx: &mut Context<Self>) {
        if self.saving.later.is_some() {
            self.save(cx);
        }
    }

    pub(super) fn quit(&mut self, cx: &mut Context<Self>) {
        log::info!("Quit");
        self.saving.quitting = true;
        self.save(cx);
        self.leave_when_saved(cx);
    }

    fn leave_when_saved(&self, cx: &mut Context<Self>) {
        if self.saving.quitting && !self.saving.writing {
            self.desktop.flush_clipboard();
            self.desktop.stop_watch();
            cx.quit();
        }
    }

    fn write(&self, snapshot: Snapshot, cx: &mut Context<Self>) {
        self.background(
            cx,
            move || {
                Ok(match snapshot.write() {
                    Ok(()) => None,
                    Err(error) => Some((snapshot, error)),
                })
            },
            |shelf, result, cx| {
                let error = match result {
                    Ok(None) => None,
                    Ok(Some((unsaved, error))) => {
                        shelf.store.restore(unsaved);
                        Some(error)
                    }
                    Err(error) => Some(error),
                };
                if let Some(error) = error {
                    shelf.fail(fill(tr().save_failed, &error), cx);
                }
                if let Some(next) = shelf.saving.finished() {
                    shelf.write(next, cx);
                }
                shelf.leave_when_saved(cx);
            },
        );
    }
}
