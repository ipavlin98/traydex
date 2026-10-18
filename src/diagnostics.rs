use crate::actions::Action;
use crate::store::Item;
use gpui::{Bounds, Pixels};
use serde::Serialize;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

pub const FILE: &str = "diagnostics.json";

type Rect = [f32; 4];

thread_local! {
    static MARKS: RefCell<BTreeMap<String, Rect>> = const { RefCell::new(BTreeMap::new()) };
    static WRITTEN: RefCell<String> = const { RefCell::new(String::new()) };
}

pub fn enabled() -> bool {
    crate::profile::current().testing()
}

fn rect(bounds: Bounds<Pixels>) -> Rect {
    [bounds.origin.x.into(), bounds.origin.y.into(), bounds.size.width.into(), bounds.size.height.into()]
}

pub fn begin() {
    MARKS.with_borrow_mut(BTreeMap::clear);
}

pub fn record(name: impl Into<String>, bounds: Bounds<Pixels>) {
    MARKS.with_borrow_mut(|marks| marks.insert(name.into(), rect(bounds)));
}

#[derive(Serialize)]
pub struct Tile {
    pub title: String,
    pub action: Action,
}

#[derive(Serialize)]
pub struct State {
    pub pid: u32,
    pub page: &'static str,
    pub layout: &'static str,
    pub dark: bool,
    pub language: crate::i18n::Language,
    pub scale: f32,
    pub size: [f32; 2],
    pub dropping: bool,
    pub over: Option<String>,
    pub pinned: bool,
    pub alert: Option<String>,
    pub jobs: Vec<String>,
    pub selected: usize,
    pub preview: Option<String>,
    pub preview_loaded: bool,
    pub items: Vec<Item>,
    pub tiles: Vec<Tile>,
    pub marks: BTreeMap<String, Rect>,
}

pub fn publish(root: &Path, mut state: State, targets: &[(Bounds<Pixels>, usize)], zone: Option<Bounds<Pixels>>) {
    state.marks = MARKS.take();
    for (bounds, index) in targets {
        state.marks.insert(format!("tile:{index}"), rect(*bounds));
    }
    if let Some(zone) = zone {
        state.marks.insert("zone".into(), rect(zone));
    }
    let Ok(text) = serde_json::to_string_pretty(&state) else {
        return;
    };
    if WRITTEN.with_borrow(|written| *written == text) {
        return;
    }
    let temp = root.join("diagnostics.tmp");
    match fs::write(&temp, &text).and_then(|_| fs::rename(&temp, root.join(FILE))) {
        Ok(()) => WRITTEN.set(text),
        Err(error) => log::warn!("Couldn't write the diagnostics: {error}"),
    }
}

#[cfg(test)]
mod tests;
