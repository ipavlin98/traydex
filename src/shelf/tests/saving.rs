use super::*;

fn save_failed(h: &mut Harness) -> bool {
    h.alert().is_some_and(|alert| alert.starts_with(fill(tr().save_failed, "").trim_end()))
}

#[gpui::test]
fn saving_happens_off_the_ui_thread_and_ends_with_the_latest_state(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, notes(&["a", "b"]));
    let ids = h.ids();
    h.pending(|shelf, _, cx| shelf.remove(&ids[0], cx));
    assert_eq!(h.saved().items().len(), 2, "the handler does not write to disk itself");
    h.pending(|shelf, _, cx| shelf.remove(&ids[1], cx));
    h.settle();
    assert!(h.saved().items().is_empty(), "a change made during a write is written after it");
}

#[gpui::test]
fn note_edits_from_a_failed_save_are_written_by_the_next_one(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, notes(&["старый текст"]));
    let id = h.ids().remove(0);
    let path = h.read(|shelf| shelf.store.items()[0].path.clone());
    let blocker = h.sandbox.folder("root/shelf.tmp");
    h.act(|shelf, _, cx| {
        shelf.store.edit_text(&id, "новый текст");
        shelf.save(cx);
    });
    assert!(save_failed(&mut h));
    std::fs::remove_dir(blocker).expect("unblock");
    h.act(|shelf, _, cx| shelf.toggle_blur(cx));
    assert_eq!(h.saved().items()[0].text.as_deref(), Some("новый текст"));
    assert_eq!(std::fs::read_to_string(path).expect("note"), "новый текст");
}

#[gpui::test]
fn quit_waits_for_the_write_in_flight_and_saves_the_rest(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, notes(&["a", "b"]));
    let ids = h.ids();
    h.pending(|shelf, _, cx| shelf.remove(&ids[0], cx));
    h.pending(|shelf, _, cx| {
        shelf.remove(&ids[1], cx);
        shelf.quit(cx);
    });
    assert!(!h.fake.calls().contains(&Call::StopWatch), "the app stays until the writes finish");
    h.settle();
    assert!(h.fake.calls().contains(&Call::StopWatch));
    assert!(h.saved().items().is_empty());
}

#[gpui::test]
fn item_details_come_from_the_latest_refresh(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    h.pending(|shelf, _, cx| shelf.refresh(cx));
    let id = h.pending(|shelf, _, cx| {
        shelf.store.add_text("свежая".into()).expect("text");
        shelf.refresh(cx);
        shelf.store.items()[0].id.clone()
    });
    let jobs: Vec<_> = h.fake.state().jobs.drain(..).collect();
    for job in jobs.into_iter().rev() {
        job();
    }
    h.settle();
    assert!(h.read(|shelf| shelf.catalog.meta(&id).is_some()), "an older refresh finishing last is dropped");
}

#[gpui::test]
fn quit_saves_and_releases_system_resources(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, notes(&["kept"]));
    std::fs::remove_file(h.sandbox.storage().root().join("shelf.json")).ok();
    h.send(Event::Quit);
    let calls = h.fake.calls();
    assert!(calls.contains(&Call::FlushClipboard));
    assert!(calls.contains(&Call::StopWatch));
    assert_eq!(h.saved().items().len(), 1);
}

#[gpui::test]
fn quit_right_after_typing_keeps_the_note(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    h.act(|shelf, window, cx| shelf.new_note(window, cx));
    h.cx.simulate_input("купить молоко");
    h.cx.run_until_parked();
    h.send(Event::Quit);
    let saved = h.saved();
    let note = saved.items().iter().find(|item| item.note()).expect("the note is saved");
    assert_eq!(note.text.as_deref(), Some("купить молоко"));
    assert_eq!(note.title, "купить молоко", "the title saved after typing, not the empty one");
}

#[gpui::test]
fn save_failure_is_reported(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    std::fs::create_dir_all(h.sandbox.storage().root().join("shelf.tmp")).expect("block");
    h.act(|shelf, _, cx| shelf.toggle_blur(cx));
    assert!(h.alert().is_some_and(|alert| alert.starts_with(fill(tr().save_failed, "").trim_end())));
}

#[gpui::test]
fn broken_shelf_file_is_set_aside_and_reported(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, |sandbox, store, _| {
        sandbox.file("root/shelf.json", "{ not json");
        *store = sandbox.store();
    });
    let broken = h.sandbox.storage().root().join("shelf.broken.json");
    assert_eq!(h.alert(), Some(fill(tr().shelf_set_aside, broken.display())));
    assert_eq!(std::fs::read_to_string(&broken).expect("backup"), "{ not json");
}

#[gpui::test]
fn broken_tiles_file_is_reported(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, |sandbox, _, _| {
        sandbox.file("root/actions.json", "{ not json");
    });
    assert!(h.alert().is_some_and(|alert| alert.starts_with(fill(tr().actions_failed, "").trim_end())));
    assert!(h.read(|shelf| shelf.tiles.is_empty()));
    assert!(h.sandbox.storage().root().join("actions.broken.json").exists());
}

#[gpui::test]
fn panicking_handler_restores_the_saved_shelf(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, notes(&["kept"]));
    h.act(|shelf, _, cx| shelf.save(cx));
    h.act(|shelf, _, cx| {
        shelf.guarded(cx, |shelf, _| {
            let all = shelf.store.items().iter().map(|item| item.id.clone()).collect();
            shelf.store.remove(&all);
            shelf.store.settings_mut().pinned = true;
            panic!("broken handler");
        })
    });
    assert_eq!(h.titles(), ["kept"]);
    assert!(!h.read(|shelf| shelf.store.settings().pinned));
    assert_eq!(h.alert().as_deref(), Some(tr().internal_error));
    h.act(|shelf, _, cx| shelf.toggle_blur(cx));
    assert_eq!(h.saved().items().len(), 1);
}

#[gpui::test]
fn panic_while_a_save_is_written_restores_the_state_being_written(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, notes(&["kept"]));
    h.act(|shelf, _, cx| shelf.save(cx));
    h.pending(|shelf, _, cx| {
        shelf.guarded(cx, |shelf, _| {
            shelf.store.add_text("fresh".into()).expect("text");
        })
    });
    h.pending(|shelf, _, cx| shelf.guarded(cx, |_, _| panic!("broken handler")));
    h.settle();
    let mut shown = h.titles();
    shown.sort();
    assert_eq!(shown, ["fresh", "kept"], "the committed note survives the panic");
    assert_eq!(h.saved().items().len(), 2, "the disk and the window agree");
}

#[gpui::test]
fn panic_on_a_note_page_leaves_through_the_usual_cleanup(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    h.act(|shelf, window, cx| shelf.new_note(window, cx));
    assert!(h.read(|shelf| shelf.page() == Page::Preview));
    h.act(|shelf, _, cx| shelf.guarded(cx, |_, _| panic!("broken handler")));
    assert!(h.read(|shelf| shelf.page() == Page::Shelf));
    assert!(h.titles().is_empty(), "the empty note is removed as on a normal exit");
}

#[gpui::test]
fn recovery_keeps_files_of_items_not_yet_saved(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    h.act(|shelf, _, cx| shelf.save(cx));
    let path = h.act(|shelf, _, cx| {
        let mut path = None;
        shelf.guarded(cx, |shelf, _| {
            shelf.store.add_text("unsaved".into()).expect("text");
            path = Some(shelf.store.items()[0].path.clone());
            panic!("broken handler");
        });
        path
    });
    assert!(path.expect("the text was added").exists());
}
