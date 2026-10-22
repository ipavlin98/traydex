use super::{Editor, Ended, Job, JobId, JobKind, Page, SHARE_COPY, Screen, Share, Shelf, Waiting};
use crate::actions::{self, Action, Field, Outcome, Presence, Show, Slot, TileId, Tiles, Tool};
use crate::geometry;
use crate::i18n::{fill, tr};
use crate::input::{Changed, Submitted, TextInput};
use crate::store::Payload;
use crate::system::Server;
use gpui::*;
use qrcodegen::{QrCode, QrCodeEcc};
use std::path::PathBuf;

const STRIP_SLACK: f32 = 0.5;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Edge {
    Start,
    End,
}

#[derive(Clone, Copy)]
pub enum Lane {
    Tiles,
    Tabs,
}

impl Edge {
    pub const BOTH: [Edge; 2] = [Edge::Start, Edge::End];

    pub fn hidden(self, offset: Pixels, max: Pixels) -> bool {
        let offset = offset.min(max).max(px(0.));
        let hidden = match self {
            Edge::Start => offset,
            Edge::End => max - offset,
        };
        hidden > px(1.)
    }

    fn page(self, offset: f32, width: f32, tiles: &[(f32, f32)], cover: f32) -> f32 {
        let clear = width - cover * 2.;
        match self {
            Edge::End => {
                let limit = offset + width - cover + STRIP_SLACK;
                let Some((start, _)) = tiles.iter().find(|(_, end)| *end > limit) else {
                    return f32::MAX;
                };
                if start - cover > offset { start - cover } else { offset + clear }
            }
            Edge::Start => {
                let limit = offset + cover - STRIP_SLACK;
                let Some((_, end)) = tiles.iter().rev().find(|(start, _)| *start < limit) else {
                    return 0.;
                };
                if end - width + cover < offset { end - width + cover } else { offset - clear }
            }
        }
    }
}

impl Shelf {
    pub(super) fn perform(&mut self, tile: TileId, payload: Payload, cx: &mut Context<Self>) {
        let Some(action) = self.tiles.get(tile).cloned() else {
            return self.ingest(payload, cx);
        };
        if action.cancels() {
            return log::debug!("The cancel tile does nothing");
        }
        let title = action.title();
        let label = action.job_label();
        let timeout = action.timeout();
        let pending = actions::pending(&action, &payload);
        let waited = pending.is_some();
        log::info!("Action \"{title}\" started with {}", payload.describe());
        let searches = matches!(action.kind, actions::Kind::Search { .. });
        let subject = payload.subject();
        let storage = self.store.storage().clone();
        let services = self.services.clone();
        let id = self.run_job(
            cx,
            JobKind::Action { tile, timeout },
            label,
            subject,
            move |progress| actions::run(&action, payload, progress, &storage, &*services),
            move |shelf, id, ended, cx| {
                let Ended::Finished(result) = ended else {
                    return log::info!("Action \"{title}\" stopped");
                };
                shelf.finish(title, id, waited, result, cx);
            },
        );
        if searches && !self.store.settings().pinned {
            self.hide("web search", cx);
        }
        if let Some(pending) = pending {
            self.show(Screen::Waiting(Waiting { job: id, pending }), cx);
        }
    }

    pub fn presence(&self, tool: Tool) -> Presence {
        self.tiles.presence(tool)
    }

    pub(super) fn detect_tools(&mut self, cx: &mut Context<Self>) {
        let storage = self.store.storage().clone();
        let services = self.services.clone();
        let scan = self.tiles.begin_scan();
        self.background(
            cx,
            move || Ok((Tiles::detect_tools(&storage, &*services), actions::scan_plugins(&storage))),
            move |shelf, found, cx| {
                if let Ok((tools, plugins)) = found
                    && shelf.tiles.finish_scan(scan, tools, plugins)
                {
                    cx.notify();
                }
            },
        );
    }

    pub fn tool_job<'a>(&self, tool: Tool, cx: &'a App) -> Option<&'a Job> {
        self.jobs.read(cx).for_tool(tool)
    }

    pub fn ready(&self, action: &Action) -> bool {
        self.tiles.ready(action)
    }

    pub fn refuses(&self, tile: TileId) -> bool {
        let Some(offer) = self.session.offer() else {
            return false;
        };
        self.tiles.get(tile).is_some_and(|action| !action.accepts(offer))
    }

    pub fn install(&mut self, tool: Tool, place: Slot, cx: &mut Context<Self>) {
        if self.tool_job(tool, cx).is_some() {
            return;
        }
        let name = tool.name();
        let label = format!("{}…", tool.download_label());
        log::info!("Installing {name}");
        let storage = self.store.storage().clone();
        let services = self.services.clone();
        self.run_job(
            cx,
            JobKind::Install { tool, place },
            label,
            None,
            move |progress| tool.install(&storage, &*services, progress),
            move |shelf, _, ended, cx| {
                shelf.detect_tools(cx);
                match ended {
                    Ended::Finished(Ok(path)) => log::info!("{name} installed: {}", path.display()),
                    Ended::Finished(Err(error)) => shelf.fail(format!("{error:#}"), cx),
                    Ended::Stopped => log::info!("{name} installation cancelled"),
                }
                cx.notify();
            },
        );
    }

    pub fn cancel_install(&mut self, tool: Tool, cx: &mut Context<Self>) {
        if let Some(job) = self.tool_job(tool, cx) {
            job.progress.cancel();
            cx.notify();
        }
    }

    pub fn remove_tool(&mut self, tool: Tool, cx: &mut Context<Self>) {
        if let Err(error) = tool.remove(self.store.storage()) {
            return self.fail(error.to_string(), cx);
        }
        log::info!("{} removed", tool.name());
        self.detect_tools(cx);
        cx.notify();
    }

    pub fn stop_jobs(&mut self, cx: &mut Context<Self>) {
        self.jobs.update(cx, |jobs, cx| jobs.stop_all(cx));
        if matches!(self.screen, Screen::Waiting(_)) {
            self.navigate(Screen::Shelf, cx);
        }
    }

    pub fn open_job(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let tile = self.jobs.read(cx).iter().rev().find_map(|job| self.tiles.find(job.place()?));
        if let Some(tile) = tile {
            self.open_editor(tile, None, window, cx);
        }
    }

    pub(super) fn finish(
        &mut self,
        title: String,
        job: JobId,
        waited: bool,
        result: anyhow::Result<Outcome>,
        cx: &mut Context<Self>,
    ) {
        let waiting = self.waiting_for(job);
        let opens = waiting || !waited;
        let (text, failed) = match result {
            Ok(outcome) => {
                if let Some(text) = outcome.clipboard {
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                }
                match outcome.show {
                    Some(_) if !opens => log::debug!("The page was left while \"{title}\" ran, its result is dropped"),
                    Some(Show::Share(server)) => self.open_share(server, cx),
                    Some(Show::Hashes(digests, lookup)) => self.open_hashes(digests, lookup, cx),
                    Some(Show::Qr(text)) => self.open_qr(text, cx),
                    Some(Show::Rename(batch)) => self.open_rename(batch, cx),
                    Some(Show::Media(media)) => self.open_media(media, cx),
                    Some(Show::Password(archives)) => self.open_password(archives, cx),
                    None => {}
                }
                if self.store.add_paths(outcome.created) > 0 {
                    self.items_changed(cx);
                }
                log::info!("Action \"{title}\" finished: {}", outcome.message);
                (outcome.message, false)
            }
            Err(error) => {
                if waiting {
                    self.navigate(Screen::Shelf, cx);
                }
                (format!("{title}: {error:#}"), true)
            }
        };
        if failed {
            if !self.shown(cx) {
                self.desktop.notify(&text);
            }
            self.fail(text, cx);
        } else if !text.is_empty() {
            if self.shown(cx) {
                self.show_toast(text, cx);
            } else {
                self.desktop.notify(&text);
            }
        }
    }

    fn open_share(&mut self, server: Server, cx: &mut Context<Self>) {
        let code = match QrCode::encode_text(&server.url, QrCodeEcc::Medium) {
            Ok(code) => code,
            Err(error) => return self.fail(error.to_string(), cx),
        };
        self.show(Screen::Share(Share { server, code }), cx);
        if !self.shown(cx) {
            self.reveal(cx);
        }
    }

    pub fn stop_share(&mut self, cx: &mut Context<Self>) {
        self.navigate(Screen::Shelf, cx);
    }

    pub fn copy_share_link(&mut self, cx: &mut Context<Self>) {
        let Some(url) = self.share().map(|share| share.server.url.clone()) else {
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(url));
        self.flash_copied(SHARE_COPY.into(), cx);
    }

    pub(super) fn fit(&self, cx: &mut Context<Self>) {
        let zoom = self.zoom();
        let (width, height) = geometry::least_size(self.store.settings().layout, self.tiles.len());
        let (width, height) = (width * zoom, height * zoom);
        self.after_update(move |desktop| desktop.set_min_size(width, height), cx);
    }

    pub(super) fn save_tiles(&mut self, cx: &mut Context<Self>) {
        if let Err(error) = self.tiles.save() {
            self.fail(fill(tr().actions_save_failed, &error), cx);
        }
        self.fit(cx);
        cx.notify();
    }

    pub(super) fn leave_editor(&mut self, cx: &mut Context<Self>) {
        if self.page() == Page::Edit {
            self.navigate(Screen::Actions, cx);
        }
    }

    pub(crate) fn tiles(&self) -> &Tiles {
        &self.tiles
    }

    pub(crate) fn record_fitting_width(&self, width: f32) {
        self.fitting_width.set(width);
    }

    pub(crate) fn lane(&self, lane: Lane) -> &ScrollHandle {
        match lane {
            Lane::Tiles => &self.tile_strip,
            Lane::Tabs => &self.tab_strip,
        }
    }

    pub fn scroll_lane(&mut self, lane: Lane, edge: Edge, cover: f32, cx: &mut Context<Self>) {
        let strip = self.lane(lane);
        let view = strip.bounds();
        let left = f32::from(view.left());
        let tiles: Vec<(f32, f32)> = (0..)
            .map_while(|index| strip.bounds_for_item(index))
            .map(|tile| (f32::from(tile.left()) - left, f32::from(tile.right()) - left))
            .collect();
        let offset = -f32::from(strip.offset().x);
        let target = edge.page(offset, view.size.width.into(), &tiles, cover);
        let target = target.min(strip.max_offset().width.into()).max(0.);
        strip.set_offset(point(px(-target), px(0.)));
        cx.notify();
    }

    pub fn has_action(&self, slot: &Slot) -> bool {
        self.tiles.find(slot).is_some()
    }

    pub(super) fn add_action(&mut self, action: Action, cx: &mut Context<Self>) -> Option<TileId> {
        if self.refuse_when_full(cx) {
            return None;
        }
        let tile = self.tiles.add(action)?;
        self.save_tiles(cx);
        Some(tile)
    }

    fn refuse_when_full(&mut self, cx: &mut Context<Self>) -> bool {
        let full = self.tiles.full();
        if full {
            self.fail(fill(tr().tiles_full, actions::MAX_TILES), cx);
        }
        full
    }

    pub fn move_tile(&mut self, tile: TileId, target: TileId, cx: &mut Context<Self>) {
        if self.tiles.move_to(tile, target) {
            self.save_tiles(cx);
        }
    }

    pub fn add_or_edit(&mut self, action: Action, focus: Option<Field>, window: &mut Window, cx: &mut Context<Self>) {
        let tile = match self.tiles.find(&action.slot()) {
            Some(tile) => Some(tile),
            None => self.add_action(action, cx),
        };
        if let Some(tile) = tile {
            self.open_editor(tile, focus, window, cx);
        }
    }

    pub fn add_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let search = |url: &str| Action::new(actions::Kind::Search { url: url.into() });
        let action = actions::ENGINES
            .into_iter()
            .map(search)
            .find(|action| !self.has_action(&action.slot()))
            .unwrap_or_else(|| search(""));
        let focus = (!action.ready()).then_some(Field::SearchUrl);
        self.add_or_edit(action, focus, window, cx);
    }

    pub fn set_engine(&mut self, tile: TileId, engine: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(actions::Kind::Search { url }) = self.tiles.get_mut(tile).map(|a| &mut a.kind) else {
            return;
        };
        *url = engine.into();
        self.save_tiles(cx);
        self.open_editor(tile, engine.is_empty().then_some(Field::SearchUrl), window, cx);
    }

    pub fn toggle_action(&mut self, action: Action, focus: Option<Field>, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(tile) = self.tiles.find(&action.slot()) {
            return self.remove_action(tile, cx);
        }
        let ready = self.ready(&action);
        let Some(tile) = self.add_action(action, cx) else {
            return;
        };
        if focus.is_some() || !ready {
            self.open_editor(tile, focus, window, cx);
        }
    }

    pub fn open_editor(&mut self, tile: TileId, focus: Option<Field>, window: &mut Window, cx: &mut Context<Self>) {
        self.edit(tile, cx);
        let input = self
            .editor()
            .and_then(|editor| editor.inputs.iter().find(|(field, _)| Some(*field) == focus))
            .map(|(_, input)| input.clone());
        if let Some(input) = input {
            input.read(cx).focus(window);
        }
    }

    pub(super) fn edit(&mut self, tile: TileId, cx: &mut Context<Self>) {
        let Some(action) = self.tiles.get(tile) else {
            return;
        };
        let languages = match action.kind {
            actions::Kind::Ocr { .. } => self.services.ocr_languages(),
            _ => Vec::new(),
        };
        let (settings, mut subscriptions) = self.setting_inputs(tile, action, cx);
        let mut inputs = Vec::new();
        for &field in action.fields() {
            let placeholder = match field {
                Field::Name => action.default_title(),
                Field::Description => action.verb().to_string(),
                _ => field.placeholder().to_string(),
            };
            let value = action.text(field);
            let input = cx.new(|cx| TextInput::new(value, placeholder, field.secret(), cx));
            subscriptions.push(cx.subscribe(&input, move |shelf, input, _: &Changed, cx| {
                let text = input.read(cx).text().to_string();
                shelf.set_text(tile, field, text, cx);
            }));
            subscriptions.push(cx.subscribe(&input, |shelf, _, _: &Submitted, cx| shelf.leave_editor(cx)));
            inputs.push((field, input));
        }
        self.navigate(Screen::Edit(Editor { tile, inputs, settings, languages, _subscriptions: subscriptions }), cx);
    }

    fn set_text(&mut self, tile: TileId, field: Field, text: String, cx: &mut Context<Self>) {
        if let Some(slot) = self.tiles.get_mut(tile).and_then(|a| a.text_mut(field)) {
            *slot = text;
            self.save_tiles(cx);
        }
    }

    pub fn set_copy(&mut self, tile: TileId, copy: bool, cx: &mut Context<Self>) {
        let Some(action) = self.tiles.get_mut(tile) else {
            return;
        };
        let (actions::Kind::Move { folder } | actions::Kind::Copy { folder }) = &action.kind else {
            return;
        };
        let folder = folder.clone();
        action.kind = if copy { actions::Kind::Copy { folder } } else { actions::Kind::Move { folder } };
        self.save_tiles(cx);
    }

    pub fn set_icon_only(&mut self, tile: TileId, on: bool, cx: &mut Context<Self>) {
        if let Some(action) = self.tiles.get_mut(tile) {
            action.icon_only = on;
            self.save_tiles(cx);
        }
    }

    pub fn edit_kind(&mut self, tile: TileId, cx: &mut Context<Self>, change: impl FnOnce(&mut actions::Kind)) {
        if let Some(action) = self.tiles.get_mut(tile) {
            change(&mut action.kind);
            self.save_tiles(cx);
        }
    }

    pub fn change_path(&mut self, tile: TileId, cx: &mut Context<Self>) {
        let Some(action) = self.tiles.get(tile) else {
            return;
        };
        let Some(current) = action.path().map(PathBuf::from) else {
            return;
        };
        let folders = !matches!(action.kind, actions::Kind::Run { .. });
        let title = if folders { tr().pick_folder } else { tr().pick_program };
        let services = self.services.clone();
        self.dialog(
            cx,
            move || {
                services.pick(title, folders).map(|picked| {
                    picked.map(|path| {
                        let names = (services.display_name(&current), services.display_name(&path));
                        (path, names)
                    })
                })
            },
            move |shelf, result, cx| match result {
                Ok(Some((path, (old, new)))) => shelf.set_path(tile, path, &old, new, cx),
                Ok(None) => {}
                Err(error) => shelf.fail(error.to_string(), cx),
            },
        );
    }

    fn set_path(&mut self, tile: TileId, path: PathBuf, old: &str, new: String, cx: &mut Context<Self>) {
        let Some(action) = self.tiles.get_mut(tile) else {
            return;
        };
        if let Some(slot) = action.path_mut() {
            *slot = path;
        }
        if action.name.is_empty() || action.name == old {
            action.name = new.clone();
            let input = self
                .editor()
                .filter(|editor| editor.tile == tile)
                .and_then(|editor| editor.inputs.iter().find(|(field, _)| *field == Field::Name))
                .map(|(_, input)| input.clone());
            if let Some(input) = input {
                input.update(cx, |input, cx| input.set_text(new, cx));
            }
        }
        self.save_tiles(cx);
    }

    pub fn pick_action(
        &mut self,
        title: &'static str,
        folders: bool,
        kind: fn(PathBuf) -> actions::Kind,
        cx: &mut Context<Self>,
    ) {
        if self.refuse_when_full(cx) {
            return;
        }
        let services = self.services.clone();
        self.dialog(
            cx,
            move || {
                services.pick(title, folders).map(|picked| {
                    picked.map(|path| Action { name: services.display_name(&path), ..Action::new(kind(path)) })
                })
            },
            |shelf, result, cx| match result {
                Ok(Some(action)) => {
                    shelf.add_action(action, cx);
                }
                Ok(None) => {}
                Err(error) => shelf.fail(error.to_string(), cx),
            },
        );
    }

    pub fn remove_action(&mut self, tile: TileId, cx: &mut Context<Self>) {
        let Some(action) = self.tiles.remove(tile) else {
            return;
        };
        self.jobs.read(cx).cancel_installs(&action.slot());
        self.leave_editor(cx);
        self.save_tiles(cx);
    }

    pub(super) fn reset_tiles(&mut self, cx: &mut Context<Self>) {
        if let Err(error) = self.tiles.reset(&*self.services) {
            self.fail(fill(tr().actions_save_failed, &error), cx);
        }
        self.detect_tools(cx);
        self.fit(cx);
    }
}
