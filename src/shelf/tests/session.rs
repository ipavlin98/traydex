use super::*;

#[gpui::test]
fn near_cursor_opens_the_drag_window_by_the_pointer_and_later_returns_it_home(cx: &mut TestAppContext) {
    let mut h = open_with(cx, false, |_, store, _| {
        store.settings_mut().near_cursor = true;
        store.settings_mut().position = Some((10, 20));
    });
    h.fake.take_calls();
    h.fake.state().buttons = true;
    h.send(Event::DragStart);
    let calls = h.fake.take_calls();
    let near = calls.iter().position(|call| *call == Call::PlaceNearCursor).expect("placed by the cursor");
    let show = calls.iter().position(|call| *call == Call::Show(false)).expect("shown");
    assert!(near < show, "moved before it appears: {calls:?}");
    h.fake.state().buttons = false;
    h.send(Event::DragEnd);
    h.elapse(Duration::from_secs(1));
    assert!(!h.fake.state().visible);
    h.fake.take_calls();
    h.send(Event::Toggle);
    let calls = h.fake.take_calls();
    let home = calls.iter().position(|call| *call == Call::Place(Some((10, 20)))).expect("returned to its place");
    let show = calls.iter().position(|call| *call == Call::Show(true)).expect("shown");
    assert!(home < show, "moved back before it appears: {calls:?}");
    assert_eq!(h.read(|shelf| shelf.store.settings().position), Some((10, 20)), "the saved place is kept");
}

#[gpui::test]
fn near_cursor_is_locked_while_auto_show_is_off_and_kept_for_later(cx: &mut TestAppContext) {
    let mut h = open_with(cx, false, |_, store, _| {
        store.settings_mut().near_cursor = true;
        store.settings_mut().automatic = false;
        store.settings_mut().corner = Some(Corner::TopRight);
        store.settings_mut().corner_drag = true;
    });
    h.act(|shelf, _, cx| shelf.toggle_near_cursor(cx));
    assert!(h.read(|shelf| shelf.store.settings().near_cursor), "a flip while auto-show is off is ignored");
    h.fake.state().buttons = true;
    h.send(Event::DragToCorner);
    assert_eq!(h.count(&Call::Show(false)), 1);
    assert_eq!(h.count(&Call::PlaceNearCursor), 0, "the hot corner opens the window at its own place");
}

#[gpui::test]
fn drag_shows_window_without_focus_and_hides_after_drop_elsewhere(cx: &mut TestAppContext) {
    let mut h = open(cx, false);
    h.fake.state().buttons = true;
    h.send(Event::DragStart);
    assert_eq!(h.count(&Call::Show(false)), 1);
    assert_eq!(h.count(&Call::PlaceNearCursor), 0, "the window opens at its own place unless asked otherwise");
    assert!(h.read(|shelf| shelf.session.dragging()));
    h.elapse(Duration::from_millis(300));
    assert!(h.fake.state().visible);
    h.fake.state().buttons = false;
    h.send(Event::DragEnd);
    assert!(h.read(|shelf| shelf.session.lingering() && !shelf.session.dragging()));
    h.elapse(Duration::from_millis(30));
    assert!(!h.fake.state().visible);
    assert!(h.read(|shelf| !shelf.session.lingering() && !shelf.session.watching()));
}

#[gpui::test]
fn shake_level_is_locked_while_auto_show_is_off_and_kept_for_later(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    h.act(|shelf, _, cx| {
        shelf.set_shake(ShakeLevel::Low, cx);
        shelf.toggle_automatic(cx);
    });
    h.fake.take_calls();
    h.act(|shelf, _, cx| shelf.set_shake(ShakeLevel::High, cx));
    assert_eq!(h.saved().settings().shake, ShakeLevel::Low, "a pick while auto-show is off is ignored");
    assert!(!h.fake.calls().iter().any(|call| matches!(call, Call::Shake(_))));
    h.act(|shelf, _, cx| {
        shelf.toggle_automatic(cx);
        shelf.set_shake(ShakeLevel::High, cx);
    });
    assert_eq!(h.saved().settings().shake, ShakeLevel::High);
    assert_eq!(h.fake.calls().last(), Some(&Call::Shake(ShakeLevel::High)));
}

#[gpui::test]
fn corner_drag_is_locked_only_while_the_window_shows_at_once(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, |_, store, _| {
        store.settings_mut().corner = Some(Corner::TopRight);
        store.settings_mut().corner_drag = true;
    });
    let last = |h: &Harness| h.fake.calls().into_iter().rev().find(|call| matches!(call, Call::CornerDrag(_)));
    assert_eq!(last(&h), Some(Call::CornerDrag(false)), "the window already shows on every drag");
    h.act(|shelf, _, cx| shelf.toggle_corner_drag(cx));
    assert!(h.read(|shelf| shelf.store.settings().corner_drag), "a flip while the window shows at once is ignored");
    h.act(|shelf, _, cx| shelf.toggle_automatic(cx));
    assert_eq!(last(&h), Some(Call::CornerDrag(true)), "without auto-show the kept choice comes back");
    h.act(|shelf, _, cx| shelf.toggle_automatic(cx));
    assert_eq!(last(&h), Some(Call::CornerDrag(false)));
    h.act(|shelf, _, cx| shelf.set_shake(ShakeLevel::Low, cx));
    assert_eq!(last(&h), Some(Call::CornerDrag(true)), "the corner works alongside the shake");
    h.act(|shelf, _, cx| shelf.toggle_corner_drag(cx));
    assert!(!h.saved().settings().corner_drag, "the switch is free while the window waits for a shake");
    assert_eq!(last(&h), Some(Call::CornerDrag(false)));
    h.act(|shelf, _, cx| {
        shelf.toggle_corner_drag(cx);
        shelf.set_shake(ShakeLevel::Off, cx);
    });
    assert_eq!(last(&h), Some(Call::CornerDrag(false)));
}

#[gpui::test]
fn corner_drag_switch_needs_a_hot_corner_and_reaches_the_system(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, |_, store, _| store.settings_mut().automatic = false);
    assert!(!h.saved().settings().corner_drag, "off by default: corners react only to the cursor");
    h.fake.take_calls();
    h.act(|shelf, _, cx| shelf.toggle_corner_drag(cx));
    assert!(!h.read(|shelf| shelf.store.settings().corner_drag), "locked while no corner is chosen");
    assert!(!h.fake.calls().iter().any(|call| matches!(call, Call::CornerDrag(_))));
    h.act(|shelf, _, cx| {
        shelf.set_corner(Some(Corner::TopRight), cx);
        shelf.toggle_corner_drag(cx);
    });
    assert!(h.saved().settings().corner_drag);
    assert_eq!(h.fake.calls().last(), Some(&Call::CornerDrag(true)));
    h.act(|shelf, _, cx| shelf.toggle_corner_drag(cx));
    assert!(!h.saved().settings().corner_drag);
    assert_eq!(h.fake.calls().last(), Some(&Call::CornerDrag(false)));
}

#[gpui::test]
fn drag_into_the_hot_corner_shows_drop_targets_even_without_auto_show(cx: &mut TestAppContext) {
    let mut h = open_with(cx, false, |_, store, _| {
        store.settings_mut().automatic = false;
        store.settings_mut().corner = Some(Corner::BottomRight);
    });
    h.fake.state().buttons = true;
    h.send(Event::DragStart);
    assert!(!h.fake.state().visible, "auto-show is off, so a plain drag keeps the window hidden");
    h.send(Event::DragToCorner);
    assert_eq!(h.count(&Call::Show(false)), 1, "shown without taking focus from the drag source");
    assert!(h.read(|shelf| shelf.dropping()));
    h.fake.state().buttons = false;
    h.send(Event::DragEnd);
    h.elapse(Duration::from_millis(30));
    assert!(!h.fake.state().visible, "a drop elsewhere hides it again, like an automatic show");
}

#[gpui::test]
fn drag_into_the_hot_corner_leaves_an_open_page_for_the_drop_targets(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, |_, store, _| store.settings_mut().automatic = false);
    h.act(|shelf, _, cx| shelf.toggle_page(Page::Settings, cx));
    h.fake.state().buttons = true;
    h.send(Event::DragToCorner);
    assert!(h.fake.state().visible);
    assert!(h.read(|shelf| shelf.page() == Page::Shelf && shelf.dropping()));
}

#[gpui::test]
fn drag_ending_inside_keeps_window_until_pointer_leaves(cx: &mut TestAppContext) {
    let mut h = open(cx, false);
    h.send(Event::DragStart);
    h.fake.state().inside = true;
    h.send(Event::DragEnd);
    assert!(!h.read(|shelf| shelf.session.lingering()));
    h.elapse(Duration::from_secs(2));
    assert!(h.fake.state().visible);
    h.fake.state().inside = false;
    h.elapse(Duration::from_millis(30));
    assert!(!h.fake.state().visible);
}

#[gpui::test]
fn drag_keeps_window_while_buttons_look_released(cx: &mut TestAppContext) {
    let mut h = open(cx, false);
    h.send(Event::DragStart);
    h.elapse(Duration::from_secs(1));
    assert!(h.fake.state().visible, "an elevated drag source hides the held button from Traydex");
    h.send(Event::DragEnd);
    h.elapse(Duration::from_millis(30));
    assert!(!h.fake.state().visible);
}

#[gpui::test]
fn drag_with_auto_show_off_leaves_window_alone(cx: &mut TestAppContext) {
    let mut h = open_with(cx, false, |_, store, _| store.settings_mut().automatic = false);
    h.send(Event::DragStart);
    h.send(Event::DragEnd);
    h.elapse(Duration::from_secs(1));
    assert!(!h.fake.calls().iter().any(|call| matches!(call, Call::Show(_) | Call::Hide)));
}

#[gpui::test]
fn drag_over_visible_window_returns_to_drop_targets(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, notes(&["a"]));
    h.act(|shelf, _, cx| {
        shelf.toggle_page(Page::Settings, cx);
        shelf.ask_clear(cx);
        shelf.toggle_menu(Dropdown::Language, cx);
    });
    h.fake.take_calls();
    h.send(Event::DragStart);
    h.read(|shelf| {
        assert!(shelf.page() == Page::Shelf);
        assert!(shelf.confirming.is_none() && shelf.menu.open() != Some(Dropdown::Language));
        assert!(!shelf.session.automatic() && !shelf.session.watching());
    });
    h.send(Event::DragEnd);
    h.elapse(Duration::from_secs(1));
    assert!(h.fake.calls().is_empty());
}

#[gpui::test]
fn pinning_during_drag_session_keeps_the_window(cx: &mut TestAppContext) {
    let mut h = open(cx, false);
    h.send(Event::DragStart);
    h.act(|shelf, _, cx| shelf.toggle_pinned(cx));
    h.send(Event::DragEnd);
    h.elapse(Duration::from_secs(1));
    assert!(h.fake.state().visible);
    assert!(h.saved().settings().pinned);
}

#[gpui::test]
fn right_click_during_drag_hides_even_a_pinned_window(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, |_, store, _| store.settings_mut().pinned = true);
    h.send(Event::DragStart);
    h.send(Event::CancelDrag);
    assert!(!h.fake.state().visible);
    h.send(Event::DragEnd);
    h.elapse(Duration::from_secs(1));
    assert!(!h.fake.state().visible);
}

#[gpui::test]
fn drop_into_window_ends_the_automatic_session(cx: &mut TestAppContext) {
    let mut h = open(cx, false);
    let file = h.sandbox.file("dropped.txt", "x");
    h.send(Event::DragStart);
    let zone = h.read(|shelf| shelf.regions.zone().expect("zone")).center();
    h.send(Event::Pointer(f32::from(zone.x), f32::from(zone.y)));
    h.send(Event::Drop(Payload::Paths(vec![file.clone()])));
    h.send(Event::DragEnd);
    h.elapse(Duration::from_secs(1));
    assert!(h.fake.state().visible);
    assert_eq!(h.titles(), ["dropped.txt"]);
    assert_eq!(h.saved().items()[0].path, file);
}

#[gpui::test]
fn received_paths_are_added_and_window_revealed(cx: &mut TestAppContext) {
    let mut h = open(cx, false);
    let a = h.sandbox.file("a.txt", "a");
    let folder = h.sandbox.folder("folder");
    h.send(Event::Receive(vec![a.clone(), h.sandbox.join("missing.txt"), folder.clone(), a.clone()]));
    assert_eq!(h.titles(), ["a.txt", "folder"]);
    assert!(h.fake.calls().contains(&Call::Show(true)));
    let saved = h.saved();
    assert!(saved.items().iter().map(|item| item.kind).eq([Kind::File, Kind::Folder]));
    h.send(Event::Receive(vec![a]));
    assert_eq!(h.titles().len(), 2);
}
