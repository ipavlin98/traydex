use super::*;

#[gpui::test]
fn startup_configures_the_system_and_shows_the_window(cx: &mut TestAppContext) {
    let h = open(cx, true);
    let calls = h.fake.calls();
    assert_eq!(calls[..3], [Call::Automatic(true), Call::Shake(ShakeLevel::Off), Call::Corner(None)]);
    let (width, height) = store::DEFAULT_SIZE;
    for expected in [
        Call::Prepare(width, height),
        Call::Place(None),
        Call::Show(true),
        Call::Autostart(true),
        Call::ContextMenu(true, tr().context_menu_verb.into()),
    ] {
        assert_eq!(calls.iter().filter(|call| **call == expected).count(), 1, "{expected:?}");
    }
    let prepare = calls.iter().position(|call| *call == Call::Prepare(width, height));
    let show = calls.iter().position(|call| *call == Call::Show(true));
    assert!(prepare < show);
    assert!(calls.iter().any(|call| matches!(call, Call::MinSize(..))));
    assert!(h.fake.state().visible);
}

#[gpui::test]
fn hidden_start_restores_geometry_without_showing(cx: &mut TestAppContext) {
    let h = open_with(cx, false, |_, store, _| {
        store.settings_mut().position = Some((10, 20));
        store.settings_mut().size = Some((700., 650.));
        store.settings_mut().context_menu = false;
        store.settings_mut().automatic = false;
        store.settings_mut().corner = Some(Corner::TopLeft);
        store.settings_mut().corner_drag = true;
        store.settings_mut().tray_icon = false;
    });
    let calls = h.fake.calls();
    assert!(calls.contains(&Call::Prepare(700., 650.)));
    assert!(calls.contains(&Call::CornerDrag(true)));
    assert!(calls.contains(&Call::Place(Some((10, 20)))));
    assert!(calls.contains(&Call::Automatic(false)));
    assert!(calls.contains(&Call::Corner(Some(Corner::TopLeft))));
    assert!(calls.contains(&Call::Tray(false)));
    assert!(!calls.iter().any(|call| matches!(call, Call::Show(_) | Call::ContextMenu(..))));
}

#[gpui::test]
fn inactive_start_shows_the_sized_window_without_taking_focus(cx: &mut TestAppContext) {
    let h = launch_with(cx, Launch::Inactive, |_, store, _| store.settings_mut().size = Some((600., 640.)));
    let calls = h.fake.calls();
    let prepare = calls.iter().position(|call| *call == Call::Prepare(600., 640.));
    let show = calls.iter().position(|call| *call == Call::Show(false));
    assert!(prepare.is_some() && prepare < show, "the window is sized before it is shown: {calls:?}");
    assert!(!calls.contains(&Call::Show(true)));
    assert!(h.fake.state().visible);
}

#[gpui::test]
fn startup_registration_failures_are_reported(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, |_, _, fake| fake.state().menu_error = Some("menu denied".into()));
    assert_eq!(h.alert(), Some(fill(tr().context_menu_failed, "menu denied")));
    drop(h);
    let mut h = open_with(cx, true, |_, _, fake| fake.state().autostart_error = Some("registry denied".into()));
    assert_eq!(h.alert(), Some(fill(tr().autostart_setup_failed, "registry denied")));
}

#[gpui::test]
fn alerts_disappear_after_ten_seconds_from_the_latest_failure(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    h.send(Event::Failure("first".into()));
    assert_eq!(h.alert().as_deref(), Some("first"));
    h.elapse(Duration::from_secs(6));
    h.send(Event::Failure("second".into()));
    h.elapse(Duration::from_millis(4100));
    assert_eq!(h.alert().as_deref(), Some("second"));
    h.elapse(Duration::from_millis(5800));
    assert_eq!(h.alert().as_deref(), Some("second"));
    h.elapse(Duration::from_millis(200));
    assert_eq!(h.alert(), None);
}

#[gpui::test]
fn alert_can_be_dismissed_early(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    h.send(Event::Failure("boom".into()));
    h.act(|shelf, _, cx| shelf.dismiss_notice(crate::shelf::Notice::Alert, cx));
    assert_eq!(h.alert(), None);
    h.elapse(Duration::from_secs(11));
    assert_eq!(h.alert(), None);
}

#[gpui::test]
fn toggle_hides_and_shows_the_window(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    h.fake.take_calls();
    h.send(Event::Toggle);
    assert_eq!(h.fake.take_calls(), [Call::Hide]);
    h.send(Event::Hotkey(Command::Toggle));
    assert!(h.fake.take_calls().contains(&Call::Show(true)));
    h.send(Event::Corner);
    assert_eq!(h.fake.take_calls(), [Call::Hide]);
    h.send(Event::Hotkey(Command::Pin));
    assert!(h.fake.take_calls().is_empty());
}

fn animated(_: &Sandbox, store: &mut Store, _: &Fake) {
    store.settings_mut().window_animations = true;
    store.settings_mut().interface_animations = true;
}

#[gpui::test]
fn animated_hide_keeps_the_page_until_the_window_fades_out(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, animated);
    h.act(|shelf, _, cx| shelf.toggle_page(Page::Settings, cx));
    h.send(Event::Toggle);
    assert!(h.fake.state().visible);
    assert!(h.read(|shelf| shelf.page() == Page::Settings));
    h.elapse(Duration::from_millis(300));
    assert!(!h.fake.state().visible);
    assert!(h.read(|shelf| shelf.page() == Page::Shelf));
}

#[gpui::test]
fn toggle_during_fade_out_brings_the_window_back(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, animated);
    h.send(Event::Toggle);
    h.elapse(Duration::from_millis(50));
    h.send(Event::Toggle);
    h.elapse(Duration::from_secs(1));
    assert!(h.fake.state().visible);
    assert!(h.read_cx(|shelf, cx| shelf.fade.read(cx).level(cx.background_executor().now()) == 1.));
}

#[gpui::test]
fn drag_during_fade_out_keeps_the_window(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, animated);
    h.send(Event::Toggle);
    h.elapse(Duration::from_millis(50));
    h.fake.state().buttons = true;
    h.send(Event::DragStart);
    h.elapse(Duration::from_secs(1));
    assert!(h.fake.state().visible);
}

#[gpui::test]
fn animated_window_stays_transparent_until_first_shown(cx: &mut TestAppContext) {
    let mut h = open_with(cx, false, animated);
    assert!(h.read_cx(|shelf, cx| shelf.fade.read(cx).level(cx.background_executor().now()) == 0.));
    h.send(Event::Reveal);
    h.elapse(Duration::from_secs(1));
    assert!(h.read_cx(|shelf, cx| shelf.fade.read(cx).level(cx.background_executor().now()) == 1.));
}

#[gpui::test]
fn animated_window_fades_in_noticeably_but_briefly(cx: &mut TestAppContext) {
    let mut h = open_with(cx, false, animated);
    h.send(Event::Reveal);
    let level = |h: &mut Harness| h.read_cx(|shelf, cx| shelf.fade.read(cx).level(cx.background_executor().now()));
    h.elapse(Duration::from_millis(50));
    let early = level(&mut h);
    assert!(early > 0. && early < 0.5, "50 ms in, the window is still visibly fading in: {early}");
    h.elapse(Duration::from_millis(250));
    assert_eq!(level(&mut h), 1., "the fade is over within 300 ms");
}

#[gpui::test]
fn window_animation_turns_blur_off_and_keeps_it_locked(cx: &mut TestAppContext) {
    let mut h = open_with(cx, false, |sandbox, store, fake| {
        animated(sandbox, store, fake);
        store.settings_mut().transparency = 20;
        store.settings_mut().blur = true;
    });
    let blurred = |h: &mut Harness| {
        h.render();
        h.read(|shelf| shelf.backdrop == WindowBackgroundAppearance::LightBlurred)
    };
    h.send(Event::Reveal);
    h.elapse(Duration::from_millis(50));
    let early = h.read_cx(|shelf, cx| shelf.fade.read(cx).level(cx.background_executor().now()));
    assert!(early > 0. && early < 1., "a window saved with blur still fades in: {early}");
    h.elapse(Duration::from_secs(1));
    assert!(!blurred(&mut h));
    h.act(|shelf, _, cx| shelf.toggle_blur(cx));
    assert!(!blurred(&mut h), "the blur switch is locked while the window animates");
    assert!(h.read(|shelf| shelf.store.settings().blur), "the locked switch keeps the saved choice");
    h.act(|shelf, _, cx| shelf.toggle_window_animations(cx));
    assert!(blurred(&mut h), "without window animation the saved blur comes back");
    h.act(|shelf, _, cx| shelf.toggle_blur(cx));
    assert!(!blurred(&mut h));
    assert!(!h.saved().settings().blur);
}

#[gpui::test]
fn cancelled_drag_keeps_drop_targets_while_the_window_fades_out(cx: &mut TestAppContext) {
    let mut h = open_with(cx, false, animated);
    h.fake.state().buttons = true;
    h.send(Event::DragStart);
    h.elapse(Duration::from_millis(300));
    assert!(h.read(|shelf| shelf.dropping()));
    h.send(Event::CancelDrag);
    h.fake.state().buttons = false;
    h.send(Event::DragEnd);
    assert!(h.fake.state().visible);
    assert!(h.read(|shelf| shelf.dropping()));
    h.elapse(Duration::from_millis(300));
    assert!(!h.fake.state().visible);
    assert!(h.read(|shelf| !shelf.dropping()));
}

#[gpui::test]
fn reveal_during_fade_out_shows_the_current_state(cx: &mut TestAppContext) {
    let mut h = open_with(cx, false, animated);
    h.fake.state().buttons = true;
    h.send(Event::DragStart);
    h.elapse(Duration::from_millis(300));
    h.send(Event::CancelDrag);
    h.fake.state().buttons = false;
    h.send(Event::DragEnd);
    h.elapse(Duration::from_millis(50));
    h.send(Event::Reveal);
    h.elapse(Duration::from_secs(1));
    assert!(h.fake.state().visible);
    assert!(h.read(|shelf| !shelf.dropping()));
}

#[gpui::test]
fn interface_animations_switch_controls_hover_transitions(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, animated);
    assert!(h.cx.update(|_, cx| cx.global::<HoverTransition>().duration.is_some()));
    h.act(|shelf, _, cx| shelf.toggle_window_animations(cx));
    assert!(h.cx.update(|_, cx| cx.global::<HoverTransition>().duration.is_some()));
    h.act(|shelf, _, cx| shelf.toggle_interface_animations(cx));
    assert!(h.cx.update(|_, cx| cx.global::<HoverTransition>().duration.is_none()));
}

#[gpui::test]
fn interface_animations_alone_hide_the_window_at_once(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, |_, store, _| store.settings_mut().interface_animations = true);
    h.send(Event::Toggle);
    assert!(!h.fake.state().visible);
}

#[gpui::test]
fn toggle_right_after_focus_loss_does_not_reopen(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    h.activate();
    h.deactivate();
    assert!(!h.fake.state().visible);
    h.fake.take_calls();
    h.send(Event::Toggle);
    assert!(!h.fake.calls().contains(&Call::Show(true)));
    h.send(Event::Corner);
    assert!(h.fake.take_calls().contains(&Call::Show(true)));
    h.activate();
    h.deactivate();
    h.elapse(Duration::from_millis(500));
    h.fake.take_calls();
    h.send(Event::Toggle);
    assert!(h.fake.calls().contains(&Call::Show(true)));
}

#[gpui::test]
fn focus_loss_keeps_pinned_picking_or_exporting_window(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, |_, store, _| store.settings_mut().pinned = true);
    h.activate();
    h.deactivate();
    assert!(h.fake.state().visible);
    h.act(|shelf, _, cx| shelf.toggle_pinned(cx));
    h.act(|shelf, _, _| shelf.session.set_picking(true));
    h.activate();
    h.deactivate();
    assert!(h.fake.state().visible);
    h.act(|shelf, _, _| shelf.session.set_picking(false));
    h.fake.state().exporting = true;
    h.activate();
    h.deactivate();
    assert!(h.fake.state().visible);
    assert_eq!(h.count(&Call::Hide), 0);
}

#[gpui::test]
fn outside_press_hides_only_after_buttons_are_released(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    h.fake.state().buttons = true;
    h.send(Event::PressOutside);
    h.elapse(Duration::from_millis(300));
    assert!(h.fake.state().visible);
    h.fake.state().buttons = false;
    h.fake.state().inside = true;
    h.elapse(Duration::from_millis(300));
    assert!(h.fake.state().visible);
    h.fake.state().inside = false;
    h.elapse(Duration::from_millis(30));
    assert!(!h.fake.state().visible);
    assert_eq!(h.count(&Call::Hide), 1);
    h.elapse(Duration::from_secs(1));
    assert_eq!(h.count(&Call::Hide), 1);
}

#[gpui::test]
fn outside_press_on_hidden_window_does_nothing(cx: &mut TestAppContext) {
    let mut h = open(cx, false);
    h.send(Event::PressOutside);
    h.elapse(Duration::from_secs(1));
    assert_eq!(h.count(&Call::Hide), 0);
}

#[gpui::test]
fn placement_and_size_reset_are_persisted(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    h.send(Event::Placed { x: -100, y: 50, width: 800., height: 600. });
    let saved = h.saved();
    assert_eq!((saved.settings().position, saved.settings().size), (Some((-100, 50)), Some((800., 600.))));
    h.fake.take_calls();
    h.send(Event::ResetSize);
    let (width, height) = store::DEFAULT_SIZE;
    assert_eq!(h.fake.take_calls(), [Call::Resize(width, height)]);
    assert_eq!(h.saved().settings().size, None);
    assert_eq!(h.saved().settings().position, Some((-100, 50)));
    h.act(|shelf, _, cx| shelf.reset_position(cx));
    assert_eq!(h.fake.take_calls(), [Call::ResetPosition]);
    assert_eq!(h.saved().settings().position, None);
}
