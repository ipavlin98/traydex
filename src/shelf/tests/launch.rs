use super::*;
use crate::store::TileInput;
use crate::testing::plant_plugin;
use gpui::{Bounds, ScrollDelta, ScrollWheelEvent};

fn add_search(h: &mut Harness) -> (TileId, String) {
    let tile = h.act(|shelf, _, cx| shelf.add_action(search(0), cx)).expect("tile");
    let index = h.read(|shelf| shelf.tiles.iter().position(|each| each.id == tile)).expect("listed");
    (tile, format!("strip-tile:{index}"))
}

fn searched(h: &Harness, text: &str) -> bool {
    h.fake.calls().iter().any(|call| matches!(call, Call::ShellOpen(url) if url.contains(text)))
}

fn set_input(h: &mut Harness, input: TileInput) {
    h.act(|shelf, _, cx| shelf.set_tile_input(input, cx));
    assert_eq!(h.saved().settings().tile_input, input);
}

#[gpui::test]
fn clicking_a_tile_on_the_shelf_processes_the_clipboard_by_default(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, notes(&["kittens"]));
    let (_, mark) = add_search(&mut h);
    h.render();
    h.fake.state().pasted.push_back(Ok(Payload::Text("puppies".into())));
    click(&mut h, interned(&mark));
    assert!(searched(&h, "puppies"), "the clipboard text is searched");
    assert!(!searched(&h, "kittens"), "the shelf is left alone");
    h.fake.take_calls();
    click(&mut h, interned(&mark));
    assert_eq!(h.alert().as_deref(), Some("empty clipboard"));
    assert!(!h.fake.calls().iter().any(|call| matches!(call, Call::ShellOpen(_))));
}

#[gpui::test]
fn selection_input_processes_only_the_selected_items(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, notes(&["kittens", "puppies"]));
    let (tile, _) = add_search(&mut h);
    set_input(&mut h, TileInput::Selection);
    h.act(|shelf, _, cx| shelf.launch(tile, cx));
    assert_eq!(h.alert().as_deref(), Some(tr().select_first));
    assert!(!h.fake.calls().iter().any(|call| matches!(call, Call::ShellOpen(_) | Call::Paste)));
    let ids = h.ids();
    h.act(|shelf, _, cx| shelf.toggle(&ids[0], cx));
    h.act(|shelf, _, cx| shelf.launch(tile, cx));
    assert!(searched(&h, "kittens") && !searched(&h, "puppies"));
}

#[gpui::test]
fn picker_input_processes_the_picked_files_and_ignores_a_cancel(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    let (tile, _) = add_search(&mut h);
    set_input(&mut h, TileInput::Picker);
    h.act(|shelf, _, cx| shelf.launch(tile, cx));
    assert_eq!(h.count(&Call::PickFiles(tr().pick_files.into())), 1);
    assert!(h.alert().is_none() && !h.fake.calls().iter().any(|call| matches!(call, Call::ShellOpen(_))));
    let report = h.sandbox.path().join("папка с пробелами").join("annual_report.pdf");
    h.fake.state().picked_files.push_back(Ok(vec![report]));
    h.act(|shelf, _, cx| shelf.launch(tile, cx));
    assert!(searched(&h, "annual"));
    assert!(!h.read(|shelf| shelf.session.picking()), "the window may hide again after the dialog");
}

#[gpui::test]
fn dragging_shelf_items_opens_the_tiles_without_the_shelf_zone(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, notes(&["kittens"]));
    let (tile, _) = add_search(&mut h);
    let index = h.read(|shelf| shelf.tiles.iter().position(|each| each.id == tile)).expect("listed");
    h.render();
    let list = h.center("items");
    let start = |h: &mut Harness| {
        h.act(|shelf, _, cx| {
            shelf.press_all(&mouse_down(list, 1));
            shelf.track(&mouse_move(point(list.x, list.y + px(40.)), Some(MouseButton::Left)), cx);
        });
        h.fake.state().exporting = true;
        assert!(h.read(|shelf| shelf.dropping()), "the tiles open as soon as a shelf item is dragged");
        assert!(h.cx.debug_bounds("drop-zone").is_none(), "there is no zone to drop back onto the shelf");
    };
    start(&mut h);
    let target = h.center(&format!("drop-tile:{index}"));
    let title = h.center("title-bar");
    h.send(Event::Pointer(f32::from(title.x), f32::from(title.y)));
    assert_eq!(h.fake.calls().last(), Some(&Call::Reject(true)), "only a tile takes the shelf's own items");
    h.send(Event::DropBack);
    h.send(Event::ExportEnd);
    assert!(!searched(&h, "kittens"));
    assert!(!h.read(|shelf| shelf.dropping()), "the shelf comes back after the drag");
    assert_eq!(h.ids().len(), 1, "nothing is added to the shelf");
    start(&mut h);
    h.send(Event::ExportEnd);
    assert!(!h.read(|shelf| shelf.dropping()), "a cancelled drag brings the shelf back");
    start(&mut h);
    h.send(Event::Pointer(f32::from(target.x), f32::from(target.y)));
    assert_eq!(h.fake.calls().last(), Some(&Call::Reject(false)), "the tile takes the items");
    h.send(Event::DropBack);
    h.send(Event::ExportEnd);
    assert!(searched(&h, "kittens"));
    assert_eq!(h.ids().len(), 1);
}

#[gpui::test]
fn dropping_a_shelf_drag_onto_the_cancel_tile_does_nothing(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, notes(&["kittens"]));
    add_search(&mut h);
    h.render();
    let actions = h.read(|shelf| shelf.tiles.len());
    assert!(h.cx.debug_bounds(interned(&format!("strip-tile:{}", actions - 1))).is_some());
    assert!(h.cx.debug_bounds(interned(&format!("strip-tile:{actions}"))).is_none(), "the strip has no cancel tile");
    h.send(Event::Hover(true));
    assert!(h.read(|shelf| shelf.dropping()));
    assert!(h.cx.debug_bounds("drop-cancel").is_none(), "a drag from the computer has no cancel tile");
    h.send(Event::Hover(false));
    h.render();
    let list = h.center("items");
    h.act(|shelf, _, cx| {
        shelf.press_all(&mouse_down(list, 1));
        shelf.track(&mouse_move(point(list.x, list.y + px(40.)), Some(MouseButton::Left)), cx);
    });
    h.fake.state().exporting = true;
    let cancel = h.center("drop-cancel");
    h.send(Event::Pointer(f32::from(cancel.x), f32::from(cancel.y)));
    assert_eq!(h.fake.calls().last(), Some(&Call::Reject(false)), "the cancel tile takes the drop like any tile");
    h.send(Event::DropBack);
    h.send(Event::ExportEnd);
    assert!(!h.read(|shelf| shelf.dropping()), "the shelf comes back");
    assert!(!h.fake.calls().iter().any(|call| matches!(call, Call::ShellOpen(_))), "no tile runs");
    assert_eq!(h.ids().len(), 1, "nothing is added to the shelf");
    assert_eq!(h.running(), 0);
}

#[gpui::test]
fn the_strip_spreads_a_few_tiles_across_the_width_and_scrolls_many(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, notes(&["kittens"]));
    while let Some(tile) = h.read(|shelf| shelf.tiles.actions().next().map(|tile| tile.id)) {
        h.act(|shelf, _, cx| shelf.remove_action(tile, cx));
    }
    for index in 0..3 {
        h.act(|shelf, _, cx| shelf.add_action(search(1000 + index), cx)).expect("tile");
    }
    h.render();
    let bounds = |h: &mut Harness, mark: &str| h.cx.debug_bounds(interned(mark)).expect("rendered");
    let strip = bounds(&mut h, "tile-strip");
    let tiles: Vec<_> = (0..3).map(|index| bounds(&mut h, &format!("strip-tile:{index}"))).collect();
    let widths: Vec<f32> = tiles.iter().map(|tile| f32::from(tile.size.width)).collect();
    assert!(widths.iter().all(|width| (width - widths[0]).abs() <= 1.), "equal shares: {widths:?}");
    assert!((f32::from(tiles[0].left()) - f32::from(strip.left())).abs() <= 0.5, "{:?} in {strip:?}", tiles[0]);
    assert!((f32::from(tiles[2].right()) - f32::from(strip.right())).abs() <= 0.5, "{:?} in {strip:?}", tiles[2]);
    fill_tiles(&mut h);
    h.render();
    let strip = bounds(&mut h, "tile-strip");
    let last = h.cx.debug_bounds(interned(&format!("strip-tile:{}", actions::MAX_TILES - 1)));
    assert!(
        last.is_none_or(|last| last.right() > strip.right()),
        "too many tiles for one row scroll instead: {last:?} in {strip:?}"
    );
}

fn shown(h: &mut Harness, mark: &'static str) -> bool {
    h.cx.debug_bounds(mark).is_some()
}

fn overflowing_strip(cx: &mut TestAppContext) -> Harness<'_> {
    let mut h = open_with(cx, true, notes(&["kittens"]));
    while let Some(tile) = h.read(|shelf| shelf.tiles.actions().next().map(|tile| tile.id)) {
        h.act(|shelf, _, cx| shelf.remove_action(tile, cx));
    }
    h.act(|shelf, _, cx| shelf.add_action(search(1000), cx)).expect("tile");
    h.render();
    assert!(!shown(&mut h, "strip-start") && !shown(&mut h, "strip-end"), "a strip that fits has no scroll buttons");
    fill_tiles(&mut h);
    h.render();
    assert!(shown(&mut h, "strip-end"), "tiles hidden on the right get a button");
    assert!(!shown(&mut h, "strip-start"), "nothing is hidden on the left yet");
    h
}

#[gpui::test]
fn the_strip_shows_a_scroll_button_on_each_side_that_hides_tiles(cx: &mut TestAppContext) {
    let mut h = overflowing_strip(cx);
    let position = h.center("strip-tile:0");
    h.cx.simulate_event(ScrollWheelEvent { position, delta: ScrollDelta::Lines(point(0., -1.)), ..Default::default() });
    h.settle();
    h.render();
    assert!(
        shown(&mut h, "strip-start") && shown(&mut h, "strip-end"),
        "a strip scrolled halfway hides tiles on both sides"
    );
}

#[gpui::test]
fn tiles_in_a_scrolling_strip_keep_their_whole_titles(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, notes(&["kittens"]));
    while let Some(tile) = h.read(|shelf| shelf.tiles.actions().next().map(|tile| tile.id)) {
        h.act(|shelf, _, cx| shelf.remove_action(tile, cx));
    }
    for name in ["Уменьшить изображение до размера", "Уменьшить изображение до размера экрана телефона и планшета"]
    {
        let mut action = search(name.len());
        action.name = name.into();
        h.act(|shelf, _, cx| shelf.add_action(action, cx)).expect("tile");
    }
    fill_tiles(&mut h);
    h.render();
    assert!(shown(&mut h, "strip-end"), "the strip scrolls");
    let width = |h: &mut Harness, index: usize| {
        h.cx.debug_bounds(interned(&format!("strip-tile:{index}"))).expect("rendered").size.width
    };
    assert!(width(&mut h, 1) > width(&mut h, 0), "a longer title makes a wider tile instead of being cut");
}

fn fully_seen(h: &mut Harness) -> Vec<usize> {
    let strip = h.cx.debug_bounds("tile-strip").expect("strip");
    let cover = |h: &mut Harness, mark: &'static str| if shown(h, mark) { crate::view::STRIP_COVER } else { 0. };
    let left = f32::from(strip.left()) + cover(h, "strip-start") - 0.5;
    let right = f32::from(strip.right()) - cover(h, "strip-end") + 0.5;
    (0..actions::MAX_TILES)
        .filter(|index| {
            h.cx.debug_bounds(interned(&format!("strip-tile:{index}")))
                .is_some_and(|tile| f32::from(tile.left()) >= left && f32::from(tile.right()) <= right)
        })
        .collect()
}

fn page_through(h: &mut Harness, button: &'static str) -> Vec<usize> {
    let mut seen = fully_seen(h);
    let mut clicks = 0;
    while shown(h, button) {
        clicks += 1;
        assert!(clicks <= actions::MAX_TILES, "{button} keeps the strip moving");
        click(h, button);
        h.render();
        seen.extend(fully_seen(h));
    }
    assert!(clicks > 1, "{button} pages through the strip instead of jumping to its end");
    seen.sort_unstable();
    seen.dedup();
    seen
}

#[gpui::test]
fn scroll_buttons_page_through_the_strip_without_skipping_a_tile(cx: &mut TestAppContext) {
    let mut h = overflowing_strip(cx);
    h.fake.take_calls();
    h.fake.state().pasted.push_back(Ok(Payload::Text("puppies".into())));
    let every: Vec<usize> = (0..actions::MAX_TILES).collect();
    assert_eq!(page_through(&mut h, "strip-end"), every, "every tile is shown whole on the way to the end");
    assert!(shown(&mut h, "strip-start"), "the tiles on the left can be brought back");
    assert_eq!(page_through(&mut h, "strip-start"), every, "every tile is shown whole on the way back");
    assert!(shown(&mut h, "strip-end"));
    assert!(!h.fake.calls().iter().any(|call| matches!(call, Call::ShellOpen(_))), "no tile under a button runs");
    assert_eq!(h.running(), 0);
}

#[gpui::test]
fn fitting_the_window_widens_it_until_the_strip_stops_scrolling(cx: &mut TestAppContext) {
    let mut h = overflowing_strip(cx);
    h.fake.take_calls();
    h.send(Event::FitSize);
    let (default_width, default_height) = store::DEFAULT_SIZE;
    let calls = h.fake.take_calls();
    let &[Call::Resize(width, height)] = calls.as_slice() else { panic!("one resize, got {calls:?}") };
    assert!(width > default_width, "the window grows wider than the default: {width}");
    assert_eq!(height, default_height);
    assert_eq!(h.saved().settings().size, Some((width, height)));
    h.cx.simulate_resize(size(px(width), px(height)));
    h.settle();
    h.render();
    assert!(!shown(&mut h, "strip-start") && !shown(&mut h, "strip-end"), "every tile fits without scrolling");
}

#[gpui::test]
fn fitting_a_too_wide_window_narrows_it_to_the_tiles(cx: &mut TestAppContext) {
    let mut h = overflowing_strip(cx);
    let resize = |h: &mut Harness, width: f32| {
        h.cx.simulate_resize(size(px(width), px(store::DEFAULT_SIZE.1)));
        h.settle();
        h.render();
    };
    let fit = |h: &mut Harness| {
        h.fake.take_calls();
        h.send(Event::FitSize);
        let calls = h.fake.take_calls();
        let &[Call::Resize(width, _)] = calls.as_slice() else { panic!("one resize, got {calls:?}") };
        width
    };
    let fitted = fit(&mut h);
    resize(&mut h, fitted + 300.);
    assert!(!shown(&mut h, "strip-end"), "a wider window has room for every tile");
    assert_eq!(fit(&mut h), fitted, "fitting a wider window narrows it back to the tiles");
    let last = interned(&format!("strip-tile:{}", actions::MAX_TILES - 1));
    let ends = |h: &mut Harness| {
        let strip = h.cx.debug_bounds("tile-strip").expect("strip");
        h.cx.debug_bounds(last).map(|tile| (tile.right(), strip.right()))
    };
    let device = h.cx.update(|window, _| 1. / window.scale_factor());
    resize(&mut h, fitted - device);
    assert!(
        ends(&mut h).is_none_or(|(tile, strip)| tile > strip),
        "one device pixel narrower and the last tile no longer fits: {:?}",
        ends(&mut h)
    );
    resize(&mut h, fitted);
    assert!(
        ends(&mut h).is_some_and(|(tile, strip)| tile <= strip),
        "the fitted width holds the last tile: {:?}",
        ends(&mut h)
    );
}

#[gpui::test]
fn fitting_a_strip_that_fits_the_default_width_restores_the_default_size(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, notes(&["kittens"]));
    while let Some(tile) = h.read(|shelf| shelf.tiles.actions().next().map(|tile| tile.id)) {
        h.act(|shelf, _, cx| shelf.remove_action(tile, cx));
    }
    h.act(|shelf, _, cx| shelf.add_action(search(1000), cx)).expect("tile");
    h.send(Event::Placed { x: 0, y: 0, width: 900., height: 700. });
    h.cx.simulate_resize(size(px(900.), px(700.)));
    h.render();
    h.fake.take_calls();
    h.send(Event::FitSize);
    let (width, height) = store::DEFAULT_SIZE;
    assert_eq!(h.fake.take_calls(), [Call::Resize(width, height)]);
    assert_eq!(h.saved().settings().size, None);
}

fn with_icon_plugin(_: &Sandbox, store: &mut Store, _: &Fake) {
    store.add_text("kittens".into()).expect("note");
    plant_plugin(
        store.storage(),
        r#"{"id": "shrink", "name": "Уменьшить фото", "entry": "main.ps1", "icon": "icon.svg"}"#,
    );
    let icon = store.storage().root().join("actions").join("custom").join("shrink").join("icon.svg");
    let svg = "<svg xmlns='http://www.w3.org/2000/svg' width='24' height='24'><circle cx='12' cy='12' r='9'/></svg>";
    std::fs::write(icon, svg).expect("icon");
}

#[gpui::test]
fn the_strip_fills_the_width_evenly_or_from_each_tile_and_scrolls_only_when_they_do_not_fit(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, with_icon_plugin);
    while let Some(tile) = h.read(|shelf| shelf.tiles.actions().next().map(|tile| tile.id)) {
        h.act(|shelf, _, cx| shelf.remove_action(tile, cx));
    }
    h.act(|shelf, _, cx| {
        let plugin = shelf.plugins().first().expect("installed plugin").clone();
        shelf.add_action(Action::new(actions::Kind::Custom(actions::Custom::new(plugin))), cx)
    })
    .expect("plugin tile");
    for (index, name) in ["Отправить файл", "Скачать видео", "QR"].iter().enumerate() {
        let action = Action { name: (*name).into(), ..search(index) };
        h.act(|shelf, _, cx| shelf.add_action(action, cx)).expect("tile");
    }
    let layout = |h: &mut Harness, width: f32| {
        h.cx.simulate_resize(size(px(width), px(540.)));
        h.settle();
        h.render();
        let strip = h.cx.debug_bounds("tile-strip").expect("strip");
        let tiles: Vec<Bounds<Pixels>> = (0..4)
            .map(|index| h.cx.debug_bounds(interned(&format!("strip-tile:{index}"))).expect("strip tile"))
            .collect();
        (strip, tiles)
    };
    let widths = |tiles: &[Bounds<Pixels>]| tiles.iter().map(|tile| f32::from(tile.size.width)).collect::<Vec<_>>();
    let ends = |tiles: &[Bounds<Pixels>], strip: Bounds<Pixels>| {
        (f32::from(tiles[3].right()) - f32::from(strip.right())).abs() <= 0.5
    };
    let narrow = NARROW_SIZE.0;
    let (strip, natural) = layout(&mut h, narrow);
    assert!(natural[3].right() > strip.right(), "four tiles do not fit the narrowest window");
    let own = widths(&natural);
    let margins = narrow - f32::from(strip.size.width);
    let gaps = f32::from(natural[1].left() - natural[0].right()) * 3.;
    let total: f32 = own.iter().sum::<f32>() + gaps;
    let even = own.iter().copied().fold(0., f32::max) * 4. + gaps;
    let kept = |tiles: &[Bounds<Pixels>]| widths(tiles).iter().zip(&own).all(|(width, own)| width >= &(own - 0.5));

    let (strip, tiles) = layout(&mut h, total + margins - 2.);
    assert!(widths(&tiles).iter().zip(&own).all(|(width, own)| (width - own).abs() <= 0.5), "too narrow: own widths");
    assert!(tiles[3].right() > strip.right(), "too narrow: the row scrolls {tiles:?} in {strip:?}");

    let (strip, tiles) = layout(&mut h, total + margins + 2.);
    assert!(ends(&tiles, strip) && kept(&tiles), "the row fits: tiles grow to the edge {tiles:?} in {strip:?}");
    assert!(tiles[3].size.width < tiles[0].size.width, "growing keeps a short tile shorter: {tiles:?}");

    let (strip, tiles) = layout(&mut h, even + margins - 2.);
    assert!(ends(&tiles, strip) && kept(&tiles) && tiles[3].size.width < tiles[0].size.width);

    let (strip, tiles) = layout(&mut h, even + margins + 2.);
    let shares = widths(&tiles);
    assert!(shares.iter().all(|width| (width - shares[0]).abs() <= 1.), "equal shares: {shares:?}");
    assert!(ends(&tiles, strip) && kept(&tiles), "equal shares keep every tile's content {tiles:?} in {strip:?}");
}
