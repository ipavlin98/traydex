use super::{Confirm, Ended, JobKind, Shelf};
use crate::i18n::{fill, tr};
use crate::update::{self, Release, Version};
use gpui::*;
use std::time::Duration;

const FIRST_CHECK: Duration = Duration::from_secs(60);
const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);

pub struct Due;

#[derive(Default)]
pub struct Updates {
    checking: bool,
    declined: Option<Version>,
    schedule: Option<Task<()>>,
}

impl EventEmitter<Due> for Updates {}

impl Updates {
    pub fn checking(&self) -> bool {
        self.checking
    }

    pub(super) fn schedule(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.schedule = enabled.then(|| {
            cx.spawn(async move |this, cx| {
                let mut wait = FIRST_CHECK;
                loop {
                    cx.background_executor().timer(wait).await;
                    if this.update(cx, |_, cx| cx.emit(Due)).is_err() {
                        return;
                    }
                    wait = CHECK_INTERVAL;
                }
            })
        });
    }

    fn set_checking(&mut self, checking: bool, cx: &mut Context<Self>) {
        self.checking = checking;
        cx.notify();
    }
}

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Check {
    Auto,
    Manual,
}

impl Shelf {
    pub(crate) fn checking_updates(&self, cx: &App) -> bool {
        self.updates.read(cx).checking()
    }

    pub fn check_updates(&mut self, cx: &mut Context<Self>) {
        self.check(Check::Manual, cx);
    }

    pub(super) fn check(&mut self, check: Check, cx: &mut Context<Self>) {
        if self.checking_updates(cx) {
            return;
        }
        self.updates.update(cx, |updates, cx| updates.set_checking(true, cx));
        let services = self.services.clone();
        self.background(
            cx,
            move || update::check(&*services),
            move |shelf, result, cx| {
                shelf.updates.update(cx, |updates, cx| updates.set_checking(false, cx));
                match (result, check) {
                    (Ok(Some(release)), _) => shelf.offer_update(release, check, cx),
                    (Ok(None), Check::Manual) => shelf.show_toast(tr().update_none.into(), cx),
                    (Ok(None), Check::Auto) => log::info!("No updates"),
                    (Err(error), Check::Manual) => shelf.fail(fill(tr().update_check_failed, format!("{error:#}")), cx),
                    (Err(error), Check::Auto) => log::warn!("Couldn't check for updates: {error:#}"),
                }
            },
        );
    }

    fn offer_update(&mut self, release: Release, check: Check, cx: &mut Context<Self>) {
        log::info!("Update available: {}", release.version);
        if check == Check::Auto {
            let declined = self.updates.read(cx).declined.as_ref() == Some(&release.version);
            if declined || self.confirming.is_some() {
                return;
            }
            if !self.shown(cx) {
                self.desktop.notify(&fill(tr().update_notify, &release.version));
            }
        }
        self.confirming = Some(Confirm::Update(release));
        cx.notify();
    }

    pub(super) fn decline_update(&mut self, release: Release, cx: &mut Context<Self>) {
        self.updates.update(cx, |updates, _| updates.declined = Some(release.version));
    }

    pub(super) fn install_update(&mut self, release: Release, cx: &mut Context<Self>) {
        let services = self.services.clone();
        let subject = release.version.to_string();
        self.run_job(
            cx,
            JobKind::Other,
            tr().job_update.into(),
            Some(subject),
            move |progress| update::prepare(&release, &*services, progress),
            |shelf, _, ended, cx| match ended {
                Ended::Finished(Ok(command)) => shelf.quit_then(command, cx),
                Ended::Finished(Err(error)) => shelf.fail(fill(tr().update_failed, format!("{error:#}")), cx),
                Ended::Stopped => {}
            },
        );
    }
}
