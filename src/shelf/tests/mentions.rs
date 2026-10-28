use super::*;
use crate::textarea::AreaEvent;

const REPORT: &str = "отчёт за май.pdf";

fn files(names: &'static [&'static str]) -> impl FnOnce(&Sandbox, &mut Store, &Fake) {
    move |sandbox, store, _| {
        store.add_text("купить хлеб".into()).expect("text");
        let note = store.add_note();
        store.edit_text(&note, "старая заметка");
        store.add_paths(names.iter().map(|name| sandbox.file(name, "x")).collect());
    }
}

fn id_of(h: &mut Harness, title: &str) -> ItemId {
    h.read(|shelf| shelf.store.items().iter().find(|item| item.title == title).map(|item| item.id.clone()))
        .expect("the card is on the shelf")
}

fn area(h: &mut Harness) -> Entity<TextArea> {
    h.read(|shelf| shelf.note().expect("a note is open").area.clone())
}

fn cursor(h: &mut Harness) -> usize {
    let area = area(h);
    h.cx.update(|_, cx| area.read(cx).cursor())
}

fn hits(h: &mut Harness) -> Option<Vec<ItemId>> {
    h.read(|shelf| shelf.note().and_then(|note| note.picker.as_ref()).map(|picker| picker.hits.clone()))
}

fn mention_report(h: &mut Harness) -> (ItemId, ItemId) {
    let report = id_of(h, REPORT);
    h.act(|shelf, window, cx| shelf.new_note(window, cx));
    let note = h.previewed().expect("the new note is open");
    h.cx.simulate_input("см. @отч");
    h.settle();
    h.keys("enter");
    (note, report)
}

fn send_area(h: &mut Harness, event: AreaEvent) {
    h.act(|shelf, window, cx| shelf.area_event(&event, window, cx));
}

#[gpui::test]
fn typing_at_offers_matching_cards_and_enter_saves_a_mention(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, files(&["список.txt", REPORT]));
    let report = id_of(&mut h, REPORT);
    h.act(|shelf, window, cx| shelf.new_note(window, cx));
    h.cx.simulate_input("см. @");
    h.settle();
    let offered = hits(&mut h).expect("the list is open");
    let titles: Vec<String> =
        offered.iter().map(|id| h.read(|shelf| shelf.store.item(id).expect("card").title.clone())).collect();
    assert_eq!(titles, ["список.txt", REPORT], "every file is offered, newest first, without notes and texts");
    h.cx.simulate_input("отч");
    h.settle();
    assert_eq!(hits(&mut h).and_then(|hits| hits.first().cloned()), Some(report.clone()));
    h.keys("enter");
    assert_eq!(hits(&mut h), None);
    assert_eq!(h.read(|shelf| shelf.page()), Page::Preview, "Enter picked the card and did not close the note");
    h.keys("escape");
    let saved = h.saved();
    let note = saved.items().first().expect("the note is saved");
    assert_eq!(note.text.as_deref(), Some(format!("см. {} ", store::mention::token(&report, REPORT)).as_str()));
    assert_eq!(note.title, format!("см. @{REPORT}"));
    assert_eq!(h.read(|shelf| shelf.catalog.citing(&report).to_vec()), vec![note.id.clone()]);
}

#[gpui::test]
fn a_mention_opens_its_card_and_escape_comes_back_to_the_same_place(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, files(&[REPORT]));
    let (note, report) = mention_report(&mut h);
    let at = cursor(&mut h);
    send_area(&mut h, AreaEvent::Open { id: report.clone(), reveal: false });
    assert_eq!(h.previewed(), Some(report));
    assert!(h.read(|shelf| shelf.preview().is_some_and(|preview| preview.back.is_some())));
    h.keys("escape");
    assert_eq!(h.previewed(), Some(note));
    assert_eq!(cursor(&mut h), at);
    h.keys("escape");
    assert_eq!(h.read(|shelf| shelf.page()), Page::Shelf);
}

#[gpui::test]
fn ctrl_click_on_a_mention_shows_the_card_on_the_shelf(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, files(&["другое", REPORT]));
    let (_, report) = mention_report(&mut h);
    send_area(&mut h, AreaEvent::Open { id: report, reveal: true });
    assert_eq!(h.read(|shelf| shelf.page()), Page::Shelf);
    assert_eq!(h.selected(), [REPORT]);
}

#[gpui::test]
fn a_mention_of_a_removed_card_says_so_and_keeps_the_note_open(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, files(&[REPORT]));
    let (note, report) = mention_report(&mut h);
    h.act(|shelf, _, cx| {
        shelf.store.remove(&std::collections::HashSet::from([report.clone()]));
        shelf.items_changed(cx);
    });
    send_area(&mut h, AreaEvent::Open { id: report, reveal: false });
    assert_eq!(h.previewed(), Some(note));
    assert_eq!(h.alert().as_deref(), Some(tr().mention_gone));
}

#[gpui::test]
fn removing_a_mentioned_card_asks_first_unless_its_notes_go_too(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, files(&[REPORT]));
    let (note, report) = mention_report(&mut h);
    h.keys("escape");
    h.act(|shelf, _, cx| shelf.remove(&report, cx));
    assert!(h.read(|shelf| matches!(&shelf.confirming, Some(Confirm::Remove(keys)) if keys.contains(&report))));
    h.keys("escape");
    assert!(h.read(|shelf| shelf.store.item(&report).is_some()), "cancelling keeps the card");
    h.act(|shelf, _, cx| {
        shelf.toggle(&report, cx);
        shelf.toggle(&note, cx);
        shelf.remove_selected(cx);
    });
    assert!(h.read(|shelf| shelf.confirming.is_none()), "no note is left with a broken link");
    assert!(h.read(|shelf| shelf.store.item(&report).is_none() && shelf.store.item(&note).is_none()));
}

#[gpui::test]
fn files_pasted_or_dropped_into_a_note_land_on_the_shelf_and_in_the_text(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    let invoice = h.sandbox.file("счёт мой.pdf", "x");
    let photo = h.sandbox.file("фото.png", png(2, 2));
    h.fake.state().pasted.push_back(Ok(Payload::Paths(vec![invoice.clone()])));
    h.act(|shelf, window, cx| shelf.new_note(window, cx));
    h.cx.simulate_input("Счёт:");
    h.keys("ctrl-v");
    h.send(Event::Drop(Payload::Paths(vec![photo.clone()])));
    assert_eq!(h.read(|shelf| shelf.page()), Page::Preview, "the note stays open");
    let invoice_id = h.read(|shelf| shelf.store.items().iter().find(|item| item.path == invoice).map(|i| i.id.clone()));
    let photo_id = h.read(|shelf| shelf.store.items().iter().find(|item| item.path == photo).map(|i| i.id.clone()));
    let (invoice_id, photo_id) = (invoice_id.expect("pasted file"), photo_id.expect("dropped file"));
    let expected = format!(
        "Счёт: {} {} ",
        store::mention::token(&invoice_id, "счёт мой.pdf"),
        store::mention::token(&photo_id, "фото.png")
    );
    let area = area(&mut h);
    assert_eq!(h.cx.update(|_, cx| area.read(cx).text()), expected);
}

#[gpui::test]
fn a_mentioned_card_leads_back_to_its_note(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, files(&[REPORT]));
    let (note, report) = mention_report(&mut h);
    h.keys("escape");
    h.act(|shelf, window, cx| shelf.open_citing(&report, window, cx));
    assert_eq!(h.previewed(), Some(note));
    assert_eq!(cursor(&mut h), "см. ".len() + '\u{E000}'.len_utf8());
}

#[gpui::test]
fn dragging_a_mention_drags_its_card(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, files(&[REPORT]));
    let (_, report) = mention_report(&mut h);
    send_area(&mut h, AreaEvent::Press { id: report.clone(), position: point(px(10.), px(10.)) });
    h.act(|shelf, _, cx| shelf.track(&mouse_move(point(px(10.), px(80.)), Some(MouseButton::Left)), cx));
    h.settle();
    assert_eq!(h.count(&Call::Drag(vec![report])), 1);
}

#[gpui::test]
fn the_list_of_cards_appears_under_the_at_sign(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, files(&["другое", REPORT]));
    h.act(|shelf, window, cx| shelf.new_note(window, cx));
    h.cx.simulate_input("см. @");
    h.settle();
    h.cx.run_until_parked();
    let area = area(&mut h);
    let anchor = h.cx.update(|_, cx| area.read(cx).anchor());
    assert!(anchor.is_some(), "the @ is laid out");
    let menu = h.cx.debug_bounds("mention-menu").expect("the list is shown");
    let rows = (0..2).filter(|index| h.cx.debug_bounds(interned(&format!("mention-pick:{index}"))).is_some()).count();
    assert_eq!(rows, 2);
    let note = h.cx.debug_bounds("note").expect("note");
    assert!(menu.top() > note.top() && menu.left() >= note.left(), "{menu:?} inside {note:?}");
}

#[gpui::test]
fn hovering_a_mention_shows_its_card_after_a_short_pause(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, files(&[REPORT]));
    let (_, report) = mention_report(&mut h);
    let bounds = gpui::Bounds::new(point(px(40.), px(60.)), size(px(80.), px(16.)));
    let shown = |h: &mut Harness| h.read(|shelf| shelf.note().and_then(|note| note.peek.clone()).map(|peek| peek.id));
    send_area(&mut h, AreaEvent::Hover(Some(crate::textarea::Peek { id: report.clone(), bounds })));
    assert_eq!(shown(&mut h), None, "a passing cursor shows nothing");
    h.elapse(Duration::from_millis(400));
    assert_eq!(shown(&mut h), Some(report));
    h.cx.run_until_parked();
    let card = h.cx.debug_bounds("mention-peek").expect("the card is shown");
    assert_eq!(card.left(), bounds.left(), "the card starts exactly at the mention's left edge");
    assert!(card.top() > bounds.bottom(), "{card:?} opens below {bounds:?}");
    send_area(&mut h, AreaEvent::Hover(None));
    assert_eq!(shown(&mut h), None);
}

#[gpui::test]
fn a_long_list_scrolls_to_the_chosen_card(cx: &mut TestAppContext) {
    const MANY: &[&str] = &["a1", "a2", "a3", "a4", "a5", "a6", "a7", "a8", "a9", "a10", "a11", "a12"];
    let mut h = open_with(cx, true, files(MANY));
    h.act(|shelf, window, cx| shelf.new_note(window, cx));
    h.cx.simulate_input("@");
    h.settle();
    h.cx.run_until_parked();
    let list = h.cx.debug_bounds("mention-list").expect("the list is shown");
    let window = h.cx.update(|window, _| window.viewport_size());
    assert!(list.size.height < window.height / 2., "{list:?} shows a few rows, not all twelve");
    h.keys("down down down down down down down down down down");
    h.cx.run_until_parked();
    let chosen = h.cx.debug_bounds("mention-pick:10").expect("the chosen row");
    let list = h.cx.debug_bounds("mention-list").expect("the list is shown");
    assert!(chosen.top() >= list.top() && chosen.bottom() <= list.bottom(), "{chosen:?} is scrolled into {list:?}");
}

#[gpui::test]
fn the_preview_of_a_mentioned_card_links_to_its_note(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, files(&[REPORT]));
    let (note, report) = mention_report(&mut h);
    h.keys("escape");
    h.act(|shelf, window, cx| shelf.show_preview(report, window, cx));
    h.cx.run_until_parked();
    click(&mut h, "preview-cited");
    assert_eq!(h.previewed(), Some(note));
}

#[gpui::test]
fn the_card_under_the_mouse_becomes_the_only_chosen_one(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, files(&["список.txt", REPORT]));
    h.act(|shelf, window, cx| shelf.new_note(window, cx));
    h.cx.simulate_input("@");
    h.settle();
    let chosen = |h: &mut Harness| h.read(|shelf| shelf.note().and_then(|note| note.picker.as_ref()).map(|p| p.chosen));
    assert_eq!(chosen(&mut h), Some(0));
    h.act(|shelf, _, cx| shelf.point_pick(1, cx));
    assert_eq!(chosen(&mut h), Some(1));
    h.keys("enter");
    let area = area(&mut h);
    let report = id_of(&mut h, REPORT);
    assert_eq!(h.cx.update(|_, cx| area.read(cx).text()), format!("{} ", store::mention::token(&report, REPORT)));
}

#[gpui::test]
fn the_wheel_turns_the_preview_link_through_every_note_that_mentions_the_card(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, files(&[REPORT]));
    let (first, report) = mention_report(&mut h);
    h.keys("escape");
    let (second, _) = mention_report(&mut h);
    h.keys("escape");
    h.act(|shelf, window, cx| shelf.show_preview(report.clone(), window, cx));
    h.cx.run_until_parked();
    let bar = h.center("preview-cited");
    h.cx.simulate_event(gpui::ScrollWheelEvent {
        position: bar,
        delta: gpui::ScrollDelta::Lines(point(0., -1.)),
        ..Default::default()
    });
    h.settle();
    click(&mut h, "preview-cited");
    assert_eq!(h.previewed(), Some(first), "the newest note comes first, one notch down shows the older one");
    h.act(|shelf, window, cx| shelf.show_preview(report.clone(), window, cx));
    h.cx.run_until_parked();
    click(&mut h, "preview-cited");
    assert_eq!(h.previewed(), Some(second), "opening the preview again starts from the newest note");
}
