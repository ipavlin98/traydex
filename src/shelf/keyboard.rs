use super::{Page, Screen, Shelf};
use crate::keys::Command;
use crate::store::Layout;
use gpui::*;

impl Shelf {
    pub(crate) fn focus_handle(&self) -> &FocusHandle {
        &self.focus
    }

    pub fn keys(&self, command: Command) -> Vec<SharedString> {
        self.store.settings().shortcuts.get(command).labels()
    }

    pub(super) fn focused_input(&self, window: &Window, cx: &App) -> &'static str {
        if self.focus.is_focused(window) {
            "shelf"
        } else if self.search.read(cx).focused(window) {
            "shelf search"
        } else if self.find.input().read(cx).focused(window) {
            "page search"
        } else if self.note_focused(window, cx) {
            "note"
        } else if self.title_focused(window, cx) {
            "note title"
        } else if self.unlocking().is_some_and(|unlocking| unlocking.password.read(cx).focused(window)) {
            "password"
        } else if self.renaming().is_some_and(|renaming| renaming.name.read(cx).focused(window)) {
            "rename"
        } else if self.editor().is_some_and(|editor| {
            let mut inputs =
                editor.inputs.iter().map(|(_, input)| input).chain(editor.settings.iter().map(|(_, input)| input));
            inputs.any(|input| input.read(cx).focused(window))
        }) {
            "tile editor"
        } else if window.focused(cx).is_some() {
            "other"
        } else {
            "nothing"
        }
    }

    pub(super) fn escape(&mut self, cx: &mut Context<Self>) {
        if self.menu.close() || self.confirming.take().is_some() {
            return cx.notify();
        }
        match self.page() {
            Page::Edit => self.navigate(Screen::Actions, cx),
            Page::Shelf if !self.selection.is_empty() => self.clear_selection(cx),
            Page::Shelf => self.hide("Esc pressed", cx),
            _ => self.navigate(Screen::Shelf, cx),
        }
    }

    pub fn key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        let control = event.keystroke.modifiers.control;
        let slash = event.keystroke.key_char.as_deref() == Some("/") || matches!(key, "/" | ".");
        if self.search_input().read(cx).focused(window) {
            if self.page() == Page::Shelf && matches!(key, "up" | "down") {
                let step = self.row_step();
                self.step_hover(if key == "up" { -step } else { step }, cx);
                cx.stop_propagation();
                return;
            }
            if key == "escape" {
                if self.search_input().read(cx).text().is_empty() {
                    window.focus(&self.focus, cx);
                } else {
                    self.clear_current_search(cx);
                }
                cx.stop_propagation();
            }
            return;
        }
        if self.title_focused(window, cx) {
            if matches!(key, "escape" | "tab") {
                self.focus_note(window, cx);
                cx.stop_propagation();
            }
            return;
        }
        if self.note_focused(window, cx) {
            match key {
                "escape" => {
                    self.leave_preview(cx);
                    window.focus(&self.focus, cx);
                }
                "f2" | "tab" => self.focus_title(window, cx),
                _ => return,
            }
            cx.stop_propagation();
            return;
        }
        let page = self.page();
        let grid = self.store.settings().layout == Layout::Grid;
        match key {
            _ if slash && !control && self.searchable() => {
                self.search_input().update(cx, |input, cx| input.focus(window, cx));
                cx.stop_propagation();
                cx.notify();
            }
            "escape" if page == Page::Preview && self.back_to_note(window, cx) => {}
            "escape" => self.escape(cx),
            "enter" if self.confirming.is_some() => self.confirm(window, cx),
            "up" if page == Page::Shelf => self.step_hover(-self.row_step(), cx),
            "down" if page == Page::Shelf => self.step_hover(self.row_step(), cx),
            "left" if page == Page::Shelf && grid => self.step_hover(-1, cx),
            "right" if page == Page::Shelf && grid => self.step_hover(1, cx),
            "left" | "up" if page == Page::Preview => self.step_preview(-1, window, cx),
            "right" | "down" if page == Page::Preview => self.step_preview(1, window, cx),
            "f2" if page == Page::Preview => self.focus_title(window, cx),
            _ => {
                let desktop = self.desktop.clone();
                if let Some(command) =
                    self.store.settings().shortcuts.find(&event.keystroke, &|key| desktop.layout_char(key))
                {
                    self.run(command, window, cx);
                }
            }
        }
    }

    pub(super) fn run(&mut self, command: Command, window: &mut Window, cx: &mut Context<Self>) {
        let page = self.page();
        let shelf = page == Page::Shelf;
        let actions = page == Page::Actions;
        let previewing = page == Page::Preview;
        match command {
            Command::Preview if shelf || previewing => self.toggle_preview(window, cx),
            Command::Open | Command::Reveal if previewing => self.open_previewed(command == Command::Reveal, cx),
            Command::Mark if shelf => {
                if let Some(key) = self.hovered_item() {
                    self.toggle(&key, cx);
                }
            }
            Command::Copy if previewing => self.copy_previewed(cx),
            Command::Copy if page == Page::Share => self.copy_share_link(cx),
            Command::Remove if previewing => self.remove_previewed(window, cx),
            Command::Pin => self.toggle_pinned(cx),
            Command::Actions => self.toggle_page(Page::Actions, cx),
            Command::Settings => self.toggle_page(Page::Settings, cx),
            Command::Storage => self.open_storage(),
            Command::Layout if shelf => self.toggle_layout(cx),
            Command::Paste => self.paste(cx),
            Command::Note => self.new_note(window, cx),
            Command::Copy => self.copy_targets(cx),
            Command::SelectAll if shelf => self.select_all(cx),
            Command::Clear if shelf => self.ask_clear(cx),
            Command::Stick if shelf => self.stick_targets(cx),
            Command::Remove if shelf => self.remove_targets(cx),
            Command::Remove if actions => {
                if let Some(tile) = self.hovered_tile() {
                    self.remove_action(tile, cx);
                }
            }
            Command::Open | Command::Reveal if shelf => {
                for key in self.targets() {
                    self.open(&key, command == Command::Reveal, cx);
                }
            }
            Command::Open if actions => {
                if let Some(tile) = self.hovered_tile() {
                    self.open_editor(tile, None, window, cx);
                }
            }
            _ => {}
        }
    }
}
