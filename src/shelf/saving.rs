use super::Shelf;
use crate::i18n::{fill, tr};
use crate::store::{Snapshot, Unsaved};
use gpui::*;
use std::process::Command;
use std::time::Duration;

const TYPING_PAUSE: Duration = Duration::from_millis(500);

#[derive(Default)]
pub(super) struct Saving {
    writing: bool,
    queued: Option<Snapshot>,
    later: Option<Task<()>>,
    quitting: bool,
    then: Option<Command>,
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

    pub(super) fn busy(&self) -> bool {
        self.writing || self.queued.is_some() || self.later.is_some()
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

    pub(super) fn quit_then(&mut self, command: Command, cx: &mut Context<Self>) {
        self.saving.then = Some(command);
        self.quit(cx);
    }

    fn leave_when_saved(&mut self, cx: &mut Context<Self>) {
        if self.saving.quitting && !self.saving.writing {
            if let Some(mut command) = self.saving.then.take()
                && let Err(error) = self.services.launch(&mut command)
            {
                self.saving.quitting = false;
                return self.fail(fill(tr().update_failed, &error), cx);
            }
            self.desktop.flush_clipboard();
            self.desktop.stop_watch();
            cx.quit();
        }
    }

    fn write(&self, snapshot: Snapshot, cx: &mut Context<Self>) {
        let services = self.services.clone();
        self.background(
            cx,
            move || {
                Ok(match snapshot.write() {
                    Ok(()) => {
                        snapshot.recycle(&*services);
                        None
                    }
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
