use super::{Page, Screen, Shelf};
use crate::i18n::{self, Language, fill, tr};
use crate::keys::{Command, Scope, Shortcut};
use crate::store::{self, Corner, Settings, ShakeLevel, TileInput};
use crate::theme::ThemeMode;
use gpui::*;
use std::time::Duration;

const HOVER_TIME: Duration = Duration::from_millis(150);

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Dropdown {
    Language,
    TileInput,
}

#[derive(Default)]
pub struct Menu {
    open: Option<Dropdown>,
    dismissed: Option<Dropdown>,
}

impl Menu {
    pub fn open(&self) -> Option<Dropdown> {
        self.open
    }

    pub(super) fn close(&mut self) -> bool {
        self.open.take().is_some()
    }
}

#[derive(Default)]
pub struct Recorder {
    command: Option<Command>,
    held: Shortcut,
}

impl Recorder {
    pub fn command(&self) -> Option<Command> {
        self.command
    }

    pub fn held(&self) -> Shortcut {
        self.held
    }
}

fn differs<T: PartialEq>(old: Option<&Settings>, new: &Settings, field: impl Fn(&Settings) -> T) -> bool {
    old.is_none_or(|old| field(old) != field(new))
}

impl Shelf {
    pub(crate) fn menu(&self) -> &Menu {
        &self.menu
    }

    pub(crate) fn recorder(&self) -> &Recorder {
        &self.recorder
    }

    pub(super) fn update_settings(&mut self, change: impl FnOnce(&mut Settings), cx: &mut Context<Self>) {
        let old = self.store.settings().clone();
        change(self.store.settings_mut());
        self.sync_settings(Some(&old), cx);
        cx.notify();
    }

    pub(super) fn sync_settings(&mut self, old: Option<&Settings>, cx: &mut Context<Self>) {
        let new = self.store.settings().clone();
        if differs(old, &new, |s| s.automatic) {
            self.desktop.set_automatic(new.automatic);
        }
        if differs(old, &new, |s| s.shake) {
            self.desktop.set_shake(new.shake);
        }
        if differs(old, &new, |s| s.corner) {
            self.desktop.set_corner(new.corner);
        }
        if differs(old, &new, Settings::corner_drag_on) {
            self.desktop.set_corner_drag(new.corner_drag_on());
        }
        if differs(old, &new, |s| s.tray_icon) {
            self.desktop.set_tray(new.tray_icon);
        }
        if differs(old, &new, |s| s.check_updates) {
            self.updates.update(cx, |updates, cx| updates.schedule(new.check_updates, cx));
        }
        if differs(old, &new, |s| s.theme) {
            self.apply_theme(cx);
        }
        if differs(old, &new, |s| s.font_size) {
            self.fit(cx);
        }
        if differs(old, &new, |s| s.interface_animations) {
            self.apply_motion(cx);
        }
        if !new.window_animations && differs(old, &new, |s| s.window_animations) {
            self.fade.update(cx, |fade, cx| fade.reset(cx));
        }
        let relabel = old.is_some_and(|old| old.language != new.language);
        if relabel {
            i18n::set(new.language.unwrap_or_else(Language::system));
            self.translate(cx);
            self.refresh(cx);
        }
        if differs(old, &new, |s| s.autostart)
            && let Err(error) = self.desktop.set_autostart(new.autostart)
        {
            if old.is_some() {
                self.store.settings_mut().autostart = !new.autostart;
                self.fail(fill(tr().autostart_change_failed, &error), cx);
            } else {
                self.fail(fill(tr().autostart_setup_failed, &error), cx);
            }
        }
        let switched = differs(old, &new, |s| s.context_menu) && (old.is_some() || new.context_menu);
        if (switched || relabel && new.context_menu) && !self.register_menu(new.context_menu, cx) && switched {
            self.store.settings_mut().context_menu = !new.context_menu;
        }
    }

    pub fn reset_settings(&mut self, cx: &mut Context<Self>) {
        log::info!("Resetting all settings");
        self.confirming = None;
        self.menu.close();
        self.stop_recording(cx);
        if self.page() == Page::Edit {
            self.navigate(Screen::Actions, cx);
        }
        self.store.reset_settings();
        i18n::set(Language::system());
        self.translate(cx);
        self.sync_settings(None, cx);
        for (command, shortcut) in self.store.settings().shortcuts.system() {
            if !self.desktop.bind(command, shortcut) {
                self.fail(fill(tr().hotkey_busy, shortcut.text()), cx);
            }
        }
        self.reset_tiles(cx);
        let (width, height) = store::DEFAULT_SIZE;
        self.after_update(
            move |desktop| {
                desktop.resize(width, height);
                desktop.reset_position();
            },
            cx,
        );
        self.refresh(cx);
    }

    pub fn set_theme(&mut self, mode: ThemeMode, cx: &mut Context<Self>) {
        self.update_settings(|settings| settings.theme = mode, cx);
    }

    pub fn set_transparency(&mut self, transparency: u8, cx: &mut Context<Self>) {
        self.update_settings(|settings| settings.transparency = transparency, cx);
    }

    pub fn set_font_size(&mut self, font_size: u8, cx: &mut Context<Self>) {
        self.update_settings(|settings| settings.font_size = font_size, cx);
    }

    pub fn toggle_blur(&mut self, cx: &mut Context<Self>) {
        self.update_settings(Settings::toggle_blur, cx);
    }

    pub fn toggle_window_animations(&mut self, cx: &mut Context<Self>) {
        self.update_settings(|settings| settings.window_animations = !settings.window_animations, cx);
    }

    #[allow(dead_code, reason = "the interface animation setting is hidden for now")]
    pub fn toggle_interface_animations(&mut self, cx: &mut Context<Self>) {
        self.update_settings(|settings| settings.interface_animations = !settings.interface_animations, cx);
    }

    pub(super) fn apply_motion(&self, cx: &mut Context<Self>) {
        cx.set_global(HoverTransition { duration: self.store.settings().interface_animations.then_some(HOVER_TIME) });
    }

    pub fn toggle_automatic(&mut self, cx: &mut Context<Self>) {
        self.update_settings(|settings| settings.automatic = !settings.automatic, cx);
    }

    pub fn toggle_double_click_preview(&mut self, cx: &mut Context<Self>) {
        self.update_settings(|settings| settings.double_click_preview = !settings.double_click_preview, cx);
    }

    pub fn toggle_tray_icon(&mut self, cx: &mut Context<Self>) {
        self.update_settings(|settings| settings.tray_icon = !settings.tray_icon, cx);
    }

    pub fn toggle_check_updates(&mut self, cx: &mut Context<Self>) {
        self.update_settings(|settings| settings.check_updates = !settings.check_updates, cx);
    }

    pub fn toggle_recycle(&mut self, cx: &mut Context<Self>) {
        self.update_settings(|settings| settings.recycle = !settings.recycle, cx);
    }

    pub fn toggle_separate_notes(&mut self, cx: &mut Context<Self>) {
        self.selection.reset();
        self.update_settings(|settings| settings.separate_notes = !settings.separate_notes, cx);
    }

    pub fn set_shake(&mut self, level: ShakeLevel, cx: &mut Context<Self>) {
        self.update_settings(|settings| settings.set_shake(level), cx);
    }

    pub fn set_corner(&mut self, corner: Option<Corner>, cx: &mut Context<Self>) {
        self.update_settings(|settings| settings.corner = corner, cx);
    }

    pub fn toggle_corner_drag(&mut self, cx: &mut Context<Self>) {
        self.update_settings(Settings::toggle_corner_drag, cx);
    }

    pub fn toggle_near_cursor(&mut self, cx: &mut Context<Self>) {
        self.update_settings(Settings::toggle_near_cursor, cx);
    }

    pub fn toggle_autostart(&mut self, cx: &mut Context<Self>) {
        self.update_settings(|settings| settings.autostart = !settings.autostart, cx);
    }

    fn register_menu(&mut self, enabled: bool, cx: &mut Context<Self>) -> bool {
        match self.desktop.set_context_menu(enabled, tr().context_menu_verb) {
            Ok(()) => true,
            Err(error) => {
                self.fail(fill(tr().context_menu_failed, &error), cx);
                false
            }
        }
    }

    pub fn toggle_context_menu(&mut self, cx: &mut Context<Self>) {
        self.update_settings(|settings| settings.context_menu = !settings.context_menu, cx);
    }

    pub fn toggle_pinned(&mut self, cx: &mut Context<Self>) {
        self.session.end();
        self.update_settings(|settings| settings.pinned = !settings.pinned, cx);
    }

    pub fn open_link(&self, url: impl Into<String>) {
        let url = url.into();
        let services = self.services.clone();
        self.services.spawn(Box::new(move || services.shell_open(&url)));
    }

    pub fn open_logs(&self) {
        let folder = crate::logging::folder(self.store.storage()).to_string_lossy().into_owned();
        self.open_link(folder);
    }

    pub fn open_storage(&self) {
        let folder = self.store.storage().content().to_string_lossy().into_owned();
        self.open_link(folder);
    }

    pub fn reset_position(&mut self, cx: &mut Context<Self>) {
        self.store.settings_mut().position = None;
        self.after_update(|desktop| desktop.reset_position(), cx);
    }

    pub fn reset_size(&mut self, cx: &mut Context<Self>) {
        self.store.settings_mut().size = None;
        let (width, height) = store::DEFAULT_SIZE;
        self.after_update(move |desktop| desktop.resize(width, height), cx);
    }

    pub fn fit_size(&mut self, cx: &mut Context<Self>) {
        let width = self.fitting_width.get();
        let (default_width, height) = store::DEFAULT_SIZE;
        let shown = self.page() == Page::Shelf && !self.tiles.is_empty();
        if !shown || width <= default_width {
            return self.reset_size(cx);
        }
        self.store.settings_mut().size = Some((width, height));
        self.after_update(move |desktop| desktop.resize(width, height), cx);
    }

    pub fn set_language(&mut self, language: Language, cx: &mut Context<Self>) {
        self.menu.close();
        self.update_settings(|settings| settings.language = Some(language), cx);
    }

    pub fn set_tile_input(&mut self, input: TileInput, cx: &mut Context<Self>) {
        self.menu.close();
        self.update_settings(|settings| settings.tile_input = input, cx);
    }

    pub fn toggle_menu(&mut self, menu: Dropdown, cx: &mut Context<Self>) {
        if self.menu.dismissed.take() == Some(menu) {
            return;
        }
        self.menu.open = if self.menu.open == Some(menu) { None } else { Some(menu) };
        cx.notify();
    }

    pub fn dismiss_menu(&mut self, cx: &mut Context<Self>) {
        self.menu.dismissed = self.menu.open;
        if self.menu.close() {
            cx.notify();
        }
    }

    pub fn pointer_pressed(&mut self, cx: &mut Context<Self>) {
        self.menu.dismissed = None;
        self.reclaim_focus(cx);
    }

    pub fn record(&mut self, command: Command, cx: &mut Context<Self>) {
        if self.recorder.command == Some(command) {
            return self.stop_recording(cx);
        }
        if self.recorder.command.is_none() {
            self.desktop.capture(true);
        }
        self.recorder = Recorder { command: Some(command), held: Shortcut::default() };
        cx.notify();
    }

    pub(super) fn holding(&mut self, held: Shortcut, cx: &mut Context<Self>) {
        if self.recorder.command.is_some() {
            self.recorder.held = held;
            cx.notify();
        }
    }

    pub fn stop_recording(&mut self, cx: &mut Context<Self>) {
        if std::mem::take(&mut self.recorder).command.is_some() {
            self.desktop.capture(false);
            cx.notify();
        }
    }

    pub(super) fn recorded(&mut self, shortcut: Shortcut, cx: &mut Context<Self>) {
        let Some(command) = self.recorder.command else {
            return;
        };
        if shortcut.is_cancel() {
            return self.stop_recording(cx);
        }
        if !shortcut.fits(command.scope()) {
            return self.fail(tr().shortcut_needs_modifier, cx);
        }
        if let Some(owner) = self.store.settings().shortcuts.owner(shortcut).filter(|owner| *owner != command) {
            return self.fail(fill(tr().shortcut_taken, owner.label()), cx);
        }
        self.stop_recording(cx);
        self.assign(command, shortcut, cx);
    }

    pub fn reset_shortcut(&mut self, command: Command, cx: &mut Context<Self>) {
        self.stop_recording(cx);
        self.assign(command, command.default_shortcut(), cx);
    }

    fn assign(&mut self, command: Command, shortcut: Shortcut, cx: &mut Context<Self>) {
        if command.scope() == Scope::System && !self.desktop.bind(command, shortcut) {
            return self.fail(fill(tr().hotkey_busy, shortcut.text()), cx);
        }
        log::info!("{command:?} is now {}", shortcut.text());
        self.store.settings_mut().shortcuts.set(command, shortcut);
        cx.notify();
    }

    fn translate(&mut self, cx: &mut Context<Self>) {
        for input in [&self.search, self.find.input()] {
            input.update(cx, |input, cx| input.set_placeholder(tr().search_placeholder, cx));
        }
    }

    pub fn toggle_page(&mut self, page: Page, cx: &mut Context<Self>) {
        self.session.end();
        self.stop_recording(cx);
        let screen = match page {
            _ if self.page() == page => Screen::Shelf,
            Page::Actions => Screen::Actions,
            Page::Settings => Screen::Settings,
            _ => Screen::Shelf,
        };
        self.show(screen, cx);
    }
}
