use crate::actions::{Slot, TileId, Tool};
use crate::system::Progress;
use gpui::*;
use std::time::Duration;

const POLL: Duration = Duration::from_millis(100);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct JobId(usize);

pub enum Ended<T> {
    Finished(anyhow::Result<T>),
    Stopped,
}

pub enum JobKind {
    Action { tile: TileId, timeout: Option<Duration> },
    Install { tool: Tool, place: Slot },
    Page(Slot),
    Other,
}

pub struct Job {
    id: JobId,
    pub label: String,
    pub subject: Option<String>,
    pub progress: Progress,
    kind: JobKind,
    _deadline: Option<Task<()>>,
}

impl Job {
    pub fn place(&self) -> Option<&Slot> {
        match &self.kind {
            JobKind::Install { place, .. } | JobKind::Page(place) => Some(place),
            JobKind::Action { .. } | JobKind::Other => None,
        }
    }

    fn tile(&self) -> Option<TileId> {
        match self.kind {
            JobKind::Action { tile, .. } => Some(tile),
            JobKind::Install { .. } | JobKind::Page(_) | JobKind::Other => None,
        }
    }

    fn tool(&self) -> Option<Tool> {
        match self.kind {
            JobKind::Install { tool, .. } => Some(tool),
            JobKind::Action { .. } | JobKind::Page(_) | JobKind::Other => None,
        }
    }

    pub fn fraction(&self) -> Option<f32> {
        unfinished(self.progress.get().fraction)
    }

    pub fn percent(&self) -> Option<String> {
        self.fraction().map(|fraction| format!("{:.0}%", fraction * 100.))
    }
}

fn unfinished(fraction: Option<f32>) -> Option<f32> {
    fraction.filter(|fraction| (fraction * 100.).round() < 100.)
}

#[derive(Default)]
pub struct Jobs {
    list: Vec<Job>,
    next: usize,
    poll: Option<Task<()>>,
    failed: Vec<TileId>,
    drawn: Vec<Option<u8>>,
}

impl Jobs {
    pub fn start(
        &mut self,
        label: String,
        subject: Option<String>,
        kind: JobKind,
        cx: &mut Context<Self>,
    ) -> (JobId, Progress) {
        let id = JobId(self.next);
        self.next += 1;
        let progress = Progress::default();
        let mut deadline = None;
        if let JobKind::Action { tile, timeout } = kind {
            self.failed.retain(|failed| *failed != tile);
            deadline = timeout.map(|timeout| self.expire_after(id, timeout, cx));
        }
        self.list.push(Job { id, label, subject, progress: progress.clone(), kind, _deadline: deadline });
        self.changed(cx);
        if self.poll.is_none() {
            self.poll = Some(cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor().timer(POLL).await;
                    let running = this.update(cx, |jobs, cx| {
                        if jobs.percents() != jobs.drawn {
                            jobs.changed(cx);
                        }
                        !jobs.list.is_empty()
                    });
                    if !matches!(running, Ok(true)) {
                        break;
                    }
                }
                let _ = this.update(cx, |jobs, _| jobs.poll = None);
            }));
        }
        (id, progress)
    }

    fn expire_after(&self, id: JobId, timeout: Duration, cx: &mut Context<Self>) -> Task<()> {
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(timeout).await;
            let _ = this.update(cx, |jobs, cx| {
                if let Some(job) = jobs.list.iter().find(|job| job.id == id) {
                    log::warn!("\"{}\" ran longer than {} s, stopping it", job.label, timeout.as_secs());
                    job.progress.expire();
                    cx.notify();
                }
            });
        })
    }

    fn changed(&mut self, cx: &mut Context<Self>) {
        self.drawn = self.percents();
        cx.notify();
    }

    fn percents(&self) -> Vec<Option<u8>> {
        self.list.iter().map(|job| job.fraction().map(|fraction| (fraction * 100.).round() as u8)).collect()
    }

    pub fn for_tile(&self, tile: TileId) -> Option<&Job> {
        self.list.iter().find(|job| job.tile() == Some(tile))
    }

    pub fn record(&mut self, tile: TileId, failed: bool, cx: &mut Context<Self>) {
        self.failed.retain(|known| *known != tile);
        if failed {
            self.failed.push(tile);
        }
        cx.notify();
    }

    pub fn failed(&self, tile: TileId) -> bool {
        self.failed.contains(&tile)
    }

    pub fn end(&mut self, id: JobId, cx: &mut Context<Self>) {
        self.list.retain(|job| job.id != id);
        self.changed(cx);
    }

    pub(super) fn stop_all(&mut self, cx: &mut Context<Self>) {
        for job in self.list.drain(..) {
            log::info!("Stopping \"{}\"", job.label);
            job.progress.cancel();
        }
        self.changed(cx);
    }

    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &Job> {
        self.list.iter()
    }

    pub fn last(&self) -> Option<&Job> {
        self.list.last()
    }

    pub fn fraction(&self) -> Option<f32> {
        let fractions = self.list.iter().map(|job| job.progress.get().fraction).collect::<Option<Vec<_>>>()?;
        unfinished(Some(fractions.iter().sum::<f32>() / fractions.len().max(1) as f32))
    }

    pub(super) fn for_tool(&self, tool: Tool) -> Option<&Job> {
        self.list.iter().find(|job| job.tool() == Some(tool))
    }

    pub(super) fn cancel_installs(&self, place: &Slot) {
        self.list
            .iter()
            .filter(|job| job.tool().is_some() && job.place() == Some(place))
            .for_each(|job| job.progress.cancel());
    }
}
