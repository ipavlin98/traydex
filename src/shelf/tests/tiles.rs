use super::*;

#[gpui::test]
fn drop_on_tile_runs_the_action_and_adds_results(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    let file = h.sandbox.file("report.txt", "data");
    h.send(Event::Hover(true));
    assert!(h.read(|shelf| shelf.dropping() && shelf.regions.zone().is_some()));
    let (x, y) = h.tile_center(0);
    let first = h.tile(0);
    h.send(Event::Pointer(x, y));
    assert!(h.read(|shelf| shelf.session.look().over == Some(Target::Tile(first))));
    assert_eq!(h.fake.calls().last(), Some(&Call::Reject(false)));
    h.events.unbounded_send(Event::Drop(Payload::Paths(vec![file]))).expect("send");
    h.cx.run_until_parked();
    assert_eq!(h.running(), 1);
    h.settle();
    assert!(h.running() == 0);
    assert_eq!(h.titles(), ["report.zip"]);
    assert_eq!(h.saved().items().len(), 1);
    assert!(!h.fake.calls().iter().any(|call| matches!(call, Call::Notify(_))));
    assert!(h.alert().is_none());
}

#[gpui::test]
fn drop_on_empty_zone_goes_to_shelf_and_gaps_are_rejected(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    h.send(Event::Hover(true));
    let zone = h.read(|shelf| shelf.regions.zone().expect("zone"));
    let inside = zone.center();
    h.send(Event::Pointer(f32::from(inside.x), f32::from(inside.y)));
    assert!(h.read(|shelf| shelf.session.look().over == Some(Target::Shelf)));
    h.send(Event::Drop(Payload::Text("dropped".into())));
    assert_eq!(h.titles(), ["dropped"]);
    h.send(Event::Hover(true));
    h.send(Event::Pointer(-50., -50.));
    assert_eq!(h.fake.calls().last(), Some(&Call::Reject(true)));
    h.send(Event::Drop(Payload::Text("lost".into())));
    assert_eq!(h.titles(), ["dropped"]);
    h.send(Event::Hover(false));
    assert_eq!(h.fake.calls().last(), Some(&Call::Reject(false)));
    assert!(h.read(|shelf| shelf.session.look().over.is_none() && shelf.session.pointer().is_none()));
}

#[gpui::test]
fn hover_returns_to_shelf_page(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, notes(&["a"]));
    h.act(|shelf, _, cx| {
        shelf.toggle_page(Page::Settings, cx);
        shelf.ask_clear(cx);
    });
    h.send(Event::Hover(true));
    assert!(h.read(|shelf| shelf.page() == Page::Shelf && shelf.confirming.is_none() && shelf.session.look().hovering));
}

#[gpui::test]
fn failed_action_alerts_and_notifies_when_hidden(cx: &mut TestAppContext) {
    let mut h = open(cx, false);
    let missing = h.sandbox.join("missing.txt");
    let zip = h.tile(0);
    h.act(|shelf, _, cx| shelf.perform(zip, Payload::Paths(vec![missing]), cx));
    let alert = h.alert().expect("alert");
    assert!(alert.starts_with(&format!("{}: ", Action::new(actions::Kind::Zip).title())));
    assert!(h.fake.calls().iter().any(|call| matches!(call, Call::Notify(text) if *text == alert)));
    h.act(|shelf, _, cx| shelf.remove_action(zip, cx));
    h.act(|shelf, _, cx| shelf.perform(zip, Payload::Text("fallback".into()), cx));
    assert_eq!(h.titles(), ["fallback"]);
}

#[gpui::test]
fn successful_action_notifies_only_when_hidden(cx: &mut TestAppContext) {
    let mut h = open(cx, false);
    let file = h.sandbox.file("a.txt", "a");
    let zip = h.tile(0);
    h.act(|shelf, _, cx| shelf.perform(zip, Payload::Paths(vec![file]), cx));
    assert!(h.fake.calls().iter().any(|call| matches!(call, Call::Notify(_))));
    assert!(h.alert().is_none());
    assert_eq!(h.titles(), ["a.zip"]);
}

#[gpui::test]
fn web_search_hides_unpinned_window_and_opens_browser(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    let index = h.tile_of(|kind| matches!(kind, actions::Kind::Search { .. })).expect("search tile");
    h.fake.take_calls();
    h.pending(|shelf, _, cx| shelf.perform(index, Payload::Text("kittens".into()), cx));
    assert!(!h.fake.state().visible);
    h.settle();
    assert!(h.fake.calls().iter().any(|call| matches!(call, Call::ShellOpen(url) if url.contains("kittens"))));
    h.act(|shelf, _, cx| {
        shelf.toggle_pinned(cx);
        shelf.reveal(cx);
    });
    h.act(|shelf, _, cx| shelf.perform(index, Payload::Text("puppies".into()), cx));
    assert!(h.fake.state().visible);
}

fn add_phone_tile(h: &mut Harness) -> TileId {
    add_tile(h, actions::Kind::Phone)
}

#[gpui::test]
fn phone_tile_shows_the_link_until_the_page_is_left(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    let index = add_phone_tile(&mut h);
    let file = h.sandbox.file("photo.png", png(2, 2));
    h.act(|shelf, _, cx| shelf.perform(index, Payload::Paths(vec![file.clone()]), cx));
    h.render();
    assert!(h.read(|shelf| shelf.page() == Page::Share));
    let url = h.read(|shelf| shelf.share().map(|share| share.server.url.clone()));
    assert_eq!(url.as_deref(), Some("http://192.0.2.10:8000/1"));
    assert!(h.fake.calls().contains(&Call::Serve(vec![file])));
    h.keys("ctrl-c");
    assert_eq!(h.cx.read_from_clipboard().and_then(|item| item.text()).as_deref(), Some("http://192.0.2.10:8000/1"));
    h.keys("escape");
    h.render();
    assert!(h.read(|shelf| shelf.page() == Page::Shelf && shelf.share().is_none()));
    assert!(h.fake.state().visible);
}

#[gpui::test]
fn phone_link_reveals_hidden_window_and_stops_when_it_hides(cx: &mut TestAppContext) {
    let mut h = open(cx, false);
    let index = add_phone_tile(&mut h);
    h.act(|shelf, _, cx| shelf.perform(index, Payload::Text("note".into()), cx));
    assert!(h.fake.state().visible);
    assert!(h.read(|shelf| shelf.page() == Page::Share && shelf.share().is_some()));
    h.act(|shelf, _, cx| shelf.hide("test", cx));
    h.render();
    assert!(h.read(|shelf| shelf.share().is_none()));
    h.fake.state().serve_error = Some("port busy".into());
    h.act(|shelf, _, cx| shelf.perform(index, Payload::Text("note".into()), cx));
    assert!(h.read(|shelf| shelf.page() == Page::Shelf && shelf.share().is_none()));
    assert!(h.alert().is_some_and(|alert| alert.ends_with("port busy")));
}

#[gpui::test]
fn tool_install_runs_once_and_can_be_cancelled(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    let place = Action::new(actions::Kind::Gif).slot();
    h.fake.state().downloader = Some(Box::new(|_: &str, _: &Path, progress: &crate::system::Progress| {
        anyhow::ensure!(!progress.cancelled(), "cancelled");
        anyhow::bail!("unexpected download")
    }));
    h.pending(|shelf, _, cx| {
        shelf.install(Tool::Ffmpeg, place.clone(), cx);
        shelf.install(Tool::Ffmpeg, place.clone(), cx);
    });
    assert_eq!(h.running(), 1);
    assert_eq!(h.fake.jobs(), 1);
    h.pending(|shelf, _, cx| shelf.cancel_install(Tool::Ffmpeg, cx));
    h.settle();
    assert!(h.running() == 0);
    assert!(h.alert().is_none());
    assert!(h.read(|shelf| shelf.presence(Tool::Ffmpeg)) == actions::Presence::Missing);
    h.act(|shelf, _, cx| shelf.install(Tool::Ffmpeg, place.clone(), cx));
    assert!(h.alert().is_some_and(|alert| alert.contains("unexpected download")));
    assert!(!h.read(|shelf| shelf.presence(Tool::Ffmpeg).ready()));
}

#[gpui::test]
fn removing_a_tool_keeps_its_tile_and_editor(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, |_, store, _| plant_tools(store.storage()));
    let gif = Action::new(actions::Kind::Gif);
    let tile = h.act(|shelf, _, cx| shelf.add_action(gif.clone(), cx)).expect("tile");
    h.act(|shelf, window, cx| shelf.open_editor(tile, None, window, cx));
    assert!(h.read(|shelf| shelf.presence(Tool::Ffmpeg)) == actions::Presence::Bundled);
    h.act(|shelf, _, cx| shelf.remove_tool(Tool::Ffmpeg, cx));
    assert!(h.read(|shelf| shelf.presence(Tool::Ffmpeg)) == actions::Presence::Missing);
    assert_eq!(h.editing(), Some(tile));
    assert!(h.read(|shelf| shelf.page() == Page::Edit));
    assert!(saved_tiles(&h).iter().any(|action| action.slot() == gif.slot()));
}

#[gpui::test]
fn removing_a_tile_cancels_its_download(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    let gif = Action::new(actions::Kind::Gif);
    let place = gif.slot();
    let index = h.act(|shelf, _, cx| shelf.add_action(gif.clone(), cx)).expect("tile");
    h.pending(|shelf, _, cx| shelf.install(Tool::Ffmpeg, place.clone(), cx));
    let progress = h.read_cx(|shelf, cx| shelf.tool_job(Tool::Ffmpeg, cx).expect("job").progress.clone());
    h.pending(|shelf, _, cx| shelf.remove_action(index, cx));
    assert!(progress.cancelled());
    h.settle();
    assert!(h.alert().is_none());
    assert!(!saved_tiles(&h).iter().any(|a| a.slot() == place));
}

#[gpui::test]
fn tiles_are_added_once_and_persisted(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    let defaults = h.read(|shelf| shelf.tiles.len());
    h.act(|shelf, _, cx| {
        assert!(shelf.add_action(Action::new(actions::Kind::CopyPath), cx).is_some());
        assert!(shelf.add_action(Action::new(actions::Kind::CopyPath), cx).is_none());
    });
    assert_eq!(saved_tiles(&h).len(), defaults + 1);
    h.act(|shelf, window, cx| shelf.toggle_action(Action::new(actions::Kind::CopyPath), None, window, cx));
    assert_eq!(h.read(|shelf| shelf.tiles.len()), defaults);
    h.act(|shelf, window, cx| {
        shelf.toggle_action(Action::new(actions::Kind::Imgur { client_id: String::new() }), None, window, cx)
    });
    assert!(h.read(|shelf| shelf.page() == Page::Edit
        && shelf.editor().is_some_and(|e| shelf.tiles.position(e.tile) == Some(defaults))));
}

#[gpui::test]
fn arrows_move_a_tile_one_place_and_stop_at_the_ends(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    add_tile(&mut h, actions::Kind::Pdf);
    h.act(|shelf, _, cx| shelf.toggle_page(Page::Actions, cx));
    h.cx.simulate_resize(size(px(540.), px(900.)));
    h.settle();
    let before = h.order();
    let last = before.len() - 1;
    let down = h.center("move-down:0");
    h.cx.simulate_click(down, Modifiers::default());
    h.settle();
    let mut expected = before.clone();
    expected.swap(0, 1);
    assert_eq!(h.order(), expected);
    assert!(h.read(|shelf| shelf.page() == Page::Actions), "an arrow does not open the editor");
    let up = h.center(&format!("move-up:{last}"));
    h.cx.simulate_click(up, Modifiers::default());
    h.settle();
    expected.swap(last - 1, last);
    assert_eq!(h.order(), expected);
    let top = h.center("move-up:0");
    h.cx.simulate_click(top, Modifiers::default());
    h.settle();
    assert!(h.read(|shelf| shelf.page() == Page::Actions), "a disabled arrow does not open the editor");
    let bottom = h.center(&format!("move-down:{last}"));
    h.cx.simulate_click(bottom, Modifiers::default());
    h.settle();
    assert!(h.read(|shelf| shelf.page() == Page::Actions), "a disabled arrow does not open the editor");
    assert_eq!(h.order(), expected, "the first tile can't go up, the last can't go down");
    let saved: Vec<String> = saved_tiles(&h).iter().map(Action::title).collect();
    let shown: Vec<String> = h.read(|shelf| shelf.tiles.actions().map(|tile| tile.action.title()).collect());
    assert_eq!(saved, shown);
}

#[gpui::test]
fn dragging_a_row_puts_the_tile_in_the_place_of_the_target(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    add_tile(&mut h, actions::Kind::Pdf);
    add_tile(&mut h, actions::Kind::Qr);
    h.act(|shelf, _, cx| shelf.toggle_page(Page::Actions, cx));
    h.cx.simulate_resize(size(px(540.), px(900.)));
    h.settle();
    let before = h.order();
    assert_eq!(before.len(), 6, "five tiles and the cancel tile");
    let (from, to) = (h.center("action-row:0"), h.center("action-row:2"));
    h.cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::default());
    h.cx.simulate_mouse_move(point(from.x, from.y + px(10.)), Some(MouseButton::Left), Modifiers::default());
    h.cx.simulate_mouse_move(to, Some(MouseButton::Left), Modifiers::default());
    assert!(h.cx.debug_bounds("tile-ghost").is_some(), "the dragged tile follows the cursor");
    h.cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::default());
    h.settle();
    assert_eq!(h.order(), [before[1], before[2], before[0], before[3], before[4], before[5]]);
    assert!(h.read(|shelf| shelf.page() == Page::Actions), "a drag does not open the editor");
    let saved: Vec<String> = saved_tiles(&h).iter().map(Action::title).collect();
    let shown: Vec<String> = h.read(|shelf| shelf.tiles.actions().map(|tile| tile.action.title()).collect());
    assert_eq!(saved, shown);
    let (back, top) = (h.center("action-row:4"), h.center("action-row:0"));
    h.cx.simulate_mouse_down(back, MouseButton::Left, Modifiers::default());
    h.cx.simulate_mouse_move(top, Some(MouseButton::Left), Modifiers::default());
    h.cx.simulate_mouse_up(top, MouseButton::Left, Modifiers::default());
    h.settle();
    assert_eq!(h.order(), [before[4], before[1], before[2], before[0], before[3], before[5]]);
}

#[gpui::test]
fn dropping_a_row_on_itself_or_outside_the_list_keeps_the_order(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    h.act(|shelf, _, cx| shelf.toggle_page(Page::Actions, cx));
    h.cx.simulate_resize(size(px(540.), px(900.)));
    h.settle();
    let before = h.order();
    let from = h.center("action-row:1");
    h.cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::default());
    h.cx.simulate_mouse_move(point(from.x + px(30.), from.y), Some(MouseButton::Left), Modifiers::default());
    h.cx.simulate_mouse_up(point(from.x + px(30.), from.y), MouseButton::Left, Modifiers::default());
    h.settle();
    assert_eq!(h.order(), before);
    assert!(h.read(|shelf| shelf.page() == Page::Actions), "a drag does not open the editor");
    let chip = h.center("chip:add-zip");
    h.cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::default());
    h.cx.simulate_mouse_move(chip, Some(MouseButton::Left), Modifiers::default());
    h.cx.simulate_mouse_up(chip, MouseButton::Left, Modifiers::default());
    h.settle();
    assert_eq!(h.order(), before);
    assert_eq!(h.read(|shelf| shelf.tiles.iter().count()), before.len(), "dropping on a chip neither adds nor removes");
}

#[gpui::test]
fn tiles_beyond_the_limit_are_refused_with_an_explanation(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    fill_tiles(&mut h);
    let full = fill(tr().tiles_full, actions::MAX_TILES);
    assert_eq!(saved_tiles(&h).len(), actions::MAX_TILES);
    assert!(h.act(|shelf, _, cx| shelf.add_action(search(1000), cx)).is_none());
    assert_eq!(h.alert(), Some(full.clone()));
    h.act(|shelf, _, cx| shelf.pick_action("Pick", true, |folder| actions::Kind::Move { folder }, cx));
    assert!(
        !h.fake.calls().iter().any(|call| matches!(call, Call::Pick(..))),
        "no dialog for a tile that can't be added"
    );
    h.act(|shelf, window, cx| shelf.add_search(window, cx));
    h.act(|shelf, window, cx| shelf.toggle_action(Action::new(actions::Kind::Qr), None, window, cx));
    assert_eq!(h.read(|shelf| shelf.tiles.len()), actions::MAX_TILES);
    assert!(h.read(|shelf| shelf.page() != Page::Edit), "no editor for a refused tile");
    let first = h.tile(0);
    let added = h.read(|shelf| shelf.tiles.get(first).expect("tile").clone());
    h.act(|shelf, window, cx| shelf.toggle_action(added, None, window, cx));
    assert_eq!(h.read(|shelf| shelf.tiles.len()), actions::MAX_TILES - 1, "an added tile can still be removed");
    h.act(|shelf, window, cx| shelf.toggle_action(Action::new(actions::Kind::Qr), None, window, cx));
    assert_eq!(saved_tiles(&h).len(), actions::MAX_TILES);
}

#[gpui::test]
fn minimum_window_size_follows_the_tile_count(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    fill_tiles(&mut h);
    let many = h.min_size();
    assert!(
        many.0 > store::MIN_SIZE.0 && many.1 > store::MIN_SIZE.1,
        "{} tiles need a larger window than the smallest one: {many:?}",
        actions::MAX_TILES
    );
    while let Some(tile) = h.read(|shelf| shelf.tiles.actions().nth(1).map(|tile| tile.id)) {
        h.act(|shelf, _, cx| shelf.remove_action(tile, cx));
    }
    assert_eq!(h.min_size(), store::MIN_SIZE, "removing tiles lets the window shrink back");
}

#[gpui::test]
fn cancel_tile_is_edited_like_a_tile_but_cannot_be_removed(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    let cancel = h.tile_of(|kind| matches!(kind, actions::Kind::Cancel)).expect("cancel tile");
    h.act(|shelf, _, cx| shelf.toggle_page(Page::Actions, cx));
    h.act(|shelf, _, cx| shelf.remove_action(cancel, cx));
    assert!(h.read(|shelf| shelf.tiles.get(cancel).is_some()), "the cancel tile stays");
    h.act(|shelf, window, cx| shelf.open_editor(cancel, Some(Field::Name), window, cx));
    h.cx.simulate_input("Передумал");
    h.act(|shelf, _, cx| shelf.set_icon_only(cancel, true, cx));
    h.render();
    assert!(h.cx.debug_bounds("cancel-note").is_some(), "the editor says where the tile appears");
    assert!(h.cx.debug_bounds("delete-action").is_none(), "the editor offers no removal");
    let mut tiles = actions::Tiles::new(h.sandbox.storage());
    tiles.load(&*h.fake).expect("tiles");
    let saved = tiles.iter().find(|tile| tile.action.cancels()).expect("saved cancel tile");
    assert_eq!(saved.action.name, "Передумал");
    assert!(saved.action.icon_only);
}

#[gpui::test]
fn editor_saves_typed_values_and_submit_returns(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    h.act(|shelf, window, cx| {
        shelf.add_or_edit(Action::new(actions::Kind::Search { url: String::new() }), Some(Field::SearchUrl), window, cx)
    });
    let tile = h.editing().expect("editor");
    let index = h.read(|shelf| shelf.tiles.position(tile)).expect("tile");
    h.cx.simulate_input("https://find.example/?q=%s");
    h.settle();
    let saved = saved_tiles(&h);
    assert!(matches!(&saved[index].kind, actions::Kind::Search { url } if url == "https://find.example/?q=%s"));
    assert!(h.read(|shelf| shelf.page() == Page::Edit));
    h.keys("enter");
    assert!(h.read(|shelf| shelf.page() == Page::Actions));
}

#[gpui::test]
fn picking_a_folder_renames_tile_with_default_name(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, |sandbox, _, fake| {
        fake.state().desktop = Some(sandbox.folder("Desktop"));
    });
    let target = h.sandbox.folder("Archive");
    let first = h.tile(0);
    h.fake.state().picked.push_back(Ok(Some(target.clone())));
    h.pending(|shelf, _, cx| shelf.change_path(first, cx));
    assert!(h.read(|shelf| shelf.session.picking()));
    h.settle();
    h.read(|shelf| {
        assert!(!shelf.session.picking());
        let action = shelf.tiles.get(first).expect("tile");
        assert_eq!(action.name, "Shown Archive");
        assert_eq!(action.path(), Some(target.as_path()));
    });
    h.act(|shelf, _, _| shelf.tiles.get_mut(first).expect("tile").name = "Custom".into());
    h.fake.state().picked.push_back(Ok(Some(h.sandbox.folder("Other"))));
    h.act(|shelf, _, cx| shelf.change_path(first, cx));
    assert_eq!(h.read(|shelf| shelf.tiles.get(first).expect("tile").name.clone()), "Custom");
    h.fake.state().picked.push_back(Err(anyhow::anyhow!("dialog failed")));
    h.act(|shelf, _, cx| shelf.change_path(first, cx));
    assert_eq!(h.alert().as_deref(), Some("dialog failed"));
    assert!(h.read(|shelf| !shelf.session.picking()));
}

#[gpui::test]
fn picking_a_program_adds_a_tile(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    let before = h.read(|shelf| shelf.tiles.len());
    let program = h.sandbox.file("tool.exe", "MZ");
    h.act(|shelf, _, cx| shelf.pick_action("Pick", false, |program| actions::Kind::Run { program }, cx));
    assert_eq!(h.read(|shelf| shelf.tiles.len()), before);
    assert!(h.fake.calls().contains(&Call::Pick("Pick".into(), false)));
    h.fake.state().picked.push_back(Ok(Some(program.clone())));
    h.act(|shelf, _, cx| shelf.pick_action("Pick", false, |program| actions::Kind::Run { program }, cx));
    h.read(|shelf| {
        let added = &shelf.tiles.actions().last().expect("tile").action;
        assert_eq!(added.name, "Shown tool.exe");
        assert!(added.slot() == Slot::Program(program.to_string_lossy().to_lowercase()));
    });
}

#[gpui::test]
fn tile_description_and_icon_only_are_saved_from_the_editor(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    let first = h.tile(0);
    h.act(|shelf, window, cx| shelf.open_editor(first, Some(Field::Description), window, cx));
    h.activate();
    h.cx.simulate_input("Отправить бухгалтеру");
    h.act(|shelf, _, cx| shelf.set_icon_only(first, true, cx));
    h.settle();
    let saved = saved_tiles(&h);
    let tile = saved.first().expect("the first tile");
    assert_eq!(tile.description(), "Отправить бухгалтеру");
    assert!(tile.icon_only);
    h.act(|shelf, _, cx| shelf.set_icon_only(first, false, cx));
    assert!(!saved_tiles(&h).first().expect("the first tile").icon_only);
}

#[gpui::test]
fn failed_tile_save_keeps_the_editor_on_its_tile(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    std::fs::create_dir_all(h.sandbox.storage().root().join("actions.tmp")).expect("block");
    let first = h.tile(0);
    h.act(|shelf, _, cx| shelf.remove_action(first, cx));
    assert!(h.alert().is_some());
    let search = h.tile_of(|kind| matches!(kind, actions::Kind::Search { .. })).expect("search");
    h.act(|shelf, window, cx| shelf.open_editor(search, Some(Field::Name), window, cx));
    h.activate();
    h.cx.simulate_input("Web");
    h.settle();
    let named: Vec<bool> = h.read(|shelf| {
        shelf
            .tiles
            .iter()
            .filter(|tile| tile.action.name == "Web")
            .map(|tile| matches!(tile.action.kind, actions::Kind::Search { .. }))
            .collect()
    });
    assert_eq!(named, [true]);
}

#[test]
fn a_wheel_past_the_edge_of_a_row_hides_nothing_on_the_other_side() {
    use crate::shelf::Edge;
    let (fits, overflows) = (px(0.), px(100.));
    for offset in [px(-40.), px(0.), px(40.)] {
        assert!(Edge::BOTH.iter().all(|edge| !edge.hidden(offset, fits)), "a row that fits at {offset:?}");
    }
    assert!(!Edge::End.hidden(px(140.), overflows), "scrolled past the end");
    assert!(!Edge::Start.hidden(px(-40.), overflows), "scrolled past the start");
    assert!(Edge::Start.hidden(px(140.), overflows) && Edge::End.hidden(px(-40.), overflows));
}
