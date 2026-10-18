use super::*;
use crate::i18n::Language;
use crate::store::Kind;
use crate::testing::{Sandbox, item};
use gpui::{point, px, size};

fn state(items: Vec<Item>) -> State {
    State {
        pid: 7,
        page: "shelf",
        layout: "list",
        dark: false,
        language: Language::Russian,
        scale: 1.5,
        size: [540., 540.],
        dropping: false,
        over: None,
        pinned: false,
        alert: None,
        jobs: Vec::new(),
        selected: 0,
        preview: None,
        preview_loaded: false,
        items,
        tiles: Vec::new(),
        marks: BTreeMap::new(),
    }
}

fn bounds(x: f32, y: f32, width: f32, height: f32) -> Bounds<Pixels> {
    Bounds::new(point(px(x), px(y)), size(px(width), px(height)))
}

fn read(sandbox: &Sandbox) -> serde_json::Value {
    serde_json::from_slice(&fs::read(sandbox.join(FILE)).expect("diagnostics")).expect("json")
}

#[test]
fn diagnostics_are_off_without_the_test_profile() {
    assert!(!enabled());
}

#[test]
fn published_state_carries_items_marks_tiles_and_zone() {
    let sandbox = Sandbox::new();
    begin();
    record("settings", bounds(10., 5., 24., 24.));
    let note = item(Kind::Text, "Заметка", sandbox.join("note.txt"), Some("Заметка"));
    publish(sandbox.path(), state(vec![note]), &[(bounds(14., 100., 80., 44.), 2)], Some(bounds(14., 50., 300., 40.)));
    let json = read(&sandbox);
    assert_eq!(json["page"], "shelf");
    assert_eq!(json["language"], "Russian");
    assert_eq!(json["items"][0]["title"], "Заметка");
    assert_eq!(json["items"][0]["kind"], "Text");
    assert_eq!(json["marks"]["settings"], serde_json::json!([10., 5., 24., 24.]));
    assert_eq!(json["marks"]["tile:2"], serde_json::json!([14., 100., 80., 44.]));
    assert_eq!(json["marks"]["zone"], serde_json::json!([14., 50., 300., 40.]));
    assert!(!sandbox.join("diagnostics.tmp").exists());
}

#[test]
fn marks_belong_to_one_frame() {
    let sandbox = Sandbox::new();
    begin();
    record("old", bounds(0., 0., 1., 1.));
    begin();
    record("new", bounds(0., 0., 2., 2.));
    publish(sandbox.path(), state(Vec::new()), &[], None);
    let json = read(&sandbox);
    assert!(json["marks"].get("old").is_none());
    assert!(json["marks"].get("new").is_some());
    publish(sandbox.path(), state(Vec::new()), &[], None);
    assert!(read(&sandbox)["marks"].as_object().expect("marks").is_empty());
}

#[test]
fn unchanged_state_is_not_rewritten() {
    let sandbox = Sandbox::new();
    begin();
    publish(sandbox.path(), state(Vec::new()), &[], None);
    fs::write(sandbox.join(FILE), "touched").expect("touch");
    publish(sandbox.path(), state(Vec::new()), &[], None);
    assert_eq!(fs::read_to_string(sandbox.join(FILE)).expect("file"), "touched");
    let mut changed = state(Vec::new());
    changed.page = "settings";
    publish(sandbox.path(), changed, &[], None);
    assert_eq!(read(&sandbox)["page"], "settings");
}
