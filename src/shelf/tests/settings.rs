use super::*;

#[gpui::test]
fn larger_font_size_is_saved_and_grows_the_minimum_window(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    let (width, height) = h.min_size();
    h.act(|shelf, _, cx| shelf.set_font_size(16, cx));
    assert_eq!(h.saved().settings().font_size, 16);
    let (wider, taller) = h.min_size();
    assert!((wider - width * 16. / 13.).abs() < 0.01, "{wider} after {width}");
    assert!((taller - height * 16. / 13.).abs() < 0.01, "{taller} after {height}");
}

#[gpui::test]
fn shortcut_recording_validates_and_assigns(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    h.act(|shelf, _, cx| shelf.record(Command::Toggle, cx));
    assert_eq!(h.fake.calls().last(), Some(&Call::Capture(true)));
    let held = shortcut(true, VK_CONTROL);
    h.send(Event::Recording(held));
    assert_eq!(h.read(|shelf| shelf.recorder.held()), held);
    h.send(Event::Recorded(Command::Actions.default_shortcut()));
    assert_eq!(h.alert(), Some(fill(tr().shortcut_taken, Command::Actions.label())));
    assert_eq!(h.read(|shelf| shelf.recorder.command()), Some(Command::Toggle));
    let chosen = Shortcut { alt: true, ..shortcut(true, VK_J) };
    h.send(Event::Recorded(chosen));
    assert_eq!(h.read(|shelf| shelf.recorder.command()), None);
    assert!(h.fake.calls().ends_with(&[Call::Capture(false), Call::Bind(Command::Toggle, chosen)]));
    assert_eq!(h.saved().settings().shortcuts.get(Command::Toggle), chosen);
    h.send(Event::Recording(held));
    h.send(Event::Recorded(Command::Toggle.default_shortcut()));
    assert_eq!(h.read(|shelf| shelf.recorder.held()), Shortcut::default());
    assert_eq!(h.saved().settings().shortcuts.get(Command::Toggle), chosen);
    h.act(|shelf, _, cx| {
        shelf.dismiss_notice(crate::shelf::Notice::Alert, cx);
        shelf.record(Command::Toggle, cx);
    });
    h.send(Event::Recorded(chosen));
    assert_eq!(h.alert(), None);
    assert_eq!(h.read(|shelf| shelf.recorder.command()), None);
}

#[gpui::test]
fn system_shortcuts_need_modifiers_and_a_free_hotkey(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    h.act(|shelf, _, cx| shelf.record(Command::Toggle, cx));
    h.send(Event::Recorded(shortcut(false, VK_Q)));
    assert_eq!(h.alert().as_deref(), Some(tr().shortcut_needs_modifier));
    let busy = shortcut(true, VK_Q);
    h.fake.state().busy.push(busy);
    h.send(Event::Recorded(busy));
    assert_eq!(h.alert(), Some(fill(tr().hotkey_busy, busy.text())));
    assert_eq!(h.saved().settings().shortcuts.get(Command::Toggle), Command::Toggle.default_shortcut());
    h.act(|shelf, _, cx| shelf.record(Command::Toggle, cx));
    let free = Shortcut { alt: true, ..shortcut(true, VK_T) };
    h.send(Event::Recorded(free));
    assert!(h.fake.calls().contains(&Call::Bind(Command::Toggle, free)));
    assert_eq!(h.saved().settings().shortcuts.get(Command::Toggle), free);
    h.act(|shelf, _, cx| shelf.reset_shortcut(Command::Toggle, cx));
    assert_eq!(h.saved().settings().shortcuts.get(Command::Toggle), Command::Toggle.default_shortcut());
}

#[gpui::test]
fn recording_stops_on_cancel_repeat_page_change_and_focus_loss(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    h.act(|shelf, _, cx| shelf.record(Command::Copy, cx));
    h.act(|shelf, _, cx| shelf.record(Command::Paste, cx));
    assert_eq!(h.count(&Call::Capture(true)), 1);
    h.send(Event::Recorded(Shortcut::plain(VK_ESCAPE)));
    assert_eq!(h.read(|shelf| shelf.recorder.command()), None);
    assert_eq!(h.count(&Call::Capture(false)), 1);
    h.act(|shelf, _, cx| {
        shelf.record(Command::Copy, cx);
        shelf.record(Command::Copy, cx);
    });
    assert_eq!(h.count(&Call::Capture(false)), 2);
    h.act(|shelf, _, cx| {
        shelf.record(Command::Copy, cx);
        shelf.toggle_page(Page::Actions, cx);
    });
    assert_eq!(h.count(&Call::Capture(false)), 3);
    h.act(|shelf, _, cx| shelf.toggle_pinned(cx));
    h.act(|shelf, _, cx| shelf.record(Command::Copy, cx));
    h.activate();
    h.deactivate();
    assert_eq!(h.read(|shelf| shelf.recorder.command()), None);
    assert_eq!(h.count(&Call::Capture(false)), 4);
    assert!(h.fake.state().visible);
    assert_eq!(h.saved().settings().shortcuts.get(Command::Copy), Command::Copy.default_shortcut());
}

#[gpui::test]
fn settings_toggles_persist_and_reach_the_system(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    h.fake.take_calls();
    h.act(|shelf, _, cx| {
        shelf.set_shake(ShakeLevel::High, cx);
        shelf.toggle_automatic(cx);
        shelf.set_corner(Some(Corner::BottomRight), cx);
        shelf.toggle_autostart(cx);
        shelf.toggle_context_menu(cx);
        shelf.set_theme(ThemeMode::Dark, cx);
        shelf.set_transparency(40, cx);
        shelf.toggle_blur(cx);
        shelf.toggle_window_animations(cx);
        shelf.toggle_interface_animations(cx);
        shelf.toggle_tray_icon(cx);
    });
    let calls = h.fake.calls();
    for expected in [
        Call::Automatic(false),
        Call::Shake(ShakeLevel::High),
        Call::Corner(Some(Corner::BottomRight)),
        Call::Autostart(false),
        Call::Tray(false),
    ] {
        assert!(calls.contains(&expected), "{expected:?}");
    }
    assert!(calls.iter().any(|call| matches!(call, Call::ContextMenu(false, _))));
    let saved = h.saved();
    assert!(
        !saved.settings().automatic
            && saved.settings().shake == ShakeLevel::High
            && !saved.settings().autostart
            && !saved.settings().context_menu
    );
    assert_eq!(saved.settings().corner, Some(Corner::BottomRight));
    assert!(
        saved.settings().theme == ThemeMode::Dark
            && saved.settings().transparency == 40
            && saved.settings().blur
            && saved.settings().window_animations
            && saved.settings().interface_animations
            && !saved.settings().tray_icon
    );
    h.render();
}

#[gpui::test]
fn failed_registry_changes_keep_previous_settings(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    h.fake.state().autostart_error = Some("denied".into());
    h.fake.state().menu_error = Some("blocked".into());
    h.act(|shelf, _, cx| shelf.toggle_autostart(cx));
    assert_eq!(h.alert(), Some(fill(tr().autostart_change_failed, "denied")));
    h.act(|shelf, _, cx| shelf.toggle_context_menu(cx));
    assert_eq!(h.alert(), Some(fill(tr().context_menu_failed, "blocked")));
    let saved = h.saved();
    assert!(saved.settings().autostart && saved.settings().context_menu);
    assert!(h.read(|shelf| shelf.store.settings().autostart && shelf.store.settings().context_menu));
}

#[gpui::test]
fn pages_toggle_with_shortcuts(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    h.keys("ctrl-k");
    assert!(h.read(|shelf| shelf.page() == Page::Actions));
    h.render();
    h.keys("ctrl-o");
    assert!(h.read(|shelf| shelf.page() == Page::Settings));
    h.render();
    h.keys("ctrl-o");
    assert!(h.read(|shelf| shelf.page() == Page::Shelf));
    h.fake.take_calls();
    h.keys("ctrl-e");
    let content = h.sandbox.storage().content().to_string_lossy().into_owned();
    assert_eq!(h.fake.take_calls(), [Call::ShellOpen(content)]);
    h.act(|shelf, _, _| shelf.open_logs());
    assert!(
        matches!(&h.fake.take_calls()[..], [Call::ShellOpen(path)] if PathBuf::from(path).starts_with(h.sandbox.path()))
    );
}

#[gpui::test]
fn enter_opens_and_ctrl_enter_reveals_targets(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, notes(&["a", "b", "c"]));
    let ids = h.ids();
    h.act(|shelf, _, cx| {
        shelf.toggle(&ids[0], cx);
        shelf.toggle(&ids[2], cx);
    });
    h.fake.take_calls();
    h.keys("enter");
    let mut opened = h.fake.take_calls();
    opened.sort_by_key(|call| format!("{call:?}"));
    let mut expected = vec![Call::Open(ids[0].clone()), Call::Open(ids[2].clone())];
    expected.sort_by_key(|call| format!("{call:?}"));
    assert_eq!(opened, expected);
    h.act(|shelf, _, _| shelf.hover(Hovered::Item(ids[1].clone()), true));
    h.keys("ctrl-enter");
    assert_eq!(h.fake.take_calls(), [Call::Reveal(ids[1].clone())]);
}

#[gpui::test]
fn reset_settings_restores_defaults_everywhere(cx: &mut TestAppContext) {
    let _language = language(Language::Russian);
    let mut h = open_with(cx, true, |_, store, _| {
        store.settings_mut().automatic = false;
        store.settings_mut().shake = ShakeLevel::High;
        store.settings_mut().corner = Some(Corner::TopRight);
        store.settings_mut().pinned = true;
        store.settings_mut().layout = Layout::Grid;
        store.settings_mut().language = Some(Language::Russian);
        store.settings_mut().size = Some((900., 900.));
        store.settings_mut().tray_icon = false;
    });
    h.act(|shelf, _, cx| {
        shelf.add_action(Action::new(actions::Kind::CopyPath), cx);
        shelf.record(Command::Copy, cx);
        shelf.ask_clear(cx);
    });
    h.fake.take_calls();
    h.act(|shelf, _, cx| shelf.reset_settings(cx));
    let calls = h.fake.calls();
    let (width, height) = store::DEFAULT_SIZE;
    for expected in [
        Call::Capture(false),
        Call::Automatic(true),
        Call::Shake(ShakeLevel::Off),
        Call::Corner(None),
        Call::CornerDrag(false),
        Call::Tray(true),
        Call::Autostart(true),
        Call::Bind(Command::Toggle, Command::Toggle.default_shortcut()),
        Call::Resize(width, height),
        Call::ResetPosition,
    ] {
        assert!(calls.contains(&expected), "{expected:?}");
    }
    assert!(calls.iter().any(|call| matches!(call, Call::ContextMenu(true, _))));
    assert!(crate::i18n::current() == Language::system());
    let saved = h.saved();
    assert!(
        saved.settings().automatic
            && saved.settings().shake == ShakeLevel::Off
            && !saved.settings().pinned
            && saved.settings().corner.is_none()
            && saved.settings().tray_icon
    );
    assert!(
        saved.settings().layout == Layout::List
            && saved.settings().language.is_none()
            && saved.settings().size.is_none()
    );
    let tiles = saved_tiles(&h);
    assert!(!tiles.iter().any(|tile| matches!(tile.kind, actions::Kind::CopyPath)));
    assert!(h.read(|shelf| shelf.recorder.command().is_none() && shelf.confirming.is_none()));
}

#[gpui::test]
fn reset_settings_reports_busy_hotkey(cx: &mut TestAppContext) {
    let _language = language(Language::English);
    let mut h = open(cx, true);
    h.fake.state().busy.push(Command::Toggle.default_shortcut());
    h.act(|shelf, _, cx| {
        shelf.toggle_page(Page::Settings, cx);
        shelf.ask_reset(cx);
    });
    h.keys("enter");
    assert_eq!(h.alert(), Some(fill(tr().hotkey_busy, Command::Toggle.default_shortcut().text())));
}

#[gpui::test]
fn language_change_updates_shell_menu_label(cx: &mut TestAppContext) {
    let _language = language(Language::English);
    let mut h = open(cx, true);
    h.act(|shelf, _, cx| shelf.toggle_menu(Dropdown::Language, cx));
    assert!(h.read(|shelf| shelf.menu.open() == Some(Dropdown::Language)));
    h.act(|shelf, _, cx| shelf.set_language(Language::Russian, cx));
    let label = tr().context_menu_verb.to_string();
    assert_eq!(
        h.fake.calls().iter().rfind(|call| matches!(call, Call::ContextMenu(true, _))),
        Some(&Call::ContextMenu(true, label))
    );
    assert!(h.saved().settings().language == Some(Language::Russian));
    assert!(h.read(|shelf| shelf.menu.open() != Some(Dropdown::Language)));
    h.render();
    h.act(|shelf, _, cx| {
        shelf.toggle_menu(Dropdown::Language, cx);
        shelf.dismiss_menu(cx);
        shelf.toggle_menu(Dropdown::Language, cx);
    });
    assert!(h.read(|shelf| shelf.menu.open() != Some(Dropdown::Language)));
    h.act(|shelf, _, cx| shelf.toggle_menu(Dropdown::Language, cx));
    assert!(h.read(|shelf| shelf.menu.open() == Some(Dropdown::Language)));
}
