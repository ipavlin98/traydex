use super::*;

fn note_area(h: &mut Harness) -> Entity<TextArea> {
    h.read(|shelf| shelf.preview().and_then(|preview| preview.note.as_ref()).expect("note").area.clone())
}

fn focus_note(h: &mut Harness) {
    let area = note_area(h);
    h.act(|_, window, cx| area.update(cx, |input, cx| input.focus(window, cx)));
}

fn shelf_focused(h: &mut Harness) -> bool {
    let shelf = h.shelf.clone();
    h.cx.update(|window, cx| shelf.read(cx).focus.is_focused(window))
}

fn note_title(h: &mut Harness) -> (bool, String) {
    let title = h.read(|shelf| shelf.preview().and_then(|preview| preview.note.as_ref()).expect("note").title.clone());
    h.cx.update(|window, cx| (title.read(cx).focused(window), title.read(cx).text().to_owned()))
}

#[gpui::test]
fn new_note_is_typed_into_and_saved_when_closed(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, notes(&["старая"]));
    h.act(|shelf, window, cx| shelf.new_note(window, cx));
    assert!(h.read(|shelf| shelf.page() == Page::Preview));
    h.cx.simulate_input("Купить хлеб");
    h.keys("enter");
    h.cx.simulate_input("и молоко x");
    h.keys("space delete left right");
    assert!(h.read(|shelf| shelf.page() == Page::Preview && shelf.store.items().len() == 2));
    h.keys("escape");
    assert!(h.read(|shelf| shelf.page() == Page::Shelf && shelf.preview().is_none()));
    assert!(shelf_focused(&mut h));
    let saved = h.saved();
    assert_eq!(saved.items().len(), 2);
    assert_eq!(
        saved.items()[0].text.as_deref(),
        Some(
            "Купить хлеб
и молоко x "
        )
    );
    assert_eq!(saved.items()[0].title, "Купить хлеб и молоко x");
    assert_eq!(
        std::fs::read_to_string(&saved.items()[0].path).expect("note"),
        "Купить хлеб
и молоко x "
    );
    assert_eq!(saved.items()[1].text.as_deref(), Some("старая"));
}

#[gpui::test]
fn empty_shelf_creates_a_note_from_its_button(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    click(&mut h, "empty-note");
    assert!(h.read(|shelf| shelf.page() == Page::Preview && shelf.store.items().len() == 1));
    let area = note_area(&mut h);
    assert!(h.cx.update(|window, cx| area.read(cx).focused(window)));
}

#[gpui::test]
fn blank_note_disappears_when_closed(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    h.keys("ctrl-n");
    assert!(h.read(|shelf| shelf.page() == Page::Preview));
    h.cx.simulate_input("  ");
    h.act(|shelf, _, cx| shelf.leave_preview(cx));
    assert!(h.ids().is_empty());
    assert!(h.saved().items().is_empty());
    h.keys("ctrl-n");
    h.act(|shelf, window, cx| shelf.new_note(window, cx));
    assert_eq!(h.ids().len(), 1);
    h.send(Event::Toggle);
    assert!(h.ids().is_empty());
}

#[gpui::test]
fn previewed_note_keeps_navigation_until_its_text_is_focused(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, notes(&["первая", "вторая"]));
    let ids = h.ids();
    h.act(|shelf, window, cx| shelf.show_preview(ids[0].clone(), window, cx));
    h.keys("right");
    assert_eq!(h.previewed(), Some(ids[1].clone()));
    focus_note(&mut h);
    h.keys("ctrl-end space right left delete backspace");
    h.cx.simulate_input("ы!");
    assert_eq!(h.previewed(), Some(ids[1].clone()));
    assert_eq!(h.read(|shelf| shelf.store.items()[1].text.clone()).as_deref(), Some("втораы!"));
    assert_eq!(h.read(|shelf| shelf.store.items()[1].title.clone()), "втораы!");
    h.elapse(Duration::from_secs(1));
    assert_eq!(h.saved().items()[1].text.as_deref(), Some("втораы!"));
}

#[gpui::test]
fn hiding_the_window_keeps_unsaved_note_edits(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, notes(&["a"]));
    let id = h.ids().remove(0);
    h.act(|shelf, window, cx| shelf.show_preview(id.clone(), window, cx));
    focus_note(&mut h);
    h.cx.simulate_input("1");
    h.send(Event::Toggle);
    assert!(!h.fake.state().visible);
    assert_eq!(h.saved().items()[0].text.as_deref(), Some("1a"));
}

#[gpui::test]
fn note_title_is_edited_in_place_by_a_click_or_f2(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, notes(&["заметка"]));
    let id = h.ids().remove(0);
    h.act(|shelf, window, cx| shelf.show_preview(id.clone(), window, cx));
    click(&mut h, "note-title");
    assert!(note_title(&mut h).0);
    h.keys("ctrl-a");
    h.cx.simulate_input("Список дел");
    h.keys("enter");
    h.elapse(Duration::from_secs(1));
    assert_eq!(h.saved().items()[0].title, "Список дел");
    assert!(h.read(|shelf| shelf.page() == Page::Preview));
    let area = note_area(&mut h);
    assert!(h.cx.update(|window, cx| area.read(cx).focused(window)));
    h.keys("f2");
    h.cx.simulate_input("Дела");
    h.keys("escape");
    assert!(h.read(|shelf| shelf.page() == Page::Preview && shelf.store.items()[0].title == "Дела"));
    assert!(!note_title(&mut h).0);
    h.keys("f2 backspace");
    h.elapse(Duration::from_secs(1));
    assert_eq!(h.saved().items()[0].title, "заметка");
    focus_note(&mut h);
    h.keys("ctrl-end");
    h.cx.simulate_input(" дня");
    assert_eq!(title_shown(&mut h), (String::new(), "заметка дня".to_string()));
    h.keys("escape");
    assert_eq!(h.saved().items()[0].title, "заметка дня");
}

fn hover_title(h: &mut Harness, id: &ItemId) -> bool {
    let mark = format!("item-title:{id}");
    let position = h.cx.debug_bounds(Box::leak(mark.into_boxed_str())).expect("item title").center();
    h.cx.simulate_mouse_move(position, None, Modifiers::default());
    h.elapse(Duration::from_millis(600));
    h.cx.debug_bounds("hint").is_some()
}

#[gpui::test]
fn only_a_cut_off_tile_title_shows_its_full_text_on_hover(cx: &mut TestAppContext) {
    for layout in [Layout::Grid, Layout::List] {
        let mut h = open_with(cx, true, |sandbox, store, _| {
            store.settings_mut().layout = layout;
            store.add_text("Короткое".into()).expect("short");
            store.add_text("длинное".into()).expect("long");
            store.save().expect("save");
            let file = sandbox.storage().root().join("shelf.json");
            let mut saved: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&file).expect("shelf")).expect("json");
            saved["items"][0]["title"] = "Очень длинное название заметки ".repeat(12).into();
            saved["items"][0]["named"] = true.into();
            std::fs::write(&file, saved.to_string()).expect("write");
            *store = sandbox.store();
        });
        let ids = h.ids();
        assert!(!hover_title(&mut h, &ids[1]), "a title that fits has no tooltip");
        assert!(hover_title(&mut h, &ids[0]), "a cut-off title shows a tooltip");
    }
}

#[gpui::test]
fn note_title_selection_follows_the_mouse_past_the_field_edge(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, |_, store, _| {
        store.add_text("заметка".into()).expect("note");
        let id = store.items()[0].id.clone();
        store.rename(&id, "Список покупок");
    });
    let id = h.ids().remove(0);
    h.act(|shelf, window, cx| shelf.show_preview(id.clone(), window, cx));
    let bounds = h.cx.debug_bounds("note-title").expect("note-title");
    let start = point(bounds.left() + px(60.), bounds.center().y);
    let outside = point(bounds.left() - px(30.), bounds.center().y);
    h.cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
    h.cx.simulate_mouse_move(outside, Some(MouseButton::Left), Modifiers::default());
    h.cx.simulate_mouse_up(outside, MouseButton::Left, Modifiers::default());
    h.settle();
    let title = h.read(|shelf| shelf.preview().and_then(|preview| preview.note.as_ref()).expect("note").title.clone());
    let selected = title.update_in(h.cx, |title, window, cx| title.selected_text_range(false, window, cx));
    let selected = selected.expect("selection");
    assert_eq!(selected.range.start, 0);
    assert!(selected.range.end > 0 && selected.reversed, "the selection reaches the start, got {selected:?}");
}

fn title_shown(h: &mut Harness) -> (String, String) {
    let title = h.read(|shelf| shelf.preview().and_then(|preview| preview.note.as_ref()).expect("note").title.clone());
    h.cx.update(|_, cx| (title.read(cx).text().to_owned(), title.read(cx).placeholder().to_owned()))
}

#[gpui::test]
fn automatic_note_title_is_a_placeholder_that_follows_the_text(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, notes(&["Купить хлеб"]));
    let id = h.ids().remove(0);
    h.act(|shelf, window, cx| shelf.show_preview(id.clone(), window, cx));
    assert_eq!(title_shown(&mut h), (String::new(), "Купить хлеб".to_string()));
    focus_note(&mut h);
    h.keys("ctrl-end");
    h.cx.simulate_input(" и молоко");
    assert_eq!(title_shown(&mut h), (String::new(), "Купить хлеб и молоко".to_string()));
    h.keys("escape");
    assert_eq!(h.saved().items()[0].title, "Купить хлеб и молоко");
    h.keys("ctrl-n");
    assert_eq!(title_shown(&mut h), (String::new(), tr().new_note.to_string()));
}

#[gpui::test]
fn tab_switches_focus_between_note_title_and_text(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, notes(&["текст"]));
    let id = h.ids().remove(0);
    h.act(|shelf, window, cx| shelf.show_preview(id.clone(), window, cx));
    focus_note(&mut h);
    h.keys("tab");
    assert!(note_title(&mut h).0);
    h.keys("tab");
    let area = note_area(&mut h);
    assert!(h.cx.update(|window, cx| area.read(cx).focused(window)));
    h.keys("shift-tab");
    assert!(note_title(&mut h).0);
    assert_eq!(h.read(|shelf| shelf.store.items()[0].text.clone()).as_deref(), Some("текст"));
    assert_eq!(note_title(&mut h).1, "");
}

#[gpui::test]
fn notes_have_their_own_tab_and_stay_out_of_all_and_text(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, note_and_texts("заметка", &["текст"]));
    assert_eq!(h.titles(), ["текст"]);
    h.act(|shelf, _, cx| shelf.set_filter(Filter::Text, cx));
    assert_eq!(h.titles(), ["текст"]);
    h.act(|shelf, _, cx| shelf.set_filter(Filter::Notes, cx));
    assert_eq!(h.titles(), ["заметка"]);
    h.act(|shelf, _, cx| shelf.toggle_separate_notes(cx));
    assert!(!h.saved().settings().separate_notes);
    h.act(|shelf, _, cx| shelf.set_filter(Filter::All, cx));
    assert_eq!(h.titles(), ["заметка", "текст"]);
    h.act(|shelf, _, cx| shelf.set_filter(Filter::Text, cx));
    assert_eq!(h.titles(), ["заметка", "текст"]);
}

#[gpui::test]
fn whole_shelf_actions_leave_separate_notes_alone(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, note_and_texts("заметка", &["текст"]));
    let ids = h.ids();
    h.act(|shelf, _, cx| {
        shelf.press_all(&mouse_down(point(px(0.), px(0.)), 1));
        shelf.track(&mouse_move(point(px(0.), px(40.)), Some(MouseButton::Left)), cx);
    });
    assert_eq!(h.fake.calls().last(), Some(&Call::Drag(vec![ids[1].clone()])));
    h.keys("ctrl-c");
    assert_eq!(h.cx.read_from_clipboard().and_then(|item| item.text()).as_deref(), Some("текст"));
    h.keys("ctrl-delete enter");
    assert_eq!(h.saved().items().iter().map(|item| item.id.clone()).collect::<Vec<_>>(), [ids[0].clone()]);
    h.act(|shelf, _, cx| shelf.set_filter(Filter::Notes, cx));
    h.keys("ctrl-delete enter");
    assert!(h.saved().items().is_empty());
}

#[gpui::test]
fn empty_notes_tab_creates_the_first_note(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, notes(&["текст"]));
    h.act(|shelf, _, cx| shelf.set_filter(Filter::Notes, cx));
    click(&mut h, "first-note");
    let key = h.previewed().expect("a new note is open");
    assert!(h.read(|shelf| shelf.store.items().iter().any(|item| item.id == key && item.kind == Kind::Note)));
    let area = note_area(&mut h);
    assert!(h.cx.update(|window, cx| area.read(cx).focused(window)));
}

#[gpui::test]
fn new_note_opens_where_notes_are_shown(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, notes(&["текст"]));
    h.keys("ctrl-n");
    assert!(h.read(|shelf| shelf.filter() == Filter::Notes));
    h.cx.simulate_input("x");
    h.keys("escape");
    h.act(|shelf, _, cx| {
        shelf.toggle_separate_notes(cx);
        shelf.set_filter(Filter::All, cx);
    });
    h.keys("ctrl-n");
    assert!(h.read(|shelf| shelf.filter() == Filter::All));
}

#[gpui::test]
fn files_open_in_a_read_only_preview(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, |sandbox, store, _| {
        store.add_paths(vec![sandbox.file("plan.txt", "original")]);
    });
    let id = h.ids().remove(0);
    h.act(|shelf, window, cx| shelf.show_preview(id.clone(), window, cx));
    assert!(h.read(|shelf| shelf.preview().is_some_and(|preview| preview.note.is_none())));
}
