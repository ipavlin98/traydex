use super::*;
use crate::actions::Custom;
use crate::design::size;
use crate::system::{Offer, Offered};
use crate::testing::{archive, plant_plugin};
use serde_json::{Value, json};

const MANIFEST: &str = r#"{
    "id": "resize",
    "name": "Уменьшить фото",
    "entry": "main.ps1",
    "accepts": {"extensions": ["png"]},
    "timeout": 5,
    "settings": [
        {"key": "quality", "label": "Качество", "type": "text"},
        {"key": "keep", "label": "Оставить оригинал", "type": "toggle"},
        {"key": "target", "label": "Папка", "type": "folder"}
    ]
}"#;

fn with_plugin(_: &Sandbox, store: &mut Store, _: &Fake) {
    plant_plugin(store.storage(), MANIFEST);
}

fn add_plugin_tile(h: &mut Harness) -> TileId {
    h.act(|shelf, _, cx| {
        let plugin = shelf.plugins().first().expect("installed plugin").clone();
        shelf.add_action(Action::new(actions::Kind::Custom(Custom::new(plugin))), cx)
    })
    .expect("tile")
}

#[gpui::test]
fn dropped_bundle_installs_a_plugin_while_other_files_land_on_the_shelf(cx: &mut TestAppContext) {
    let _ru = language(Language::Russian);
    let mut h = open(cx, true);
    let bundle = h.sandbox.join("Сжатие фото.TDX");
    archive(&bundle, &[("action.json", MANIFEST.as_bytes()), ("main.ps1", b"exit 0")]);
    let note = h.sandbox.file("note.txt", "x");
    h.send(Event::Drop(Payload::Paths(vec![bundle, note])));
    assert_eq!(h.titles(), ["note.txt"], "the bundle itself isn't kept on the shelf");
    let installed: Vec<String> = h.read(|shelf| shelf.plugins().iter().map(|plugin| plugin.id().to_string()).collect());
    assert_eq!(installed, ["resize"]);
    let tile = h.read(|shelf| shelf.tiles.find(&actions::Slot::Custom("resize".into()))).expect("the tile is added");
    assert_eq!(h.read(|shelf| shelf.editor().map(|editor| editor.tile)), Some(tile), "its settings open at once");
    assert_eq!(h.toast(), Some(fill(tr().plugin_installed, "Уменьшить фото")));
}

#[gpui::test]
fn reinstalled_plugin_opens_the_settings_of_its_existing_tile(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, with_plugin);
    let tile = add_plugin_tile(&mut h);
    let tiles = h.read(|shelf| shelf.tiles.len());
    let bundle = h.sandbox.join("resize.tdx");
    archive(&bundle, &[("action.json", MANIFEST.as_bytes()), ("main.ps1", b"exit 0")]);
    h.send(Event::Drop(Payload::Paths(vec![bundle])));
    assert_eq!(h.read(|shelf| shelf.tiles.len()), tiles, "no second tile");
    assert_eq!(h.read(|shelf| shelf.editor().map(|editor| editor.tile)), Some(tile));
}

#[gpui::test]
fn several_dropped_plugins_are_counted_and_open_the_actions_page(cx: &mut TestAppContext) {
    let _ru = language(Language::Russian);
    let mut h = open(cx, true);
    let tiles = h.read(|shelf| shelf.tiles.len());
    let bundles: Vec<PathBuf> = ["a", "b"]
        .into_iter()
        .map(|id| {
            let bundle = h.sandbox.join(format!("{id}.tdx"));
            let manifest = format!(r#"{{"id": "{id}", "name": "Плагин {id}", "entry": "main.ps1"}}"#);
            archive(&bundle, &[("action.json", manifest.as_bytes()), ("main.ps1", b"exit 0")]);
            bundle
        })
        .collect();
    h.send(Event::Drop(Payload::Paths(bundles)));
    assert_eq!(h.read(|shelf| shelf.plugins().len()), 2);
    assert_eq!(h.read(|shelf| shelf.page()), Page::Actions);
    assert_eq!(h.read(|shelf| shelf.tiles.len()), tiles, "the user picks which of them become tiles");
    assert_eq!(h.toast(), Some(fill(tr().plugins_installed, 2)));
}

#[gpui::test]
fn plugin_is_uninstalled_from_its_tile_after_confirmation_together_with_the_tile(cx: &mut TestAppContext) {
    let _ru = language(Language::Russian);
    let mut h = open_with(cx, true, with_plugin);
    let tile = add_plugin_tile(&mut h);
    h.act(|shelf, window, cx| shelf.open_editor(tile, None, window, cx));
    h.act(|shelf, _, cx| shelf.ask_uninstall("resize", cx));
    h.act(|shelf, _, cx| shelf.cancel_confirm(cx));
    assert_eq!(h.read(|shelf| shelf.plugins().len()), 1, "nothing is deleted without confirmation");

    h.act(|shelf, _, cx| shelf.ask_uninstall("resize", cx));
    h.act(|shelf, _, cx| shelf.confirm(cx));
    assert!(h.read(|shelf| shelf.plugins().is_empty()));
    assert!(!h.sandbox.storage().root().join("actions").join("custom").join("resize").exists());
    assert!(h.read(|shelf| shelf.tiles.get(tile).is_none()), "its tile goes with it");
    assert_eq!(h.read(|shelf| shelf.page()), Page::Actions);
    assert_eq!(h.toast(), Some(fill(tr().plugin_uninstalled, "Уменьшить фото")));
}

#[gpui::test]
fn plugin_without_a_tile_is_uninstalled_from_its_card(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, with_plugin);
    h.act(|shelf, _, cx| shelf.toggle_page(Page::Actions, cx));
    let tiles = h.read(|shelf| shelf.tiles.len());
    h.act(|shelf, _, cx| shelf.ask_uninstall("resize", cx));
    h.act(|shelf, _, cx| shelf.confirm(cx));
    assert!(h.read(|shelf| shelf.plugins().is_empty()));
    assert_eq!(h.read(|shelf| shelf.tiles.len()), tiles, "other tiles stay");
    assert_eq!(h.read(|shelf| shelf.page()), Page::Actions);
}

#[gpui::test]
fn broken_bundle_is_reported_and_installs_nothing(cx: &mut TestAppContext) {
    let _ru = language(Language::Russian);
    let mut h = open(cx, true);
    let bundle = h.sandbox.file("broken.tdx", "not a zip");
    h.send(Event::Drop(Payload::Paths(vec![bundle])));
    assert!(h.alert().is_some_and(|alert| alert.starts_with(&fill(tr().plugin_install_failed, "broken.tdx"))));
    assert!(h.read(|shelf| shelf.plugins().is_empty()));
    assert!(h.titles().is_empty());
}

#[gpui::test]
fn plugin_tile_refuses_drags_it_cannot_take(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, with_plugin);
    let tile = add_plugin_tile(&mut h);
    let index = h.read(|shelf| shelf.tiles.position(tile)).expect("position");
    h.send(Event::Offer(Offer::Text));
    h.send(Event::Hover(true));
    let (x, y) = h.tile_center(index);
    h.send(Event::Pointer(x, y));
    assert!(h.read(|shelf| shelf.refuses(tile)));
    assert_eq!(h.fake.calls().last(), Some(&Call::Reject(true)));

    h.send(Event::Offer(Offer::Files(vec![Offered { path: r"C:\Фото\a.PNG".into(), folder: false }])));
    h.send(Event::Pointer(x + 1., y));
    assert_eq!(h.fake.calls().last(), Some(&Call::Reject(false)));

    h.send(Event::Offer(Offer::Text));
    h.send(Event::DragStart);
    assert!(!h.read(|shelf| shelf.refuses(tile)), "a new drag forgets the previous one");
}

#[gpui::test]
fn plugin_that_outlives_its_timeout_is_stopped_and_reported(cx: &mut TestAppContext) {
    let _ru = language(Language::Russian);
    let mut h = open_with(cx, true, with_plugin);
    let tile = add_plugin_tile(&mut h);
    let photo = h.sandbox.file("a.png", "png");
    h.pending(|shelf, _, cx| shelf.perform(tile, Payload::Paths(vec![photo]), cx));
    h.cx.executor().advance_clock(Duration::from_secs(4));
    h.cx.run_until_parked();
    let running = h.read_cx(|shelf, cx| shelf.jobs.read(cx).for_tile(tile).map(|job| job.progress.cancelled()));
    assert_eq!(running, Some(false), "the tile shows its job until the deadline");

    h.elapse(Duration::from_secs(1));
    assert_eq!(h.running(), 0);
    let alert = h.alert().expect("alert");
    assert!(alert.contains(&fill(tr().plugin_timeout, 5)), "{alert}");
}

#[gpui::test]
fn plugin_settings_get_editor_fields_and_are_saved(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, with_plugin);
    let tile = add_plugin_tile(&mut h);
    h.act(|shelf, window, cx| shelf.open_editor(tile, None, window, cx));
    let fields: Vec<(String, Entity<crate::input::TextInput>)> =
        h.read(|shelf| shelf.editor().expect("editor").settings.clone());
    let keys: Vec<&str> = fields.iter().map(|(key, _)| key.as_str()).collect();
    assert_eq!(keys, ["quality", "target"], "a toggle needs no text field");

    let (_, quality) = fields.first().expect("quality field").clone();
    h.act(|_, window, cx| quality.read(cx).focus(window));
    h.cx.simulate_input("90 %");
    h.act(|shelf, _, cx| shelf.set_value(tile, "keep", Value::Bool(true), cx));
    let folder = h.sandbox.folder("Готовые фото");
    h.fake.state().picked.push_back(Ok(Some(folder.clone())));
    h.act(|shelf, _, cx| shelf.pick_value(tile, "target".into(), true, cx));

    let (_, target) = fields.get(1).expect("target field").clone();
    assert_eq!(h.cx.update(|_, cx| target.read(cx).text().to_string()), folder.display().to_string());
    let saved = saved_tiles(&h).pop().expect("saved tile");
    let json = serde_json::to_value(&saved).expect("json");
    assert_eq!(json["values"], json!({"quality": "90 %", "keep": true, "target": folder}));
}

#[gpui::test]
fn clicks_around_the_uninstall_button_up_to_the_card_border_are_taken_by_the_button(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, with_plugin);
    h.act(|shelf, window, cx| {
        window.focus(&shelf.focus);
        shelf.toggle_page(Page::Actions, cx);
    });
    h.keys("/");
    h.cx.simulate_input("Уменьшить");
    h.cx.update(|window, _| window.refresh());
    h.cx.run_until_parked();
    let tiles = h.read(|shelf| shelf.tiles.len());
    let card = h.cx.debug_bounds("chip:plugin:resize").expect("the plugin card is rendered");
    let border = px(size::HAIRLINE / 2.);
    let around = [
        ("the padding", point(card.right() - px(2.), card.top() + px(3.))),
        ("the right border", point(card.right() - border, card.center().y)),
        ("the top border", point(card.right() - px(3.), card.top() + border)),
        ("the bottom border", point(card.right() - px(3.), card.bottom() - border)),
    ];
    for (place, at) in around {
        h.cx.simulate_click(at, Modifiers::default());
        h.settle();
        assert_eq!(h.read(|shelf| shelf.confirming.clone()), Some(Confirm::Uninstall("resize".into())), "{place}");
        assert_eq!(h.read(|shelf| shelf.tiles.len()), tiles, "the card itself is not toggled by {place}");
        h.act(|shelf, _, cx| shelf.cancel_confirm(cx));
    }
}
