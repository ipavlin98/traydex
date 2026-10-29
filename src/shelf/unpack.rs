use super::{Attempt, Ended, JobId, JobKind, Screen, Shelf, Unlocking};
use crate::actions::{self, Action, Kind, Outcome, Single, Slot, Unlocked};
use crate::i18n::{fill, tr};
use crate::input::{Changed, Submitted, TextInput};
use anyhow::anyhow;
use gpui::*;
use std::path::PathBuf;

const SLOT: Slot = Slot::Single(Single::Unpack);

impl Shelf {
    pub(super) fn open_password(&mut self, archives: Vec<PathBuf>, cx: &mut Context<Self>) {
        let password = cx.new(|cx| TextInput::new(String::new(), tr().password_placeholder, true, cx));
        let subscriptions = vec![
            cx.subscribe(&password, |shelf, _, _: &Changed, cx| shelf.password_changed(cx)),
            cx.subscribe(&password, |shelf, _, _: &Submitted, cx| shelf.unlock(cx)),
        ];
        let unlocking =
            Unlocking { archives, password: password.clone(), attempt: Attempt::Ready, _subscriptions: subscriptions };
        self.show(Screen::Password(unlocking), cx);
        if !self.shown(cx) {
            self.reveal(cx);
        }
        if let Some(window) = cx.active_window() {
            cx.defer(move |cx| {
                let _ = window.update(cx, |_, window, cx| password.update(cx, |input, cx| input.focus(window, cx)));
            });
        }
    }

    pub(super) fn focus_password(&self, window: &mut Window, cx: &mut App) {
        if let Some(unlocking) = self.unlocking() {
            unlocking.password.update(cx, |input, cx| input.focus(window, cx));
        }
    }

    fn set_attempt(&mut self, attempt: Attempt, cx: &mut Context<Self>) {
        if let Screen::Password(unlocking) = &mut self.screen {
            unlocking.attempt = attempt;
            cx.notify();
        }
    }

    fn password_changed(&mut self, cx: &mut Context<Self>) {
        if self.unlocking().is_some_and(|unlocking| unlocking.attempt == Attempt::Wrong) {
            self.set_attempt(Attempt::Ready, cx);
        }
        cx.notify();
    }

    fn trying(&self, job: JobId) -> bool {
        self.unlocking().is_some_and(|unlocking| unlocking.attempt == Attempt::Trying(job))
    }

    pub fn unlock(&mut self, cx: &mut Context<Self>) {
        let Some(unlocking) = self.unlocking() else {
            return;
        };
        let password = unlocking.password.read(cx).text().to_string();
        if password.is_empty() || matches!(unlocking.attempt, Attempt::Trying(_)) {
            return;
        }
        let archives = unlocking.archives.clone();
        let action = self.tiles.find(&SLOT).and_then(|tile| self.tiles.get(tile)).cloned();
        let action = action.unwrap_or_else(|| Action::new(Kind::Unpack));
        let (title, label) = (action.title(), action.job_label());
        let subject =
            archives.first().and_then(|archive| archive.file_name()).map(|name| name.to_string_lossy().into());
        let storage = self.store.storage().clone();
        let id = self.run_job(
            cx,
            JobKind::Page(SLOT),
            label,
            subject,
            move |progress| actions::unlock(&archives, &password, progress, &storage),
            move |shelf, id, ended, cx| match ended {
                Ended::Finished(result) => shelf.unlocked(id, title, result, cx),
                Ended::Stopped => {
                    if shelf.trying(id) {
                        shelf.set_attempt(Attempt::Ready, cx);
                    }
                    log::info!("Extraction stopped");
                }
            },
        );
        self.set_attempt(Attempt::Trying(id), cx);
    }

    fn unlocked(&mut self, job: JobId, title: String, result: anyhow::Result<Unlocked>, cx: &mut Context<Self>) {
        let here = self.trying(job);
        let Unlocked { created, locked, .. } = match result {
            Ok(unlocked) => unlocked,
            Err(error) => {
                if here {
                    self.set_attempt(Attempt::Ready, cx);
                }
                return self.finish(title, job, false, Err(error), cx);
            }
        };
        let message = if created.is_empty() { String::new() } else { fill(tr().unpacked, created.len()) };
        if self.store.add_paths(created) > 0 {
            self.items_changed(cx);
        }
        let outcome = Outcome { message, created: Vec::new(), clipboard: None, show: None };
        if locked.is_empty() {
            if here {
                self.navigate(Screen::Shelf, cx);
            }
            return self.finish(title, job, false, Ok(outcome), cx);
        }
        if !here {
            return self.finish(title, job, false, Err(anyhow!(tr().wrong_password)), cx);
        }
        if let Screen::Password(unlocking) = &mut self.screen {
            unlocking.archives = locked;
            unlocking.attempt = Attempt::Wrong;
        }
        self.finish(title, job, false, Ok(outcome), cx);
    }
}
