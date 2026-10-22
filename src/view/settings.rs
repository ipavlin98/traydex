use super::motion::{Enter, Motion};
use super::widgets::{
    Finder, Look, Marked, bare_button, dimmed, hint, icon, icon_button, kbd, page, searched, section, segment_base,
    segments, setting, spinner, switch, text_button, unmatched,
};
use crate::design::{glyph, line, radius, rem, size, space, text};
use crate::i18n::{self, Language, tr};
use crate::keys::Command;
use crate::shelf::{Dropdown, Page, SafeListener, Shelf};
use crate::store::{Corner, FONT_SIZES, ShakeLevel, TileInput};
use crate::theme::{Theme, ThemeMode};
use gpui::{prelude::*, *};
use std::path::Path;

const PICKER_WIDTH: f32 = 152.;
const MENU_WIDTH: f32 = 180.;
const MENU_OFFSET: f32 = size::CONTROL + space::XS;
const SHORTCUT_HEIGHT: f32 = 26.;
const FONT_LABELS: [&str; 5] = ["12", "13", "14", "15", "16"];

struct Picker<T: 'static> {
    menu: Dropdown,
    current: T,
    options: &'static [T],
    label: fn(T) -> Div,
    pick: fn(&mut Shelf, T, &mut Context<Shelf>),
}

fn dropdown_name(menu: Dropdown) -> &'static str {
    match menu {
        Dropdown::Language => "language",
        Dropdown::TileInput => "tile-input",
    }
}

fn tile_input_label(input: TileInput) -> &'static str {
    match input {
        TileInput::Clipboard => tr().input_clipboard,
        TileInput::Selection => tr().input_selection,
        TileInput::Picker => tr().input_picker,
    }
}

impl Shelf {
    fn segmented<T: Copy + PartialEq + 'static>(
        &self,
        t: Theme,
        id: &'static str,
        current: T,
        options: Vec<(T, Option<&'static str>, &'static str, bool)>,
        pick: fn(&mut Self, T, &mut Context<Self>),
        cx: &mut Context<Self>,
    ) -> Div {
        segments(t).marked(format!("segmented:{id}")).flex_shrink_0().children(options.into_iter().enumerate().map(
            |(index, (value, name, label, compact))| {
                let active = current == value;
                segment_base(t, (id, index), active)
                    .px(rem(if compact { space::S } else { space::M }))
                    .gap(rem(space::S))
                    .when_some(name, |d, name| d.child(icon(name, if active { t.text } else { t.muted }, glyph::S)))
                    .map(|d| if compact { d.tooltip(hint(t, label)) } else { d.child(label) })
                    .on_click(cx.safe_listener(move |shelf, _, _, cx| pick(shelf, value, cx)))
            },
        ))
    }

    fn theme_switcher(&self, t: Theme, cx: &mut Context<Self>) -> Div {
        let options = vec![
            (ThemeMode::System, Some("monitor"), tr().theme_system, false),
            (ThemeMode::Light, Some("sun"), tr().theme_light, false),
            (ThemeMode::Dark, Some("moon"), tr().theme_dark, false),
        ];
        self.segmented(t, "theme", self.store().settings().theme, options, Self::set_theme, cx)
    }

    fn transparency_picker(&self, t: Theme, cx: &mut Context<Self>) -> Div {
        let options = vec![
            (0, None, "0%", false),
            (10, None, "10%", false),
            (20, None, "20%", false),
            (30, None, "30%", false),
            (40, None, "40%", false),
        ];
        self.segmented(t, "transparency", self.store().settings().transparency, options, Self::set_transparency, cx)
    }

    fn font_size_picker(&self, t: Theme, cx: &mut Context<Self>) -> Div {
        let options = FONT_SIZES.into_iter().zip(FONT_LABELS).map(|(size, label)| (size, None, label, false)).collect();
        self.segmented(t, "font-size", self.store().settings().font_size, options, Self::set_font_size, cx)
    }

    fn corner_drag_switch(&self, t: Theme, cx: &mut Context<Self>) -> Stateful<Div> {
        let settings = self.store().settings();
        let reason = if settings.corner.is_none() {
            Some(tr().corner_drag_locked)
        } else if settings.shows_at_once() {
            Some(tr().corner_drag_at_once)
        } else {
            None
        };
        switch(t, "corner-drag", settings.corner_drag && reason.is_none())
            .when_some(reason, |d, reason| dimmed(d).tooltip(hint(t, reason)))
            .on_click(cx.safe_listener(|shelf, _, _, cx| shelf.toggle_corner_drag(cx)))
    }

    fn near_cursor_switch(&self, t: Theme, cx: &mut Context<Self>) -> Stateful<Div> {
        let settings = self.store().settings();
        let locked = !settings.automatic;
        switch(t, "near-cursor", settings.near_cursor && !locked)
            .when(locked, |d| dimmed(d).tooltip(hint(t, tr().auto_show_locked)))
            .on_click(cx.safe_listener(|shelf, _, _, cx| shelf.toggle_near_cursor(cx)))
    }

    fn corner_picker(&self, t: Theme, cx: &mut Context<Self>) -> Div {
        let options = vec![
            (None, None, tr().corner_off, false),
            (Some(Corner::TopLeft), Some("corner-top-left"), tr().corner_top_left, true),
            (Some(Corner::TopRight), Some("corner-top-right"), tr().corner_top_right, true),
            (Some(Corner::BottomLeft), Some("corner-bottom-left"), tr().corner_bottom_left, true),
            (Some(Corner::BottomRight), Some("corner-bottom-right"), tr().corner_bottom_right, true),
        ];
        self.segmented(t, "corner", self.store().settings().corner, options, Self::set_corner, cx)
    }

    fn shake_picker(&self, t: Theme, cx: &mut Context<Self>) -> Div {
        let options = vec![
            (ShakeLevel::Off, None, tr().shake_off, false),
            (ShakeLevel::Low, None, tr().shake_low, false),
            (ShakeLevel::High, None, tr().shake_high, false),
        ];
        let settings = self.store().settings();
        let locked = !settings.automatic;
        let current = if locked { ShakeLevel::Off } else { settings.shake };
        div().relative().flex_shrink_0().child(self.segmented(t, "shake", current, options, Self::set_shake, cx)).when(
            locked,
            |d| {
                dimmed(d).child(
                    div()
                        .id("shake-locked")
                        .absolute()
                        .inset_0()
                        .occlude()
                        .cursor(CursorStyle::OperationNotAllowed)
                        .tooltip(hint(t, tr().auto_show_locked)),
                )
            },
        )
    }

    fn dropdown<T: Copy + PartialEq + 'static>(&self, t: Theme, picker: Picker<T>, cx: &mut Context<Self>) -> Div {
        let name = dropdown_name(picker.menu);
        let open = self.menu().open() == Some(picker.menu);
        let menu = picker.menu;
        div()
            .relative()
            .flex_shrink_0()
            .child(
                div()
                    .id(name)
                    .marked(name)
                    .h(rem(size::CONTROL))
                    .min_w(rem(PICKER_WIDTH))
                    .px(rem(space::L))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(rem(space::M))
                    .rounded(rem(radius::CONTROL))
                    .border_1()
                    .border_color(if open { t.accent } else { t.border_strong })
                    .text_size(rem(text::SMALL))
                    .cursor_pointer()
                    .hover(move |s| s.bg(t.hover))
                    .child((picker.label)(picker.current))
                    .child(icon("chevron", t.muted, glyph::XS))
                    .on_click(cx.safe_listener(move |shelf, _, _, cx| shelf.toggle_menu(menu, cx))),
            )
            .when(open, |d| d.child(self.dropdown_menu(t, name, &picker, cx)))
    }

    fn dropdown_menu<T: Copy + PartialEq + 'static>(
        &self,
        t: Theme,
        name: &'static str,
        picker: &Picker<T>,
        cx: &mut Context<Self>,
    ) -> Div {
        let pick = picker.pick;
        let options = picker.options.iter().copied().enumerate().map(|(index, value)| {
            let active = value == picker.current;
            div()
                .id(SharedString::from(format!("{name}-option-{index}")))
                .h(rem(size::CONTROL))
                .px(rem(space::M))
                .flex()
                .items_center()
                .justify_between()
                .gap(rem(space::M))
                .rounded(rem(radius::SMALL))
                .cursor_pointer()
                .text_size(rem(text::SMALL))
                .text_color(if active { t.text } else { t.muted })
                .when(active, |d| d.bg(t.active))
                .hover(move |s| s.bg(t.hover).text_color(t.text))
                .child((picker.label)(value))
                .when(active, |d| d.child(icon("check", t.accent_text, glyph::XS)))
                .on_click(cx.safe_listener(move |shelf, _, _, cx| pick(shelf, value, cx)))
        });
        let menu = div()
            .id(SharedString::from(format!("{name}-menu")))
            .marked(format!("{name}-menu"))
            .min_w(rem(MENU_WIDTH))
            .p(rem(space::XS))
            .flex()
            .flex_col()
            .gap(rem(space::X2S))
            .rounded(rem(radius::CONTROL))
            .border_1()
            .border_color(t.border_strong)
            .bg(t.surface)
            .occlude()
            .on_mouse_down_out(cx.safe_listener(|shelf, _, _, cx| shelf.dismiss_menu(cx)))
            .children(options)
            .enter(SharedString::from(format!("{name}-menu-enter")), Motion::Drop, self.animated());
        div().absolute().top(rem(MENU_OFFSET)).right_0().child(
            deferred(anchored().anchor(gpui::Corner::TopRight).snap_to_window_with_margin(px(space::XS)).child(menu))
                .with_priority(1),
        )
    }

    fn language_picker(&self, t: Theme, cx: &mut Context<Self>) -> Div {
        let picker = Picker {
            menu: Dropdown::Language,
            current: i18n::current(),
            options: &Language::ALL,
            label: |language| div().font_family(language.font()).child(language.native_name()),
            pick: Self::set_language,
        };
        self.dropdown(t, picker, cx)
    }

    fn tile_input_picker(&self, t: Theme, cx: &mut Context<Self>) -> Div {
        let picker = Picker {
            menu: Dropdown::TileInput,
            current: self.store().settings().tile_input,
            options: &TileInput::ALL,
            label: |input| div().whitespace_nowrap().child(tile_input_label(input)),
            pick: Self::set_tile_input,
        };
        self.dropdown(t, picker, cx)
    }

    fn shortcut(&self, t: Theme, command: Command, cx: &mut Context<Self>) -> Div {
        let keys = self.keys(command);
        if !command.editable() {
            return kbd(t, &keys);
        }
        let recording = self.recorder().command() == Some(command);
        let field = div()
            .id(("shortcut", command as usize))
            .h(rem(SHORTCUT_HEIGHT))
            .px(rem(space::X2S))
            .flex()
            .items_center()
            .gap(rem(space::XS))
            .rounded(rem(radius::SMALL))
            .border_1()
            .cursor_pointer()
            .on_click(cx.safe_listener(move |shelf, _, _, cx| shelf.record(command, cx)));
        let field = if recording {
            let held = self.recorder().held().labels();
            field
                .border_color(t.accent)
                .when(!held.is_empty(), |d| d.child(kbd(t, &held)))
                .child(
                    div()
                        .px(rem(space::XS))
                        .text_size(rem(text::SMALL))
                        .text_color(t.muted)
                        .child(tr().shortcut_prompt),
                )
                .on_mouse_down_out(cx.safe_listener(|shelf, _, _, cx| shelf.stop_recording(cx)))
        } else {
            field
                .border_color(transparent_black())
                .hover(move |s| s.bg(t.hover))
                .child(kbd(t, &keys))
                .tooltip(hint(t, tr().shortcut_edit))
        };
        div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(rem(space::XS))
            .when(!recording && self.store().settings().shortcuts.customized(command), |d| {
                d.child(
                    icon_button(t, ("shortcut-reset", command as usize), "reset", tr().shortcut_reset, false)
                        .on_click(cx.safe_listener(move |shelf, _, _, cx| shelf.reset_shortcut(command, cx))),
                )
            })
            .child(field)
    }

    fn setting_switch(
        &self,
        t: Theme,
        id: &'static str,
        on: bool,
        flip: fn(&mut Self, &mut Context<Self>),
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        switch(t, id, on).on_click(cx.safe_listener(move |shelf, _, _, cx| flip(shelf, cx)))
    }

    fn reset_button(
        &self,
        t: Theme,
        id: &'static str,
        look: Look,
        reset: fn(&mut Self, &mut Context<Self>),
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        text_button(t, id, tr().reset, look).on_click(cx.safe_listener(move |shelf, _, _, cx| reset(shelf, cx)))
    }

    fn appearance_section(&self, t: Theme, query: Finder, cx: &mut Context<Self>) -> Div {
        let settings = self.store().settings();
        section(t, tr().section_appearance)
            .child(setting(t, query, tr().theme, None).child(self.theme_switcher(t, cx)))
            .child(setting(t, query, tr().transparency, None).child(self.transparency_picker(t, cx)))
            .child(setting(t, query, tr().font_size, None).child(self.font_size_picker(t, cx)))
            .child(
                setting(t, query, tr().blur, Some(tr().blur_hint)).child(
                    self.setting_switch(t, "blur", settings.blurred(), Self::toggle_blur, cx)
                        .when(settings.window_animations, |d| dimmed(d).tooltip(hint(t, tr().blur_locked))),
                ),
            )
            .child(setting(t, query, tr().window_animations, Some(tr().window_animations_hint)).child(
                self.setting_switch(
                    t,
                    "window-animations",
                    settings.window_animations,
                    Self::toggle_window_animations,
                    cx,
                ),
            ))
            // .child(setting(t, query, tr().interface_animations, Some(tr().interface_animations_hint)).child(
            //     self.setting_switch(
            //         t,
            //         "interface-animations",
            //         settings.interface_animations,
            //         Self::toggle_interface_animations,
            //         cx,
            //     ),
            // ))
            .child(setting(t, query, tr().language, None).child(self.language_picker(t, cx)))
    }

    fn behavior_section(&self, t: Theme, query: Finder, cx: &mut Context<Self>) -> Div {
        let settings = self.store().settings();
        section(t, tr().section_behavior)
            .child(setting(t, query, tr().auto_show, Some(tr().auto_show_hint)).child(self.setting_switch(
                t,
                "automatic",
                settings.automatic,
                Self::toggle_automatic,
                cx,
            )))
            .child(setting(t, query, tr().shake_to_show, Some(tr().shake_hint)).child(self.shake_picker(t, cx)))
            .child(setting(t, query, tr().hot_corner, Some(tr().hot_corner_hint)).child(self.corner_picker(t, cx)))
            .child(
                setting(t, query, tr().corner_drag, Some(tr().corner_drag_hint)).child(self.corner_drag_switch(t, cx)),
            )
            .child(
                setting(t, query, tr().near_cursor, Some(tr().near_cursor_hint)).child(self.near_cursor_switch(t, cx)),
            )
            .child(setting(t, query, tr().tile_input, Some(tr().tile_input_hint)).child(self.tile_input_picker(t, cx)))
            .child(setting(t, query, tr().autostart, Some(tr().autostart_hint)).child(self.setting_switch(
                t,
                "autostart",
                settings.autostart,
                Self::toggle_autostart,
                cx,
            )))
            .child(setting(t, query, tr().tray_icon, Some(tr().tray_icon_hint)).child(self.setting_switch(
                t,
                "tray-icon",
                settings.tray_icon,
                Self::toggle_tray_icon,
                cx,
            )))
            .child(setting(t, query, tr().context_menu, Some(tr().context_menu_hint)).child(self.setting_switch(
                t,
                "context-menu",
                settings.context_menu,
                Self::toggle_context_menu,
                cx,
            )))
            .child(setting(t, query, tr().double_click_preview, Some(tr().double_click_preview_hint)).child(
                self.setting_switch(
                    t,
                    "double-click-preview",
                    settings.double_click_preview,
                    Self::toggle_double_click_preview,
                    cx,
                ),
            ))
            .child(setting(t, query, tr().separate_notes, Some(tr().separate_notes_hint)).child(self.setting_switch(
                t,
                "separate-notes",
                settings.separate_notes,
                Self::toggle_separate_notes,
                cx,
            )))
            .child(setting(t, query, tr().recycle, Some(tr().recycle_hint)).child(self.setting_switch(
                t,
                "recycle",
                settings.recycle,
                Self::toggle_recycle,
                cx,
            )))
            .child(setting(t, query, tr().update_auto, Some(tr().update_auto_hint)).child(self.setting_switch(
                t,
                "check-updates-auto",
                settings.check_updates,
                Self::toggle_check_updates,
                cx,
            )))
    }

    fn folders_section(&self, t: Theme, query: Finder, cx: &mut Context<Self>) -> Div {
        let missed = query.locate(tr().watch_folders, tr().watch_folders_hint).is_none();
        section(t, tr().section_folders)
            .child(
                setting(t, query, tr().watch_folders, Some(tr().watch_folders_hint)).child(
                    text_button(t, "watch-folder", tr().watch_add, Look::Plain)
                        .marked("watch-folder")
                        .on_click(cx.safe_listener(|shelf, _, _, cx| shelf.watch_folder(cx))),
                ),
            )
            .children(
                self.store()
                    .watched()
                    .iter()
                    .enumerate()
                    .map(|(index, watched)| self.watched_row(t, index, watched.path(), missed, cx)),
            )
    }

    fn watched_row(&self, t: Theme, index: usize, folder: &Path, missed: bool, cx: &mut Context<Self>) -> Div {
        let target = folder.to_path_buf();
        div()
            .marked(format!("watched:{index}"))
            .when(missed, unmatched)
            .py(rem(space::S))
            .flex()
            .items_center()
            .gap(rem(space::M))
            .border_b_1()
            .border_color(t.border)
            .text_size(rem(text::SMALL))
            .line_height(rem(line::SMALL))
            .child(icon("folder", t.muted, glyph::S))
            .child(div().flex_1().min_w_0().truncate().child(folder.display().to_string()))
            .child(
                icon_button(t, ("unwatch", index), "close", tr().watch_remove, false)
                    .on_click(cx.safe_listener(move |shelf, _, _, cx| shelf.unwatch_folder(&target, cx))),
            )
    }

    pub(super) fn settings_page(&self, t: Theme, cx: &mut Context<Self>) -> AnyElement {
        let query = self.page_query(cx);
        let content = searched(query.as_ref(), self.regions(), |finder| self.settings_content(t, finder, cx));
        self.scroll_area(t, Page::Settings, "settings-page", content)
    }

    fn settings_content(&self, t: Theme, query: Finder, cx: &mut Context<Self>) -> Stateful<Div> {
        page("settings")
            .gap(rem(space::SECTION))
            .child(self.appearance_section(t, query, cx))
            .child(self.behavior_section(t, query, cx))
            .child(self.folders_section(t, query, cx))
            .child(
                section(t, tr().section_keys).children(
                    Command::ALL
                        .into_iter()
                        .map(|command| setting(t, query, command.label(), None).child(self.shortcut(t, command, cx))),
                ),
            )
            .child(
                section(t, tr().section_logs).child(
                    setting(t, query, tr().logs, Some(tr().logs_hint)).child(
                        text_button(t, "logs", tr().open_button, Look::Plain)
                            .on_click(cx.safe_listener(|shelf, _, _, _| shelf.open_logs())),
                    ),
                ),
            )
            .child(
                section(t, tr().section_reset)
                    .child(setting(t, query, tr().position, Some(tr().position_hint)).child(self.reset_button(
                        t,
                        "reset-position",
                        Look::Plain,
                        Self::reset_position,
                        cx,
                    )))
                    .child(setting(t, query, tr().size, Some(tr().size_hint)).child(self.reset_button(
                        t,
                        "reset-size",
                        Look::Plain,
                        Self::reset_size,
                        cx,
                    )))
                    .child(setting(t, query, tr().reset_all, Some(tr().reset_all_hint)).child(self.reset_button(
                        t,
                        "reset-all",
                        Look::Danger,
                        Self::ask_reset,
                        cx,
                    ))),
            )
            .child(self.version_footer(t, cx))
    }

    fn version_footer(&self, t: Theme, cx: &mut Context<Self>) -> Div {
        let check = if self.checking_updates(cx) {
            div().size(rem(size::BUTTON)).flex().items_center().justify_center().child(spinner(t.muted, glyph::S))
        } else {
            div().child(
                bare_button(t, "check-updates", icon("refresh", t.muted, glyph::S), tr().update_check, false, t.hover)
                    .on_click(cx.safe_listener(|shelf, _, _, cx| shelf.check_updates(cx))),
            )
        };
        div()
            .flex()
            .items_center()
            .gap(rem(space::XS))
            .text_size(rem(text::CAPTION))
            .text_color(t.faint)
            .child(crate::PRODUCT)
            .child(check.marked("check-updates"))
            .marked("version")
    }
}
