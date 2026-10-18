use super::{Ended, JobId, JobKind, Jobs, Screen, Shelf};
use crate::guard;
use crate::i18n::tr;
use crate::system::{Desktop, Progress};
use futures::channel::oneshot;
use gpui::*;

pub trait SafeListener {
    fn safe_listener<E: ?Sized>(
        &self,
        f: impl Fn(&mut Shelf, &E, &mut Window, &mut Context<Shelf>) + 'static,
    ) -> impl Fn(&E, &mut Window, &mut App) + 'static;
}

impl SafeListener for Context<'_, Shelf> {
    fn safe_listener<E: ?Sized>(
        &self,
        f: impl Fn(&mut Shelf, &E, &mut Window, &mut Context<Shelf>) + 'static,
    ) -> impl Fn(&E, &mut Window, &mut App) + 'static {
        self.listener(move |shelf, event, window, cx| shelf.guarded(cx, |shelf, cx| f(shelf, event, window, cx)))
    }
}

impl Shelf {
    pub(crate) fn jobs<'a>(&self, cx: &'a App) -> &'a Jobs {
        self.jobs.read(cx)
    }

    pub(crate) fn jobs_entity(&self) -> Entity<Jobs> {
        self.jobs.clone()
    }

    pub(super) fn guarded(&mut self, cx: &mut Context<Self>, job: impl FnOnce(&mut Self, &mut Context<Self>)) {
        if guard::or_else(
            || false,
            || {
                job(self, cx);
                true
            },
        ) {
            return self.commit(cx);
        }
        self.recover(cx);
    }

    fn recover(&mut self, cx: &mut Context<Self>) {
        log::error!("A handler panicked, restoring the last committed state");
        self.store = self.store.reverted();
        if let Err(error) = self.tiles.load(&*self.services) {
            log::error!("Couldn't reload the tiles: {error:#}");
        }
        let left = guard::or_else(
            || false,
            || {
                self.navigate(Screen::Shelf, cx);
                true
            },
        );
        if !left {
            log::error!("Leaving the page panicked too, dropping it");
            self.screen = Screen::Shelf;
        }
        self.selection.reset();
        self.hover.clear();
        self.session.release();
        self.confirming = None;
        self.menu.close();
        self.stop_recording(cx);
        self.update_hits(cx);
        self.refresh(cx);
        self.fail(tr().internal_error, cx);
    }

    pub(super) fn after_update(&self, call: impl FnOnce(&dyn Desktop) + 'static, cx: &mut Context<Self>) {
        let desktop = self.desktop.clone();
        cx.foreground_executor().spawn(async move { call(&*desktop) }).detach();
    }

    pub(super) fn after_update_then<T: 'static>(
        &self,
        call: impl FnOnce(&dyn Desktop) -> anyhow::Result<T> + 'static,
        done: impl FnOnce(&mut Self, anyhow::Result<T>, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) {
        let desktop = self.desktop.clone();
        cx.spawn(async move |this, cx| {
            let result = guard::catch(|| call(&*desktop));
            let _ = this.update(cx, |shelf, cx| shelf.guarded(cx, |shelf, cx| done(shelf, result, cx)));
        })
        .detach();
    }

    pub(super) fn background<T: Send + 'static>(
        &self,
        cx: &mut Context<Self>,
        job: impl FnOnce() -> anyhow::Result<T> + Send + 'static,
        done: impl FnOnce(&mut Self, anyhow::Result<T>, &mut Context<Self>) + 'static,
    ) {
        self.background_task(cx, job, done).detach();
    }

    pub(super) fn background_task<T: Send + 'static>(
        &self,
        cx: &mut Context<Self>,
        job: impl FnOnce() -> anyhow::Result<T> + Send + 'static,
        done: impl FnOnce(&mut Self, anyhow::Result<T>, &mut Context<Self>) + 'static,
    ) -> Task<()> {
        let (tx, result) = oneshot::channel();
        self.services.spawn(Box::new(move || {
            let _ = tx.send(guard::catch(job));
        }));
        cx.spawn(async move |this, cx| {
            let value = result.await.unwrap_or_else(|_| Err(guard::failure()));
            let _ = this.update(cx, |shelf, cx| shelf.guarded(cx, |shelf, cx| done(shelf, value, cx)));
        })
    }

    pub(super) fn run_job<T: Send + 'static>(
        &mut self,
        cx: &mut Context<Self>,
        kind: JobKind,
        label: String,
        subject: Option<String>,
        work: impl FnOnce(&Progress) -> anyhow::Result<T> + Send + 'static,
        done: impl FnOnce(&mut Self, JobId, Ended<T>, &mut Context<Self>) + 'static,
    ) -> JobId {
        let (id, progress) = self.jobs.update(cx, |jobs, cx| jobs.start(label, subject, kind, cx));
        let watched = progress.clone();
        self.background(
            cx,
            move || work(&progress),
            move |shelf, result, cx| {
                shelf.jobs.update(cx, |jobs, cx| jobs.end(id, cx));
                let ended = if watched.stopped() { Ended::Stopped } else { Ended::Finished(result) };
                done(shelf, id, ended, cx);
            },
        );
        id
    }
}
