use super::Shelf;
use crate::i18n::tr;
use crate::store;
use gpui::*;
use std::path::{Path, PathBuf};
use std::time::Duration;

const SCAN_INTERVAL: Duration = Duration::from_secs(2);

pub struct Scan;

#[derive(Default)]
pub struct Folders {
    scanning: bool,
    schedule: Option<Task<()>>,
}

impl EventEmitter<Scan> for Folders {}

impl Folders {
    pub(super) fn schedule(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.schedule = enabled.then(|| {
            cx.spawn(async move |this, cx| {
                loop {
                    if this.update(cx, |_, cx| cx.emit(Scan)).is_err() {
                        return;
                    }
                    cx.background_executor().timer(SCAN_INTERVAL).await;
                }
            })
        });
    }
}

impl Shelf {
    pub fn watch_folder(&mut self, cx: &mut Context<Self>) {
        let services = self.services.clone();
        self.dialog(
            cx,
            move || services.pick(tr().watch_pick, true),
            |shelf, result, cx| match result {
                Ok(Some(folder)) => {
                    if shelf.store.watch(folder) {
                        shelf.schedule_scans(cx);
                        cx.notify();
                    }
                }
                Ok(None) => {}
                Err(error) => shelf.fail(error.to_string(), cx),
            },
        );
    }

    pub fn unwatch_folder(&mut self, folder: &Path, cx: &mut Context<Self>) {
        if self.store.unwatch(folder) {
            self.schedule_scans(cx);
            self.forget_removed();
            self.items_changed(cx);
        }
    }

    pub(super) fn schedule_scans(&mut self, cx: &mut Context<Self>) {
        let enabled = !self.store.watched().is_empty();
        self.folders.update(cx, |folders, cx| folders.schedule(enabled, cx));
    }

    pub(super) fn scan_folders(&mut self, cx: &mut Context<Self>) {
        let folders: Vec<PathBuf> = self.store.watched().iter().map(|watched| watched.path().to_path_buf()).collect();
        if folders.is_empty() || self.folders.read(cx).scanning {
            return;
        }
        self.folders.update(cx, |folders, _| folders.scanning = true);
        self.background(
            cx,
            move || Ok(folders.into_iter().map(|folder| (store::list_folder(&folder), folder)).collect::<Vec<_>>()),
            |shelf, result, cx| {
                shelf.folders.update(cx, |folders, _| folders.scanning = false);
                let listings = match result {
                    Ok(listings) => listings,
                    Err(error) => return log::warn!("Couldn't scan the watched folders: {error:#}"),
                };
                let mut changed = false;
                for (listed, folder) in listings {
                    match listed {
                        Ok(entries) => changed |= shelf.store.sync_folder(&folder, entries),
                        Err(error) => log::debug!("Couldn't read the watched folder {}: {error}", folder.display()),
                    }
                }
                if changed {
                    shelf.forget_removed();
                    shelf.items_changed(cx);
                }
            },
        );
    }

    fn forget_removed(&mut self) {
        let store = &self.store;
        self.selection.retain(|key| store.item(key).is_some());
    }
}
