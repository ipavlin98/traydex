use super::*;

#[gpui::test]
fn escape_unwinds_one_layer_at_a_time(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, notes(&["a", "b"]));
    h.act(|shelf, window, cx| {
        let first = shelf.store.items()[0].id.clone();
        shelf.toggle(&first, cx);
        shelf.add_or_edit(Action::new(actions::Kind::Zip), None, window, cx);
        shelf.ask_clear(cx);
        shelf.toggle_menu(Dropdown::Language, cx);
    });
    let page = |h: &mut Harness| {
        h.read(|shelf| (shelf.page(), shelf.confirming.clone(), shelf.menu.open() == Some(Dropdown::Language)))
    };
    assert!(page(&mut h) == (Page::Edit, Some(Confirm::Clear), true));
    h.keys("escape");
    assert!(page(&mut h) == (Page::Edit, Some(Confirm::Clear), false));
    h.keys("escape");
    assert!(page(&mut h) == (Page::Edit, None, false));
    h.keys("escape");
    assert!(page(&mut h) == (Page::Actions, None, false));
    h.keys("escape");
    assert!(page(&mut h) == (Page::Shelf, None, false));
    assert_eq!(h.selected(), ["a"]);
    h.keys("escape");
    assert!(h.selected().is_empty());
    assert!(h.fake.state().visible);
    h.keys("escape");
    assert!(!h.fake.state().visible);
    assert_eq!(h.titles(), ["a", "b"]);
}

#[gpui::test]
fn clearing_needs_confirmation(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, notes(&["a", "b"]));
    h.keys("ctrl-delete");
    assert!(h.read(|shelf| shelf.confirming == Some(Confirm::Clear)));
    assert_eq!(h.titles().len(), 2);
    h.keys("enter");
    assert!(h.titles().is_empty());
    assert!(h.saved().items().is_empty());
    assert!(h.read(|shelf| shelf.confirming.is_none()));
    h.keys("ctrl-delete");
    assert!(h.read(|shelf| shelf.confirming.is_none()));
}

#[gpui::test]
fn p_pins_the_hovered_item_on_top_and_clearing_keeps_it(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, notes(&["a", "b", "c"]));
    h.keys("down down down p");
    assert_eq!(h.titles(), ["c", "a", "b"]);
    assert!(h.saved().items()[0].pinned);
    h.keys("ctrl-delete enter");
    assert_eq!(h.titles(), ["c"]);
    assert_eq!(h.saved().items().len(), 1);
    h.keys("down p");
    assert!(!h.saved().items()[0].pinned);
}

#[gpui::test]
fn default_window_fits_three_tiles_in_a_row(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, |_, store, _| {
        store.settings_mut().layout = Layout::Grid;
        for text in ["a", "b", "c", "d"] {
            store.add_text(text.into()).expect("text");
        }
    });
    let (width, height) = store::DEFAULT_SIZE;
    h.cx.simulate_resize(size(px(width), px(height)));
    h.render();
    assert_eq!(h.read(|shelf| shelf.rows.get().columns), 3);
}

#[gpui::test]
fn selection_supports_toggle_ranges_and_select_all(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, notes(&["a", "b", "c", "d"]));
    let ids = h.ids();
    let shift = Modifiers { shift: true, ..Default::default() };
    h.act(|shelf, _, cx| shelf.select(&ids[1], shift, cx));
    assert_eq!(h.selected(), ["b"]);
    h.act(|shelf, _, cx| shelf.select(&ids[3], shift, cx));
    assert_eq!(h.selected(), ["b", "c", "d"]);
    h.act(|shelf, _, cx| shelf.select(&ids[0], shift, cx));
    assert_eq!(h.selected(), ["a", "b", "c", "d"]);
    h.act(|shelf, _, cx| shelf.select(&ids[2], Modifiers::default(), cx));
    assert_eq!(h.selected(), ["a", "b", "d"]);
    h.act(|shelf, _, _| shelf.selection.clear());
    h.keys("ctrl-a");
    assert_eq!(h.selected(), ["a", "b", "c", "d"]);
}

#[gpui::test]
fn ctrl_click_on_an_item_reveals_it_instead_of_selecting(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, notes(&["a", "b"]));
    let ids = h.ids();
    h.fake.take_calls();
    let control = Modifiers { control: true, ..Default::default() };
    h.act(|shelf, _, cx| shelf.select(&ids[1], control, cx));
    assert_eq!(h.fake.take_calls(), [Call::Reveal(ids[1].clone())]);
    assert!(h.selected().is_empty());
}

#[gpui::test]
fn middle_click_on_an_item_reveals_it(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, notes(&["a", "b"]));
    let ids = h.ids();
    h.fake.take_calls();
    let position = h.center(&format!("item:{}", ids[1]));
    h.cx.simulate_mouse_down(position, MouseButton::Middle, Modifiers::default());
    h.cx.simulate_mouse_up(position, MouseButton::Middle, Modifiers::default());
    h.settle();
    assert_eq!(h.fake.take_calls(), [Call::Reveal(ids[1].clone())]);
    assert!(h.selected().is_empty());
}

#[gpui::test]
fn filters_limit_visible_items_and_reset_selection(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, |sandbox, store, _| {
        store.add_paths(vec![sandbox.file("doc.txt", "x"), sandbox.file("pic.png", png(2, 2)), sandbox.folder("dir")]);
        store.add_text("https://example.com".into()).expect("link");
        store.add_text("note".into()).expect("note");
    });
    h.keys("ctrl-a");
    assert_eq!(h.selected().len(), 5);
    h.act(|shelf, _, cx| shelf.set_filter(Filter::Files, cx));
    assert!(h.selected().is_empty());
    assert_eq!(h.titles(), ["doc.txt", "dir"]);
    h.act(|shelf, _, cx| shelf.set_filter(Filter::Images, cx));
    assert_eq!(h.titles(), ["pic.png"]);
    h.act(|shelf, _, cx| shelf.set_filter(Filter::Text, cx));
    assert_eq!(h.titles(), ["note", "https://example.com"]);
    h.keys("ctrl-a");
    assert_eq!(h.selected(), ["https://example.com", "note"]);
    h.act(|shelf, _, cx| shelf.set_filter(Filter::Text, cx));
    assert_eq!(h.selected().len(), 2);
    let ids = h.ids();
    let shift = Modifiers { shift: true, ..Default::default() };
    h.act(|shelf, _, cx| {
        shelf.selection.clear();
        shelf.select(&ids[0], Modifiers::default(), cx);
        shelf.select(&ids[0], Modifiers::default(), cx);
        shelf.select(&ids[1], shift, cx);
    });
    assert_eq!(h.selected(), ["https://example.com", "note"]);
}

#[gpui::test]
fn search_filters_items_and_typo_fallback_applies(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, notes(&["hello world", "another note", "apple pie"]));
    h.keys("ctrl-a");
    h.keys("/");
    assert!(h.searching());
    h.cx.simulate_input("note");
    h.settle();
    assert_eq!(h.titles(), ["another note"]);
    assert!(h.selected().is_empty());
    h.keys("escape");
    assert_eq!(h.titles().len(), 3);
    h.cx.simulate_input("helo");
    h.settle();
    assert_eq!(h.titles(), ["hello world"]);
    h.keys("escape escape");
    assert!(!h.searching());
    assert!(h.fake.state().visible);
}

#[gpui::test]
fn search_prefers_word_starts_and_falls_back_to_fragments(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, notes(&["Договор аренды", "В пути"]));
    h.keys("/");
    h.cx.simulate_input("в");
    h.settle();
    assert_eq!(h.titles(), ["В пути"]);
    h.keys("escape");
    h.cx.simulate_input("овор");
    h.settle();
    assert_eq!(h.titles(), ["Договор аренды"]);
}

fn every_kind(sandbox: &Sandbox, store: &mut Store, _: &Fake) {
    store.add_paths(vec![sandbox.file("бюджет.txt", "x"), sandbox.file("схема.png", png(2, 2))]);
    store.add_text("список дел".into()).expect("text");
    let key = store.add_note();
    store.edit_text(&key, "бюджет на год");
}

fn sorted_titles(h: &mut Harness) -> Vec<String> {
    let mut titles = h.titles();
    titles.sort();
    titles
}

fn search_tab_shown(h: &mut Harness) -> bool {
    h.read(|shelf| shelf.shown_tabs().any(|(_, filter)| filter == Filter::Found))
}

#[gpui::test]
fn search_opens_a_search_tab_with_every_match_and_closes_it_when_cleared(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, every_kind);
    h.act(|shelf, _, cx| shelf.set_filter(Filter::Images, cx));
    assert!(!search_tab_shown(&mut h));
    h.keys("/");
    h.cx.simulate_input("бюджет");
    h.settle();
    assert!(search_tab_shown(&mut h));
    assert!(h.read(|shelf| shelf.filter()) == Filter::Found);
    assert_eq!(sorted_titles(&mut h), ["бюджет на год", "бюджет.txt"]);
    h.keys("escape");
    assert!(!search_tab_shown(&mut h));
    assert!(h.read(|shelf| shelf.filter()) == Filter::Images);
    assert_eq!(h.titles(), ["схема.png"]);
}

#[gpui::test]
fn a_tab_picked_during_search_stays_until_the_next_search(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, every_kind);
    h.keys("/");
    h.cx.simulate_input("бюджет");
    h.settle();
    h.act(|shelf, _, cx| shelf.set_filter(Filter::Notes, cx));
    h.cx.simulate_input(" на");
    h.settle();
    assert!(h.read(|shelf| shelf.filter()) == Filter::Notes);
    assert_eq!(h.titles(), ["бюджет на год"]);
    h.keys("escape");
    assert!(h.read(|shelf| shelf.filter()) == Filter::Notes);
    h.cx.simulate_input("список");
    h.settle();
    assert!(h.read(|shelf| shelf.filter()) == Filter::Found);
    assert_eq!(h.titles(), ["список дел"]);
}

fn tab_buttons(h: &mut Harness) -> (bool, bool) {
    (h.cx.debug_bounds("tabs-start").is_some(), h.cx.debug_bounds("tabs-end").is_some())
}

fn page_tabs(h: &mut Harness, button: &'static str) {
    let mut clicks = 0;
    while h.cx.debug_bounds(button).is_some() {
        clicks += 1;
        assert!(clicks <= Filter::ALL.len(), "{button} keeps the tabs moving");
        click(h, button);
        h.render();
    }
}

#[gpui::test]
fn tabs_that_do_not_fit_get_scroll_buttons_on_the_hidden_sides(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, notes(&["a"]));
    h.cx.simulate_resize(size(px(1200.), px(NARROW_SIZE.1)));
    h.render();
    assert_eq!(tab_buttons(&mut h), (false, false), "tabs that fit have no scroll buttons");
    h.cx.simulate_resize(size(px(NARROW_SIZE.0), px(NARROW_SIZE.1)));
    h.render();
    assert_eq!(tab_buttons(&mut h), (false, true), "tabs hidden on the right get a button");
    page_tabs(&mut h, "tabs-end");
    assert_eq!(tab_buttons(&mut h), (true, false), "the end of the row brings back the left button");
    let (area, last) = (h.cx.debug_bounds("tab-area").expect("tab area"), h.cx.debug_bounds("tab-4").expect("tab"));
    assert!(last.right() <= area.right(), "the last tab is shown whole: {last:?} in {area:?}");
    page_tabs(&mut h, "tabs-start");
    assert_eq!(tab_buttons(&mut h), (false, true));
    assert!(h.read(|shelf| shelf.filter()) == Filter::All, "a scroll button does not pick the tab under it");
}

#[gpui::test]
fn search_finds_files_by_content_and_follows_their_changes(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, |sandbox, store, _| {
        store.add_paths(vec![sandbox.file("report.txt", "Квартальная выручка"), sandbox.file("plan.txt", "Расходы")]);
    });
    h.keys("/");
    h.cx.simulate_input("выручка");
    h.settle();
    assert_eq!(h.titles(), ["report.txt"]);
    h.sandbox.file("report.txt", "Только расходы");
    h.act(|shelf, _, cx| shelf.refresh(cx));
    assert!(h.titles().is_empty());
}

#[gpui::test]
fn search_is_unavailable_on_empty_shelf_and_other_pages_search_their_own_field(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    h.keys("/");
    assert!(!h.searching());
    drop(h);
    let mut h = open_with(cx, true, notes(&["a"]));
    h.keys("ctrl-k");
    h.keys("/");
    assert!(!h.searching());
    let shelf = h.shelf.clone();
    assert!(h.cx.update(|window, cx| shelf.read(cx).find.input().read(cx).focused(window)));
}

#[gpui::test]
fn arrows_move_keyboard_hover_and_x_marks(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, notes(&["a", "b", "c"]));
    let ids = h.ids();
    let hovered = |h: &mut Harness| h.read(|shelf| shelf.hovered_item());
    h.keys("down");
    assert_eq!(hovered(&mut h), Some(ids[0].clone()));
    assert!(h.read(|shelf| shelf.hover.keyboard()));
    h.keys("down down down");
    assert_eq!(hovered(&mut h), Some(ids[2].clone()));
    h.keys("up x");
    assert_eq!(hovered(&mut h), Some(ids[1].clone()));
    assert_eq!(h.selected(), ["b"]);
    h.keys("x");
    assert!(h.selected().is_empty());
    h.act(|shelf, _, _| shelf.hover(Hovered::Item(ids[1].clone()), false));
    h.keys("up");
    assert_eq!(hovered(&mut h), Some(ids[2].clone()));
    h.keys("left");
    assert_eq!(hovered(&mut h), Some(ids[2].clone()));
}

#[gpui::test]
fn grid_steps_by_rows_and_columns(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, notes(&["a", "b", "c", "d", "e"]));
    let ids = h.ids();
    h.keys("ctrl-g");
    assert!(h.saved().settings().layout == Layout::Grid);
    let (width, _) = h.min_size();
    assert_eq!(width, crate::geometry::required_width(Layout::Grid));
    let columns = h.read(|shelf| shelf.rows.get().columns.max(1));
    h.keys("right right down");
    let below = (1 + columns).min(4);
    assert_eq!(h.read(|shelf| shelf.hovered_item()), Some(ids[below].clone()));
    h.keys("left up");
    assert_eq!(h.read(|shelf| shelf.hovered_item()), Some(ids[(below - 1).saturating_sub(columns)].clone()));
    h.keys("ctrl-g");
    assert!(h.saved().settings().layout == Layout::List);
}

#[gpui::test]
fn hover_moves_with_mouse_after_keyboard_navigation(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, notes(&["a", "b"]));
    let ids = h.ids();
    h.keys("down");
    h.act(|shelf, _, cx| shelf.track(&mouse_move(point(px(1.), px(1.)), None), cx));
    assert!(h.read(|shelf| shelf.hover.target().is_none() && !shelf.hover.keyboard()));
    h.act(|shelf, _, cx| shelf.point(&ids[1], cx));
    assert_eq!(h.read(|shelf| shelf.hovered_item()), Some(ids[1].clone()));
}

#[gpui::test]
fn delete_removes_hovered_item_or_selection(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, notes(&["a", "b", "c", "d"]));
    let ids = h.ids();
    h.act(|shelf, _, cx| {
        shelf.toggle(&ids[0], cx);
        shelf.toggle(&ids[1], cx);
        shelf.hover(Hovered::Item(ids[3].clone()), true);
    });
    h.keys("delete");
    assert_eq!(h.titles(), ["a", "b", "c"]);
    assert_eq!(h.selected(), ["a", "b"]);
    h.act(|shelf, _, _| shelf.hover(Hovered::Item(ids[0].clone()), true));
    h.keys("backspace");
    assert_eq!(h.titles(), ["c"]);
    assert!(h.selected().is_empty());
    assert_eq!(h.saved().items().len(), 1);
    h.keys("delete");
    assert_eq!(h.titles(), ["c"]);
}

#[gpui::test]
fn removed_notes_are_deleted_from_storage_after_save(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, notes(&["a"]));
    let path = h.read(|shelf| shelf.store.items()[0].path.clone());
    assert!(path.exists());
    let id = h.ids().remove(0);
    h.act(|shelf, _, cx| shelf.remove(&id, cx));
    assert!(!path.exists());
}

fn kept(store: &mut Store, name: &str) -> PathBuf {
    store.settings_mut().recycle = false;
    let path = store.storage().content().join(name);
    std::fs::create_dir_all(store.storage().content()).expect("content folder");
    std::fs::write(&path, "x").expect("kept file");
    path
}

#[gpui::test]
fn removing_a_file_kept_only_in_traydex_asks_before_deleting_it(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, |sandbox, store, _| {
        let paths = vec![sandbox.file("отчёт 1.pdf", "x"), kept(store, "снимок экрана.png")];
        store.add_paths(paths);
    });
    let ids = h.ids();
    let (outside, inside) = h.read(|shelf| (shelf.store.items()[0].path.clone(), shelf.store.items()[1].path.clone()));
    assert!(h.read(|shelf| shelf.clear_loses_files()), "clearing warns about the stored file too");
    h.act(|shelf, _, cx| shelf.toggle_stuck(&ids[1], cx));
    assert!(!h.read(|shelf| shelf.clear_loses_files()), "a pinned card survives clearing");
    h.act(|shelf, _, cx| shelf.toggle_stuck(&ids[1], cx));
    h.act(|shelf, _, cx| shelf.remove(&ids[0], cx));
    assert_eq!(h.titles(), ["снимок экрана.png"], "a file that lives elsewhere goes at once");
    assert!(outside.exists());
    assert!(h.read(|shelf| shelf.confirming.is_none()));
    h.act(|shelf, _, cx| shelf.remove(&ids[1], cx));
    assert!(h.read(|shelf| matches!(&shelf.confirming, Some(Confirm::Remove(keys)) if keys.contains(&ids[1]))));
    h.keys("escape");
    assert_eq!(h.titles(), ["снимок экрана.png"]);
    assert!(inside.exists());
    h.act(|shelf, _, cx| shelf.remove(&ids[1], cx));
    h.keys("enter");
    assert!(h.titles().is_empty());
    assert!(h.saved().items().is_empty());
    assert!(!inside.exists());
}

#[gpui::test]
fn removing_a_selection_with_a_kept_file_asks_once_for_all(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, |sandbox, store, _| {
        let paths = vec![sandbox.file("a.txt", "x"), kept(store, "b.png"), sandbox.file("c.txt", "x")];
        store.add_paths(paths);
    });
    let ids = h.ids();
    h.act(|shelf, _, cx| {
        shelf.toggle(&ids[0], cx);
        shelf.toggle(&ids[1], cx);
        shelf.remove_selected(cx);
    });
    assert_eq!(h.titles(), ["a.txt", "b.png", "c.txt"]);
    h.keys("enter");
    assert_eq!(h.titles(), ["c.txt"]);
    assert!(h.selected().is_empty());
}

#[gpui::test]
fn deleting_a_previewed_kept_file_moves_to_the_next_after_confirmation(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, |_, store, _| {
        let paths = vec![kept(store, "a.png"), kept(store, "b.png")];
        store.add_paths(paths);
    });
    let ids = h.ids();
    h.act(|shelf, window, cx| shelf.show_preview(ids[0].clone(), window, cx));
    h.keys("delete");
    assert_eq!(h.previewed(), Some(ids[0].clone()), "stays on the file until the deletion is confirmed");
    assert_eq!(h.titles().len(), 2);
    h.keys("enter");
    assert_eq!(h.titles(), ["b.png"]);
    assert_eq!(h.previewed(), Some(ids[1].clone()));
}

#[gpui::test]
fn with_the_recycle_bin_a_stored_file_goes_there_without_asking_and_comes_back(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, |_, store, _| {
        let path = kept(store, "снимок экрана.png");
        store.settings_mut().recycle = true;
        store.add_paths(vec![path]);
    });
    let id = h.ids().remove(0);
    let path = h.read(|shelf| shelf.store.items()[0].path.clone());
    h.act(|shelf, _, cx| shelf.remove(&id, cx));
    assert!(h.read(|shelf| shelf.confirming.is_none() && !shelf.clear_loses_files()));
    h.settle();
    assert!(h.titles().is_empty());
    assert_eq!(h.count(&Call::Recycle(vec![path.clone()])), 1);
    assert!(!path.exists());
    std::fs::write(&path, "x").expect("restored from the Recycle Bin");
    h.send(Event::Toggle);
    h.send(Event::Toggle);
    h.settle();
    assert_eq!(h.titles(), ["снимок экрана.png"]);
    assert_eq!(h.saved().items().len(), 1);
}

#[gpui::test]
fn a_file_still_waiting_for_the_recycle_bin_is_not_taken_for_restored(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, |_, store, _| {
        let path = kept(store, "a.png");
        store.settings_mut().recycle = true;
        store.add_paths(vec![path]);
    });
    let id = h.ids().remove(0);
    h.act(|shelf, _, cx| {
        shelf.remove(&id, cx);
        shelf.find_recycled(cx);
    });
    h.settle();
    h.act(|shelf, _, cx| shelf.find_recycled(cx));
    h.settle();
    assert!(h.titles().is_empty());
}

#[gpui::test]
fn text_items_are_copied_as_plain_text(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, notes(&["first", "second"]));
    h.keys("ctrl-c");
    assert_eq!(h.cx.read_from_clipboard().and_then(|item| item.text()).as_deref(), Some("first\r\nsecond"));
    assert!(!h.fake.calls().iter().any(|call| matches!(call, Call::Copy(_))));
    assert_eq!(h.copied(), Some(super::super::COPY_SELECTED.to_string()));
    h.elapse(Duration::from_millis(1400));
    assert!(h.copied().is_some());
    h.elapse(Duration::from_millis(100));
    assert!(h.copied().is_none());
    let ids = h.ids();
    h.act(|shelf, _, _| shelf.hover(Hovered::Item(ids[1].clone()), true));
    h.keys("ctrl-c");
    assert_eq!(h.cx.read_from_clipboard().and_then(|item| item.text()).as_deref(), Some("second"));
    assert_eq!(h.copied(), Some(super::super::copy_id(&ids[1]).to_string()));
}

#[gpui::test]
fn files_are_copied_through_the_system(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, |sandbox, store, _| {
        store.add_paths(vec![sandbox.file("a.txt", "a")]);
        store.add_text("note".into()).expect("note");
    });
    let ids = h.ids();
    h.keys("ctrl-c");
    assert_eq!(h.fake.calls().last(), Some(&Call::Copy(ids.clone())));
    assert!(h.copied().is_some());
    h.fake.state().copy_error = Some("clipboard busy".into());
    h.elapse(Duration::from_secs(2));
    h.keys("ctrl-c");
    assert_eq!(h.alert().as_deref(), Some("clipboard busy"));
    assert!(h.copied().is_none());
    h.act(|shelf, _, cx| shelf.copy(Vec::new(), "nothing", cx));
    assert_eq!(h.count(&Call::Copy(Vec::new())), 0);
}

#[gpui::test]
fn paste_adds_clipboard_contents_or_reports_errors(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    h.fake.state().pasted.push_back(Ok(Payload::Text("pasted text".into())));
    h.keys("ctrl-v");
    assert_eq!(h.titles(), ["pasted text"]);
    h.keys("ctrl-v");
    assert_eq!(h.alert().as_deref(), Some("empty clipboard"));
    h.fake.state().pasted.push_back(Ok(Payload::Text("pasted text".into())));
    h.keys("ctrl-v");
    assert_eq!(h.titles(), ["pasted text"]);
}

#[gpui::test]
fn pasted_image_is_saved_off_the_main_thread(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    h.fake.state().pasted.push_back(Ok(Payload::Image(ImageData::Png(png(4, 3)))));
    h.cx.simulate_keystrokes("ctrl-v");
    assert!(h.titles().is_empty());
    assert_eq!(h.fake.jobs(), 1);
    h.settle();
    let item = h.read(|shelf| shelf.store.items()[0].clone());
    assert!(item.kind == Kind::Image);
    assert!(item.managed && item.path.starts_with(h.sandbox.storage().content()));
    assert_eq!(image::open(&item.path).expect("image").width(), 4);
    h.fake.state().pasted.push_back(Ok(Payload::Image(ImageData::Dib(vec![1, 2, 3]))));
    h.keys("ctrl-v");
    assert!(h.alert().is_some_and(|alert| alert.starts_with(fill(tr().add_failed, "").trim_end())));
    assert_eq!(h.titles().len(), 1);
}

#[gpui::test]
fn adding_items_resets_filter_search_and_page(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, notes(&["hello"]));
    h.keys("/");
    h.cx.simulate_input("hel");
    h.settle();
    h.act(|shelf, _, cx| {
        shelf.set_filter(Filter::Images, cx);
        shelf.toggle_page(Page::Settings, cx);
    });
    h.send(Event::Drop(Payload::Text("world".into())));
    h.read(|shelf| {
        assert!(shelf.filter() == Filter::All && shelf.page() == Page::Shelf);
        assert!(!shelf.catalog.searching());
    });
    assert_eq!(h.titles(), ["world", "hello"]);
}

#[gpui::test]
fn preview_ignores_stale_results(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, notes(&["first", "second", "third"]));
    let ids = h.ids();
    h.pending(|shelf, window, cx| shelf.show_preview(ids[0].clone(), window, cx));
    h.pending(|shelf, window, cx| shelf.step_preview(1, window, cx));
    assert_eq!(h.fake.jobs(), 2);
    h.read(|shelf| {
        let preview = shelf.preview().expect("preview");
        assert!(preview.key == ids[1] && preview.loaded.is_none());
    });
    h.fake.state().jobs.rotate_left(1);
    h.settle();
    h.read(|shelf| {
        let preview = shelf.preview().expect("preview");
        assert_eq!(preview.key, ids[1]);
        let Some(Content::Text { lines, .. }) = preview.loaded.as_ref().map(|loaded| &loaded.content) else {
            panic!("text preview");
        };
        assert_eq!(lines[0].text.as_ref(), "second");
    });
}

#[gpui::test]
fn image_zoom_starts_over_on_the_next_item(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, notes(&["first", "second"]));
    let ids = h.ids();
    h.pending(|shelf, window, cx| shelf.show_preview(ids[0].clone(), window, cx));
    let stage = crate::preview::Stage::new(size(800., 600.), size(400., 300.), 0.);
    h.act(|shelf, _, cx| shelf.zoom_preview(2., point(100., 100.), stage, cx));
    let zoomed = |h: &mut Harness| h.read(|shelf| shelf.preview().expect("preview").zoom.zoomed());
    assert!(zoomed(&mut h));
    h.pending(|shelf, window, cx| shelf.step_preview(1, window, cx));
    assert!(!zoomed(&mut h));
}

#[gpui::test]
fn preview_steps_within_bounds_and_opens_targets(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, notes(&["first", "second"]));
    let ids = h.ids();
    h.act(|shelf, _, _| shelf.hover(Hovered::Item(ids[0].clone()), true));
    h.keys("space");
    assert!(h.read(|shelf| shelf.page() == Page::Preview));
    h.keys("left");
    assert_eq!(h.previewed(), Some(ids[0].clone()));
    h.keys("down");
    assert_eq!(h.previewed(), Some(ids[1].clone()));
    h.keys("right");
    assert_eq!(h.previewed(), Some(ids[1].clone()));
    h.fake.take_calls();
    h.keys("enter ctrl-enter");
    assert_eq!(h.fake.take_calls(), [Call::Open(ids[1].clone()), Call::Reveal(ids[1].clone())]);
    h.keys("ctrl-c");
    assert_eq!(h.cx.read_from_clipboard().and_then(|item| item.text()).as_deref(), Some("second"));
    h.keys("space");
    assert!(h.read(|shelf| shelf.page() == Page::Shelf && shelf.preview().is_none()));
}

#[gpui::test]
fn preview_opens_selected_item_when_nothing_is_hovered(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, notes(&["first", "second"]));
    let ids = h.ids();
    h.keys("space");
    assert!(h.read(|shelf| shelf.page() == Page::Shelf));
    h.act(|shelf, _, cx| shelf.toggle(&ids[1], cx));
    h.keys("space");
    assert_eq!(h.previewed(), Some(ids[1].clone()));
    h.send(Event::Escape);
    assert!(h.read(|shelf| shelf.page() == Page::Shelf));
    assert_eq!(h.fake.calls().last(), Some(&Call::Focus));
}

#[gpui::test]
fn removing_previewed_item_moves_to_neighbour(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, notes(&["a", "b", "c"]));
    let ids = h.ids();
    h.act(|shelf, window, cx| shelf.show_preview(ids[1].clone(), window, cx));
    h.keys("delete");
    assert_eq!(h.previewed(), Some(ids[2].clone()));
    h.keys("delete");
    assert_eq!(h.previewed(), Some(ids[0].clone()));
    h.keys("delete");
    assert!(h.read(|shelf| shelf.page() == Page::Shelf && shelf.preview().is_none()));
    assert!(h.saved().items().is_empty());
}

#[gpui::test]
fn shell_preview_attaches_host_or_falls_back(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, |sandbox, store, fake| {
        store.add_paths(vec![sandbox.file("doc.pdf", "%PDF")]);
        fake.state().handler = Some(Handler(7));
    });
    let id = h.ids().remove(0);
    h.render();
    h.act(|shelf, window, cx| shelf.show_preview(id.clone(), window, cx));
    assert!(h.fake.calls().iter().any(|call| matches!(call, Call::Embed(_))));
    assert!(h.read(|shelf| shelf.preview().is_some_and(|preview| preview.host.is_some())));
    h.fake.take_calls();
    h.act(|shelf, _, cx| shelf.reclaim_focus(cx));
    assert_eq!(h.fake.take_calls(), [Call::Focus]);
    h.act(|shelf, _, cx| shelf.leave_preview(cx));
    h.fake.state().embed_error = Some("handler crashed".into());
    h.act(|shelf, window, cx| shelf.show_preview(id.clone(), window, cx));
    h.read(|shelf| {
        let preview = shelf.preview().expect("preview");
        assert!(preview.host.is_none());
        assert!(matches!(preview.loaded.as_ref().map(|l| &l.content), Some(Content::Thumbnail(None))));
    });
    assert!(h.alert().is_none());
}

#[gpui::test]
fn double_click_previews_or_opens(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, notes(&["a", "b"]));
    let ids = h.ids();
    let origin = point(px(10.), px(10.));
    h.act(|shelf, window, cx| {
        shelf.press(&ids[0], &mouse_down(origin, 1), window, cx);
        shelf.toggle(&ids[0], cx);
        shelf.press(&ids[0], &mouse_down(origin, 2), window, cx);
    });
    assert!(h.selected().is_empty());
    assert_eq!(h.previewed(), Some(ids[0].clone()));
    h.act(|shelf, _, cx| {
        shelf.leave_preview(cx);
        shelf.toggle_double_click_preview(cx);
    });
    assert!(!h.saved().settings().double_click_preview);
    h.fake.take_calls();
    h.act(|shelf, window, cx| shelf.press(&ids[1], &mouse_down(origin, 2), window, cx));
    assert_eq!(h.fake.take_calls(), [Call::Open(ids[1].clone())]);
}

#[gpui::test]
fn dragging_items_out_starts_after_a_small_move(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, notes(&["a", "b", "c"]));
    let ids = h.ids();
    let origin = point(px(100.), px(100.));
    h.act(|shelf, window, cx| {
        shelf.toggle(&ids[0], cx);
        shelf.toggle(&ids[2], cx);
        shelf.press(&ids[2], &mouse_down(origin, 1), window, cx);
        shelf.track(&mouse_move(point(px(102.), px(102.)), Some(MouseButton::Left)), cx);
    });
    assert!(!h.fake.calls().iter().any(|call| matches!(call, Call::Drag(_))));
    h.act(|shelf, _, cx| shelf.track(&mouse_move(point(px(103.), px(103.)), Some(MouseButton::Left)), cx));
    assert_eq!(h.fake.calls().last(), Some(&Call::Drag(vec![ids[0].clone(), ids[2].clone()])));
    h.act(|shelf, _, cx| shelf.track(&mouse_move(point(px(150.), px(150.)), Some(MouseButton::Left)), cx));
    assert_eq!(h.fake.calls().iter().filter(|call| matches!(call, Call::Drag(_))).count(), 1);
    h.fake.state().drag_error = Some("drag refused".into());
    h.act(|shelf, window, cx| {
        shelf.press(&ids[1], &mouse_down(origin, 1), window, cx);
        shelf.track(&mouse_move(point(px(100.), px(120.)), Some(MouseButton::Left)), cx);
    });
    assert_eq!(h.fake.calls().last(), Some(&Call::Drag(vec![ids[1].clone()])));
    assert_eq!(h.alert().as_deref(), Some("drag refused"));
    h.act(|shelf, window, cx| {
        shelf.press(&ids[1], &mouse_down(origin, 1), window, cx);
        shelf.track(&mouse_move(point(px(100.), px(101.)), None), cx);
        shelf.track(&mouse_move(point(px(100.), px(150.)), Some(MouseButton::Left)), cx);
    });
    assert_eq!(h.fake.calls().iter().filter(|call| matches!(call, Call::Drag(_))).count(), 2);
}

#[gpui::test]
fn drag_all_skips_missing_files(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, |sandbox, store, _| {
        store.add_paths(vec![sandbox.file("gone.txt", "x"), sandbox.file("kept.txt", "y")]);
    });
    let ids = h.ids();
    std::fs::remove_file(h.sandbox.join("gone.txt")).expect("remove");
    h.act(|shelf, _, cx| shelf.refresh(cx));
    h.act(|shelf, _, cx| {
        shelf.press_all(&mouse_down(point(px(0.), px(0.)), 1));
        shelf.track(&mouse_move(point(px(0.), px(40.)), Some(MouseButton::Left)), cx);
    });
    assert_eq!(h.fake.calls().last(), Some(&Call::Drag(vec![ids[1].clone()])));
}

#[gpui::test]
fn the_wheel_zooms_an_image_right_after_it_opens(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, |sandbox, store, _| {
        store.add_paths(vec![sandbox.file("фото.png", png(400, 300))]);
    });
    let photo = h.ids()[0].clone();
    h.pending(|shelf, window, cx| shelf.show_preview(photo, window, cx));
    h.settle();
    h.cx.run_until_parked();
    let center = h.center("preview-image");
    h.read(|shelf| shelf.preview().expect("preview").frame.set(None));
    h.cx.update(|window, _| window.refresh());
    h.cx.run_until_parked();
    h.cx.simulate_event(gpui::ScrollWheelEvent {
        position: center,
        delta: gpui::ScrollDelta::Lines(point(0., 3.)),
        ..Default::default()
    });
    h.settle();
    assert!(
        h.read(|shelf| shelf.preview().expect("preview").zoom.zoomed()),
        "the frame drawn before the area was measured still zooms, no click is needed first"
    );
}

#[gpui::test]
fn the_wheel_glides_the_list_by_exactly_one_notch(cx: &mut TestAppContext) {
    const NOTCH: f32 = 60.;
    let mut h = open_with(cx, true, |_, store, _| {
        for index in 0..80 {
            store.add_text(format!("note {index}")).expect("note");
        }
    });
    let position = h.center("items");
    let offset = |h: &mut Harness| h.read(|shelf| shelf.scroll(Page::Shelf).offset().y);
    h.cx.simulate_event(gpui::ScrollWheelEvent {
        position,
        delta: gpui::ScrollDelta::Pixels(point(px(0.), px(-NOTCH))),
        ..Default::default()
    });
    h.settle();
    let first = offset(&mut h);
    assert!(first < px(0.) && first > px(-NOTCH), "the first frame moves part of the way, got {first:?}");
    for _ in 0..60 {
        h.cx.executor().advance_clock(Duration::from_millis(16));
        h.cx.update(|window, _| window.refresh());
        h.settle();
    }
    assert_eq!(offset(&mut h), px(-NOTCH), "the list stops one notch down, not two");
}
