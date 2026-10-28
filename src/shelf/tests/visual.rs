use super::*;
use crate::design::space;
use crate::i18n::Language;
use gpui::{Bounds, size};

const SIZES: [(f32, f32); 2] = [NARROW_SIZE, store::DEFAULT_SIZE];
const EPS: f32 = 0.5;
const TITLE_ITEMS: [&str; 9] =
    ["home", "pin", "actions", "settings", "hide", "alert", "toast-icon", "jobs-stop", "jobs"];
const SWITCHES: [&str; 10] = [
    "blur",
    "window-animations",
    // "interface-animations",
    "automatic",
    "corner-drag",
    "autostart",
    "tray-icon",
    "context-menu",
    "double-click-preview",
    "separate-notes",
    "check-updates-auto",
];
const SEGMENTS: [&str; 4] = ["theme", "transparency", "shake", "corner"];
const CHIPS: [&str; 20] = [
    "add-folder",
    "add-run",
    "add-search",
    "add-upload",
    "add-zip",
    "add-unpack",
    "add-jpeg",
    "add-png",
    "add-imgur",
    "add-gif",
    "add-path",
    "add-phone",
    "add-base64",
    "add-hash",
    "add-ocr",
    "add-compress",
    "add-pdf",
    "add-rename",
    "add-qr",
    "add-ytdlp",
];
const SHARE_PARTS: [&str; 5] = ["share-code", "share-files", "share-url", "share-hint", "share-stop"];
const QR_PARTS: [&str; 3] = ["qr-code", "qr-text", "qr-buttons"];
const QR_BUTTONS: [&str; 3] = [super::super::QR_COPY, "qr-keep", "qr-done"];
const RENAME_PARTS: [&str; 5] = ["rename-title", "rename-rules", "rename-name", "rename-preview", "rename-buttons"];
const MEDIA_PARTS: [&str; 5] = ["media-header", "media-mode", "media-quality", "media-extras", "media-buttons"];
const MEDIA_SWITCHES: [&str; 4] = ["media-playlist", "media-subtitles", "media-cover", "media-sponsors"];
const PASSWORD_PARTS: [&str; 3] = ["password-header", "password-field", "password-buttons"];
const LONG_NAME: &str = "очень-длинное-имя-файла-для-проверки-обрезки-в-узком-окне-2026";

#[derive(Clone, Copy, Debug)]
enum Scene {
    Empty,
    List,
    Grid,
    GridSelection,
    Selection,
    Notes,
    Search,
    Preview,
    Note,
    Alert,
    Job,
    Confirm,
    Settings,
    Languages,
    Actions,
    FullActions,
    Editor,
    Drop,
    FullDrop,
    Lend,
    FullLend,
    Share,
    SharePending,
    Hashes,
    HashesPending,
    Qr,
    QrPending,
    Rename,
    RenamePending,
    Media,
    MediaPending,
    Password,
    Toast,
    ToastBar,
    AlertBar,
    HashEditor,
    OcrEditor,
    VideoEditor,
    PluginEditor,
}

const PLUGINS: [&str; 2] = ["long-a", "long-b"];

fn plant_long_plugins(store: &Store) {
    for id in PLUGINS {
        let manifest = serde_json::json!({
            "id": id,
            "name": format!("{LONG_NAME} {id}"),
            "description": format!("{LONG_NAME} {LONG_NAME}"),
            "entry": "main.ps1",
            "settings": [
                {"key": "token", "label": format!("{LONG_NAME} token"), "type": "secret"},
                {"key": "keep", "label": format!("{LONG_NAME} {LONG_NAME}"), "type": "toggle"},
                {"key": "mode", "label": LONG_NAME, "type": "text", "placeholder": LONG_NAME},
                {"key": "target", "label": LONG_NAME, "type": "folder"},
            ],
        });
        crate::testing::plant_plugin(store.storage(), &manifest.to_string());
    }
}

fn rich(sandbox: &Sandbox, store: &mut Store, _: &Fake) {
    plant_long_plugins(store);
    store.add_paths(vec![
        sandbox.file("quarterly-report-final-version-2026.txt", "x"),
        sandbox.file("photo.png", png(3, 3)),
        sandbox.folder("Projects"),
    ]);
    store.add_text("https://example.com/a/very/long/path/to/some/page".into()).expect("link");
    store
        .add_text("Длинная заметка с несколькими словами, которая точно не помещается в одну строку".into())
        .expect("note");
    for index in 0..15 {
        store.add_text(format!("note {index}")).expect("note");
    }
    store.watch(sandbox.join("Очень длинное имя папки/которое не помещается в одну строку настроек/Загрузки"));
}

struct Check<'a, 'b> {
    h: &'a mut Harness<'b>,
    window: Bounds<Pixels>,
    context: String,
    failures: Vec<String>,
}

impl Check<'_, '_> {
    fn find(&mut self, name: &str) -> Option<Bounds<Pixels>> {
        self.h.cx.debug_bounds(interned(name))
    }

    fn fail(&mut self, message: String) {
        self.failures.push(format!("{}: {message}", self.context));
    }

    fn get(&mut self, name: &str) -> Option<Bounds<Pixels>> {
        let found = self.find(name);
        match found {
            None => self.fail(format!("{name} is not rendered")),
            Some(bounds) if f32::from(bounds.size.width) <= 0. || f32::from(bounds.size.height) <= 0. => {
                self.fail(format!("{name} is empty: {bounds:?}"))
            }
            Some(_) => {}
        }
        found
    }

    fn inside(&mut self, inner: &str, outer: &str, vertical: bool) {
        let (Some(a), Some(b)) = (self.get(inner), self.get(outer)) else {
            return;
        };
        self.contained(inner, a, outer, b, vertical);
    }

    fn in_window(&mut self, name: &str, vertical: bool) {
        let window = self.window;
        if let Some(bounds) = self.get(name) {
            self.contained(name, bounds, "window", window, vertical);
        }
    }

    fn contained(&mut self, inner: &str, a: Bounds<Pixels>, outer: &str, b: Bounds<Pixels>, vertical: bool) {
        let fits_x =
            f32::from(a.left()) >= f32::from(b.left()) - EPS && f32::from(a.right()) <= f32::from(b.right()) + EPS;
        let fits_y =
            f32::from(a.top()) >= f32::from(b.top()) - EPS && f32::from(a.bottom()) <= f32::from(b.bottom()) + EPS;
        if !fits_x || (vertical && !fits_y) {
            self.fail(format!("{inner} {a:?} leaves {outer} {b:?}"));
        }
    }

    fn apart(&mut self, names: &[String]) {
        let found: Vec<(String, Bounds<Pixels>)> =
            names.iter().filter_map(|name| Some((name.clone(), self.get(name)?))).collect();
        for (index, (a_name, a)) in found.iter().enumerate() {
            for (b_name, b) in &found[index + 1..] {
                let separate = f32::from(a.right()) <= f32::from(b.left()) + EPS
                    || f32::from(b.right()) <= f32::from(a.left()) + EPS
                    || f32::from(a.bottom()) <= f32::from(b.top()) + EPS
                    || f32::from(b.bottom()) <= f32::from(a.top()) + EPS;
                if !separate {
                    self.fail(format!("{a_name} {a:?} overlaps {b_name} {b:?}"));
                }
            }
        }
    }

    fn right_aligned(&mut self, name: &str, other: &str) {
        let (Some(a), Some(b)) = (self.get(name), self.get(other)) else {
            return;
        };
        if (f32::from(a.right()) - f32::from(b.right())).abs() > EPS {
            self.fail(format!("{name} {a:?} isn't right-aligned with {other} {b:?}"));
        }
    }

    fn present(&mut self, names: impl IntoIterator<Item = String>) -> Vec<String> {
        names.into_iter().filter(|name| self.find(name).is_some()).collect()
    }

    fn right_above(&mut self, name: &str, other: &str) {
        let (Some(a), Some(b)) = (self.get(name), self.get(other)) else {
            return;
        };
        if (f32::from(a.bottom()) - f32::from(b.top())).abs() > EPS {
            self.fail(format!("{name} {a:?} doesn't sit right above {other} {b:?}"));
        }
    }

    fn at_bottom(&mut self, name: &str) {
        let height = f32::from(self.window.bottom());
        if let Some(bounds) = self.get(name)
            && (f32::from(bounds.bottom()) - height).abs() > EPS
        {
            self.fail(format!("{name} {bounds:?} isn't at the bottom edge {height}"));
        }
    }
}

fn lend(h: &mut Harness) {
    h.act(|shelf, _, cx| {
        shelf.press_all(&mouse_down(point(px(0.), px(0.)), 1));
        shelf.track(&mouse_move(point(px(0.), px(40.)), Some(MouseButton::Left)), cx);
    });
}

fn check_strip(check: &mut Check, neighbors: &[&str]) {
    check.in_window("tile-strip", true);
    let neighbors = check.present(neighbors.iter().map(ToString::to_string));
    for neighbor in neighbors {
        check.apart(&["tile-strip".into(), neighbor]);
    }
    let tiles = check.present(names(0..actions::MAX_TILES, "strip-tile:"));
    match tiles.first() {
        Some(first) => check.inside(&first.clone(), "tile-strip", true),
        None => check.fail("the tile strip has no tiles".into()),
    }
    check.apart(&tiles);
    for button in ["strip-start", "strip-end"] {
        if check.find(button).is_some() {
            check.inside(button, "tile-strip", true);
        }
    }
}

fn names(items: impl IntoIterator<Item = impl std::fmt::Display>, prefix: &str) -> Vec<String> {
    items.into_iter().map(|item| format!("{prefix}{item}")).collect()
}

fn prepare(h: &mut Harness, scene: Scene) {
    let ids = h.ids();
    match scene {
        Scene::Empty => {}
        Scene::List => h.act(|shelf, _, cx| shelf.toggle_stuck(&ids[0], cx)),
        Scene::Grid => h.act(|shelf, _, cx| {
            shelf.toggle_layout(cx);
            shelf.toggle_stuck(&ids[0], cx);
        }),
        Scene::GridSelection => h.act(|shelf, _, cx| {
            shelf.toggle_layout(cx);
            shelf.toggle(&ids[0], cx);
        }),
        Scene::Selection => h.act(|shelf, _, cx| {
            shelf.toggle(&ids[0], cx);
            shelf.toggle(&ids[1], cx);
        }),
        Scene::Notes => h.act(|shelf, _, cx| shelf.set_filter(Filter::Notes, cx)),
        Scene::Search => {
            h.keys("/");
            h.cx.simulate_input("о");
            h.settle();
        }
        Scene::Preview => h.act(|shelf, window, cx| shelf.show_preview(ids[ids.len() - 3].clone(), window, cx)),
        Scene::Note => {
            h.act(|shelf, window, cx| shelf.new_note(window, cx));
            h.cx.simulate_input(&format!("{LONG_NAME} {LONG_NAME}"));
            h.act(|shelf, window, cx| shelf.focus_title(window, cx));
        }
        Scene::Alert => h.act(|shelf, _, cx| shelf.fail("visible alert", cx)),
        Scene::Job => {
            h.act(|shelf, _, cx| shelf.fail("visible alert", cx));
            h.pending(|shelf, _, cx| shelf.install(Tool::Ffmpeg, Slot::Single(Single::Gif), cx));
        }
        Scene::Confirm => h.act(|shelf, _, cx| shelf.ask_clear(cx)),
        Scene::Settings => h.act(|shelf, _, cx| shelf.toggle_page(Page::Settings, cx)),
        Scene::Languages => h.act(|shelf, _, cx| {
            shelf.toggle_page(Page::Settings, cx);
            shelf.toggle_menu(Dropdown::Language, cx);
        }),
        Scene::Actions => h.act(|shelf, _, cx| shelf.toggle_page(Page::Actions, cx)),
        Scene::FullActions => {
            fill_with_long_names(h);
            h.act(|shelf, _, cx| shelf.toggle_page(Page::Actions, cx));
        }
        Scene::FullDrop => {
            fill_with_long_names(h);
            h.send(Event::Hover(true));
        }
        Scene::Editor => {
            let first = h.tile(0);
            h.act(|shelf, window, cx| {
                shelf.toggle_page(Page::Actions, cx);
                shelf.open_editor(first, Some(Field::Name), window, cx);
            });
        }
        Scene::Drop => h.send(Event::Hover(true)),
        Scene::Lend => lend(h),
        Scene::FullLend => {
            fill_with_long_names(h);
            lend(h);
        }
        Scene::Hashes => {
            let index = add_tile(h, hash_tile());
            let files = vec![h.sandbox.file(&format!("{LONG_NAME}.iso"), "a"), h.sandbox.file("b.exe", "b")];
            let stats = r#"{"data":{"attributes":{"last_analysis_stats":{"malicious":3,"undetected":60}}}}"#;
            for status in ["200", "404"] {
                h.fake.state().outputs.push_back(Ok(crate::testing::output(0, &format!("{stats}\n{status}"), "")));
            }
            h.act(|shelf, _, cx| shelf.perform(index, Payload::Paths(files), cx));
        }
        Scene::Qr => {
            let (index, payload) = page_tile(h, Opened::Qr);
            h.act(|shelf, _, cx| shelf.perform(index, payload, cx));
        }
        Scene::Rename => {
            let (index, payload) = page_tile(h, Opened::Rename);
            h.act(|shelf, _, cx| shelf.perform(index, payload, cx));
            h.act(|shelf, window, cx| shelf.set_rule(actions::Rule::Name, window, cx));
        }
        Scene::Media => {
            let (index, payload) = page_tile(h, Opened::Media);
            h.act(|shelf, _, cx| shelf.perform(index, payload, cx));
        }
        Scene::Password => {
            let index = add_tile(h, actions::Kind::Unpack);
            let archives = vec![
                super::unpack::locked_zip(&h.sandbox, &format!("{LONG_NAME}.zip")),
                super::unpack::locked_zip(&h.sandbox, "b.zip"),
            ];
            h.act(|shelf, _, cx| shelf.perform(index, Payload::Paths(archives), cx));
        }
        Scene::VideoEditor => {
            let index = add_tile(h, actions::Kind::Ytdlp(actions::Fetch::default()));
            h.act(|shelf, window, cx| {
                shelf.toggle_page(Page::Actions, cx);
                shelf.open_editor(index, None, window, cx);
            });
            h.pending(|shelf, _, cx| shelf.install(Tool::Ffmpeg, Slot::Single(Single::Ytdlp), cx));
        }
        Scene::SharePending | Scene::HashesPending | Scene::QrPending | Scene::RenamePending | Scene::MediaPending => {
            let opened = match scene {
                Scene::SharePending => Opened::Share,
                Scene::HashesPending => Opened::Hashes,
                Scene::QrPending => Opened::Qr,
                Scene::MediaPending => Opened::Media,
                _ => Opened::Rename,
            };
            let (index, payload) = page_tile(h, opened);
            h.pending(|shelf, _, cx| shelf.perform(index, payload, cx));
        }
        Scene::Toast => {
            let file = h.sandbox.file(&format!("{LONG_NAME}.txt"), "x");
            let zip = h.tile(0);
            h.act(|shelf, _, cx| shelf.perform(zip, Payload::Paths(vec![file]), cx));
            h.act(|shelf, _, cx| shelf.show_toast(format!("{} {LONG_NAME}", tr().zipped), cx));
        }
        Scene::ToastBar => h.act(|shelf, _, cx| {
            shelf.toggle_page(Page::Settings, cx);
            shelf.show_toast(format!("{} {LONG_NAME}", tr().zipped), cx);
        }),
        Scene::AlertBar => h.act(|shelf, _, cx| {
            shelf.toggle_page(Page::Settings, cx);
            shelf.show_toast(tr().zipped.to_string(), cx);
            shelf.fail(format!("{LONG_NAME}: {LONG_NAME}"), cx);
        }),
        Scene::HashEditor => {
            let index = add_tile(h, hash_tile());
            h.act(|shelf, window, cx| {
                shelf.toggle_page(Page::Actions, cx);
                shelf.open_editor(index, Some(Field::ApiKey), window, cx);
            });
        }
        Scene::PluginEditor => {
            let index = h
                .act(|shelf, _, cx| {
                    let plugin = shelf.plugins().first().expect("planted plugin").clone();
                    let mut custom = actions::Custom::new(plugin);
                    custom.set("target", serde_json::Value::String(format!(r"C:\{LONG_NAME}\{LONG_NAME}")));
                    shelf.add_action(Action::new(actions::Kind::Custom(custom)), cx)
                })
                .expect("tile");
            h.act(|shelf, window, cx| {
                shelf.toggle_page(Page::Actions, cx);
                shelf.open_editor(index, None, window, cx);
            });
        }
        Scene::OcrEditor => {
            h.fake.state().languages = ["en-US", "ru", "zh-Hans-CN", "es-ES", "fr-FR"]
                .iter()
                .map(|tag| (tag.to_string(), format!("{LONG_NAME} {tag}")))
                .collect();
            let index = add_tile(h, actions::Kind::Ocr { language: "xx-Missing".into() });
            h.act(|shelf, window, cx| {
                shelf.toggle_page(Page::Actions, cx);
                shelf.open_editor(index, None, window, cx);
            });
        }
        Scene::Share => {
            let (index, payload) = page_tile(h, Opened::Share);
            h.act(|shelf, _, cx| shelf.perform(index, payload, cx));
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum Opened {
    Share,
    Hashes,
    Qr,
    Rename,
    Media,
}

fn page_tile(h: &mut Harness, opened: Opened) -> (actions::TileId, Payload) {
    let (kind, payload) = match opened {
        Opened::Share => (actions::Kind::Phone, Payload::Paths(vec![h.sandbox.file(&format!("{LONG_NAME}.pdf"), "x")])),
        Opened::Hashes => (
            hash_tile(),
            Payload::Paths(vec![h.sandbox.file(&format!("{LONG_NAME}.iso"), "a"), h.sandbox.file("b.exe", "b")]),
        ),
        Opened::Qr => {
            (actions::Kind::Qr, Payload::Text(format!("https://example.com/{LONG_NAME}/{LONG_NAME}?query={LONG_NAME}")))
        }
        Opened::Rename => (
            actions::Kind::Rename,
            Payload::Paths((0..7).map(|n| h.sandbox.file(&format!("{LONG_NAME} {n}.jpg"), "x")).collect()),
        ),
        Opened::Media => {
            crate::testing::plant_tools(&h.sandbox.storage());
            let info = crate::testing::output(0, &crate::testing::video_info(&format!("{LONG_NAME} {LONG_NAME}")), "");
            h.fake.state().outputs.push_back(Ok(info));
            let link = format!("https://www.youtube.com/watch?v={LONG_NAME}&list={LONG_NAME}");
            (actions::Kind::Ytdlp(actions::Fetch::default()), Payload::Text(link))
        }
    };
    (add_tile(h, kind), payload)
}

fn page_marks(opened: Opened) -> Vec<String> {
    match opened {
        Opened::Share => SHARE_PARTS.iter().chain(&["share-copy", "share-stop"]).map(|part| part.to_string()).collect(),
        Opened::Hashes => names(
            [
                "hash-card:0",
                "hash-card:1",
                "copy-sha256:0",
                "copy-md5:0",
                "hash-verdict:0",
                "hash-verdict:1",
                "hashes-done",
            ],
            "",
        ),
        Opened::Qr => QR_PARTS.iter().chain(&QR_BUTTONS).map(|part| part.to_string()).collect(),
        Opened::Rename => names(0..5, "rename-preview:")
            .into_iter()
            .chain(names(["rename-title", "rename-rules", "rename-preview", "rename-buttons", "rename-apply"], ""))
            .collect(),
        Opened::Media => names(MEDIA_PARTS, "")
            .into_iter()
            .chain(names(["media-title", "media-info", "media-download"], ""))
            .chain(names(0..7, "media-quality:"))
            .chain(names(0..4, "media-extra:"))
            .collect(),
    }
}

fn fill_with_long_names(h: &mut Harness) {
    h.act(|shelf, _, cx| {
        let mut index = 0;
        while !shelf.tiles.full() {
            let folder = PathBuf::from(format!(r"C:\{LONG_NAME}\{index}"));
            let name = format!("{LONG_NAME} {index}");
            shelf
                .add_action(Action { name, ..Action::new(actions::Kind::Move { folder }) }, cx)
                .expect("below the limit");
            index += 1;
        }
    });
}

fn hash_tile() -> actions::Kind {
    actions::Kind::Hash { algorithm: actions::Algorithm::Sha256, virustotal: "key".into() }
}

fn verify(check: &mut Check, scene: Scene, ids: &[ItemId]) {
    check.in_window("title-bar", true);
    let title: Vec<String> = check.present(TITLE_ITEMS.map(String::from));
    for name in &title {
        check.inside(name, "title-bar", true);
    }
    check.apart(&title);
    check.inside("title-buttons", "title-bar", true);
    check.apart(&["home".into(), "title-buttons".into()]);
    match scene {
        Scene::Empty => {
            check.in_window("drop-area", true);
            check.apart(&["title-bar".into(), "drop-area".into()]);
            check.inside("paste", "drop-area", true);
            check.inside("empty-note", "drop-area", true);
            check.apart(&names(["paste", "empty-note"], ""));
            check_strip(check, &["drop-area"]);
        }
        Scene::List
        | Scene::Grid
        | Scene::GridSelection
        | Scene::Selection
        | Scene::Alert
        | Scene::Job
        | Scene::Confirm
        | Scene::Notes
        | Scene::Search => {
            check.in_window("drop-area", true);
            check.apart(&["title-bar".into(), "drop-area".into(), "items".into()]);
            check_strip(check, &["drop-area", "tabs", "selection-bar", "items"]);
            let sides = check.present(["shelf-buttons".to_string(), "selection-bar".to_string()]);
            if sides.is_empty() {
                check.fail("neither shelf buttons nor the selection bar are rendered".into());
            }
            for side in &sides {
                check.in_window(side, true);
            }
            if sides.iter().any(|side| side == "shelf-buttons") {
                check.inside("new-note", "shelf-buttons", true);
            }
            if check.find("tabs").is_some() {
                check.in_window("tabs", true);
                check.inside("tab-area", "tabs", true);
                let active = match scene {
                    Scene::Notes => "tab-4",
                    Scene::Search => "tab-5",
                    _ => "tab-0",
                };
                check.inside(active, "tab-area", true);
                for button in ["tabs-start", "tabs-end"] {
                    if check.find(button).is_some() {
                        check.inside(button, "tab-area", true);
                    }
                }
                for side in &sides {
                    check.inside(side, "tabs", true);
                    check.apart(&["tab-area".into(), side.clone()]);
                }
            } else if !matches!(scene, Scene::Selection | Scene::GridSelection) {
                check.fail("tabs are not rendered".into());
            }
            check.in_window("items", false);
            let items = check.present(names(ids, "item:"));
            if items != names(ids.iter().take(items.len()), "item:") {
                check.fail(format!("rendered items {items:?} are not the first ones in display order"));
            }
            if !ids.is_empty() && items.len() < ids.len() {
                let last = items.last().and_then(|name| check.find(name));
                match (last, check.find("items")) {
                    (Some(last), Some(area)) if f32::from(last.bottom()) >= f32::from(area.bottom()) - EPS => {}
                    (last, area) => check.fail(format!("rendered items end at {last:?} before the list {area:?}")),
                }
            }
            for item in &items {
                check.inside(item, "items", false);
            }
            check.apart(&items);
            for badge in check.present(names(ids, "tile-badge:")) {
                let id = badge.replacen("tile-badge:", "", 1);
                check.inside(&badge, &format!("item:{id}"), true);
                let neighbors = check.present([badge.clone(), format!("item-buttons:{id}")]);
                check.apart(&neighbors);
            }
            if matches!(scene, Scene::GridSelection) && check.present(names(ids, "tile-badge:")).is_empty() {
                check.fail("the selected tile has no check badge".into());
            }
            let pinned = check.present(names(ids, "pin-mark:"));
            if matches!(scene, Scene::List | Scene::Grid) && pinned.is_empty() {
                check.fail("the pinned item has no pin mark".into());
            }
            for mark in pinned {
                let id = mark.replacen("pin-mark:", "", 1);
                check.inside(&mark, &format!("item:{id}"), true);
                check.right_aligned(&mark, &format!("item-buttons:{id}"));
            }
            for buttons in check.present(names(ids, "item-buttons:")) {
                let row = buttons.replacen("item-buttons:", "item:", 1);
                check.inside(&buttons, &row, true);
                if !matches!(scene, Scene::Grid | Scene::GridSelection) && check.find("shelf-buttons").is_some() {
                    check.right_aligned(&buttons, "shelf-buttons");
                }
            }
            check.in_window("status-bar", true);
            check.at_bottom("status-bar");
            if matches!(scene, Scene::Notes) {
                check.inside("first-note", "items", true);
                check.apart(&["items".into(), "status-bar".into()]);
            } else {
                check.in_window("drag-all", true);
                check.apart(&["items".into(), "drag-all".into(), "status-bar".into()]);
            }
            if matches!(scene, Scene::Confirm) {
                for part in ["confirm-question", "cancel-clear", "confirm-action"] {
                    check.inside(part, "status-bar", true);
                }
                check.apart(&names(["confirm-question", "cancel-clear", "confirm-action"], ""));
            }
            if matches!(scene, Scene::Alert | Scene::Job) {
                check.inside("alert", "title-buttons", true);
            }
            if matches!(scene, Scene::Job) {
                check.inside("jobs", "title-buttons", true);
                check.inside("jobs-stop", "title-buttons", true);
            }
        }
        Scene::Preview => {
            for part in ["preview-header", "preview-content", "preview-footer"] {
                check.in_window(part, true);
            }
            check.inside("preview-close", "preview-header", true);
            check.inside("preview-title", "preview-header", true);
            check.apart(&names(["preview-title", "preview-close"], ""));
            check.apart(&names(["title-bar", "preview-header", "preview-content", "preview-footer"], ""));
        }
        Scene::Note => {
            for part in ["preview-header", "preview-content", "preview-footer"] {
                check.in_window(part, true);
            }
            check.inside("note", "preview-content", true);
            check.inside("note-title", "preview-header", true);
            check.apart(&names(["note-title", "preview-close"], ""));
            check.apart(&names(["title-bar", "preview-header", "preview-content", "preview-footer"], ""));
        }
        Scene::Settings | Scene::Languages => {
            check.in_window("settings-page", false);
            check.in_window("status-bar", true);
            check.at_bottom("status-bar");
            let controls: Vec<String> = names(SWITCHES, "switch:")
                .into_iter()
                .chain(names(SEGMENTS, "segmented:"))
                .chain(
                    ["scale", "language", "tile-input", "check-updates", "watch-folder", "watched:0"].map(String::from),
                )
                .collect();
            for control in &controls {
                check.in_window(control, false);
            }
            check.apart(&controls);
            if matches!(scene, Scene::Languages) {
                check.in_window("language-menu", true);
            }
        }
        Scene::Actions | Scene::FullActions => {
            check.in_window("actions-page", false);
            check.in_window("status-bar", true);
            check.at_bottom("status-bar");
            let rows = check.present(names(0..actions::MAX_TILES, "action-row:"));
            if rows.is_empty() {
                check.fail("no tile rows".into());
            }
            if matches!(scene, Scene::FullActions) && rows.len() != actions::MAX_TILES {
                check.fail(format!("{} of {} tile rows are rendered", rows.len(), actions::MAX_TILES));
            }
            for (index, row) in rows.iter().enumerate() {
                check.in_window(row, false);
                let parts = [format!("move-up:{index}"), format!("move-down:{index}")];
                check.inside(&format!("action-buttons:{index}"), row, true);
                for part in &parts {
                    check.inside(part, &format!("action-buttons:{index}"), true);
                }
                check.apart(&parts);
                check.apart(&[format!("action-title:{index}"), format!("action-buttons:{index}")]);
            }
            check.apart(&rows);
            if matches!(scene, Scene::FullActions) {
                check.in_window("tiles-full", false);
            } else if check.find("tiles-full").is_some() {
                check.fail("the limit notice is shown below the limit".into());
            }
            let chips = names(CHIPS, "chip:");
            for chip in &chips {
                check.in_window(chip, false);
            }
            check.apart(&chips);
            check.in_window("plugins", false);
            let mut plugins = names(PLUGINS, "chip:plugin:");
            plugins.push("plugins-hint".into());
            for part in &plugins {
                check.inside(part, "plugins", false);
            }
            check.apart(&plugins);
            for id in PLUGINS {
                let trash = format!("uninstall-plugin:{id}");
                if check.find(&trash).is_none() {
                    check.fail(format!("{trash} is missing"));
                }
                check.inside(&trash, &format!("chip:plugin:{id}"), true);
            }
        }
        Scene::Editor => {
            check.in_window("editor-page", false);
            let parts = ["back", "field:0", "field:1", "icon-only", "save-action", "delete-action"];
            for part in parts {
                check.in_window(part, false);
            }
            check.apart(&names(parts, ""));
            check.inside("switch:icon-only", "icon-only", true);
        }
        Scene::Share | Scene::SharePending => {
            check.in_window("share-page", false);
            check.in_window("share-code", true);
            for part in SHARE_PARTS {
                check.in_window(part, false);
            }
            check.inside("share-copy", "share-url", true);
            check.apart(&names(SHARE_PARTS, ""));
            check.apart(&["title-bar".into(), "share-code".into()]);
        }
        Scene::Hashes | Scene::HashesPending => {
            check.in_window("hashes-page", false);
            let cards = names(0..2, "hash-card:");
            for (index, card) in cards.iter().enumerate() {
                check.in_window(card, false);
                for part in ["copy-sha256", "copy-md5", "hash-verdict"] {
                    check.inside(&format!("{part}:{index}"), card, true);
                }
                check.apart(
                    &names(["copy-sha256", "copy-md5", "hash-verdict"], "")
                        .iter()
                        .map(|part| format!("{part}:{index}"))
                        .collect::<Vec<_>>(),
                );
            }
            check.in_window("hashes-done", false);
            let mut all = cards;
            all.push("hashes-done".into());
            check.apart(&all);
        }
        Scene::Qr | Scene::QrPending => {
            check.in_window("qr-page", false);
            check.in_window("qr-code", true);
            for part in QR_PARTS {
                check.in_window(part, false);
            }
            check.apart(&names(QR_PARTS, ""));
            for button in QR_BUTTONS {
                check.inside(button, "qr-buttons", true);
            }
            check.apart(&names(QR_BUTTONS, ""));
            check.apart(&["title-bar".into(), "qr-code".into()]);
        }
        Scene::Rename | Scene::RenamePending => {
            check.in_window("rename-page", false);
            let named = matches!(scene, Scene::Rename);
            let parts: Vec<String> = RENAME_PARTS
                .iter()
                .filter(|part| named || **part != "rename-name")
                .map(|part| part.to_string())
                .collect();
            for part in &parts {
                check.in_window(part, false);
            }
            check.apart(&parts);
            let rules = names(0..4, "rename-rule:");
            for rule in &rules {
                check.inside(rule, "rename-rules", false);
            }
            check.apart(&rules);
            let rows = names(0..5, "rename-preview:");
            for row in &rows {
                check.inside(row, "rename-preview", true);
            }
            check.apart(&rows);
            for button in ["rename-cancel", "rename-apply"] {
                check.inside(button, "rename-buttons", true);
            }
            check.apart(&names(["rename-cancel", "rename-apply"], ""));
        }
        Scene::Password => {
            check.in_window("password-page", false);
            let parts = names(PASSWORD_PARTS, "");
            for part in &parts {
                check.in_window(part, false);
            }
            check.apart(&parts);
            for part in ["password-title", "password-info"] {
                check.inside(part, "password-header", true);
            }
            check.apart(&names(["password-title", "password-info"], ""));
            check.inside("password-error", "password-field", true);
            for button in ["password-cancel", "password-unpack"] {
                check.inside(button, "password-buttons", true);
            }
            check.apart(&names(["password-cancel", "password-unpack"], ""));
        }
        Scene::Media | Scene::MediaPending => {
            check.in_window("media-page", false);
            let parts = names(MEDIA_PARTS, "");
            for part in &parts {
                check.in_window(part, false);
            }
            check.apart(&parts);
            for part in ["media-title", "media-info"] {
                check.inside(part, "media-header", true);
            }
            check.apart(&names(["media-title", "media-info"], ""));
            let qualities = names(0..7, "media-quality:");
            for quality in &qualities {
                check.inside(quality, "media-quality", true);
            }
            check.apart(&qualities);
            let rows = names(0..4, "media-extra:");
            for (row, switch) in rows.iter().zip(names(MEDIA_SWITCHES, "switch:")) {
                check.inside(row, "media-extras", true);
                check.inside(&switch, row, true);
            }
            check.apart(&rows);
            for button in ["media-cancel", "media-download"] {
                check.inside(button, "media-buttons", true);
            }
            check.apart(&names(["media-cancel", "media-download"], ""));
        }
        Scene::VideoEditor => {
            check.in_window("editor-page", false);
            let parts: Vec<String> = names(["field:0"], "")
                .into_iter()
                .chain([Tool::YtDlp, Tool::Ffmpeg, Tool::Deno].map(|tool| format!("tool:{}", tool.name())))
                .chain(names(["save-action", "delete-action"], ""))
                .collect();
            for part in &parts {
                check.in_window(part, false);
            }
            check.apart(&parts);
            for tool in [Tool::YtDlp, Tool::Ffmpeg, Tool::Deno] {
                let row = format!("tool:{}", tool.name());
                check.inside(&format!("{row}:action"), &row, true);
                check.apart(&[format!("{row}:text"), format!("{row}:action")]);
            }
        }
        Scene::Toast => {
            if check.present(["drag-all".to_string(), "status-bar".to_string()]).len() < 2 {
                check.fail("the shelf bars are not rendered".into());
            }
            check.inside("toast-icon", "title-buttons", true);
            if check.find("notice").is_some() {
                check.fail("the notice bar covers the bottom bars".into());
            }
        }
        Scene::ToastBar | Scene::AlertBar => {
            check.in_window("notice", true);
            check.right_above("notice", "status-bar");
            check.apart(&["title-bar".into(), "notice".into(), "status-bar".into()]);
            if !check.present(["toast-icon".to_string(), "alert".to_string()]).is_empty() {
                check.fail("a notice is shown both in the title and at the bottom".into());
            }
        }
        Scene::HashEditor => {
            check.in_window("editor-page", false);
            let parts = names(["back", "field:0", "field:1", "save-action", "delete-action"], "");
            for part in &parts {
                check.in_window(part, false);
            }
            check.apart(&parts);
        }
        Scene::PluginEditor => {
            check.in_window("editor-page", false);
            let mut parts = names(["field:0", "plugin-about"], "");
            parts.extend(names(0..4, "setting:"));
            parts.extend(names(["plugin-status", "save-action", "delete-action"], ""));
            for part in &parts {
                check.in_window(part, false);
            }
            check.apart(&parts);
            check.inside("uninstall-plugin", "plugin-status", true);
            check.apart(&names(["plugin-status:text", "plugin-status:action"], ""));
            check.inside("switch:setting:1", "setting:1", true);
            check.inside("pick-setting:3", "setting:3", true);
        }
        Scene::OcrEditor => {
            check.in_window("ocr-languages", false);
            check.in_window("ocr-language-add", false);
            let languages = names(0..7, "ocr-language:");
            for language in &languages {
                check.inside(language, "ocr-languages", false);
            }
            check.apart(&languages);
            let mut parts = names(["field:0", "ocr-languages", "ocr-language-add", "save-action"], "");
            check.apart(&parts);
            parts.push("delete-action".into());
            for part in &parts {
                check.in_window(part, false);
            }
        }
        Scene::Lend | Scene::FullLend => {
            if check.find("drop-zone").is_some() {
                check.fail("the shelf zone is shown while a shelf item is dragged".into());
            }
            let tiles = check.present(names(0..actions::MAX_TILES, "drop-tile:"));
            if tiles.is_empty() {
                check.fail("no drop tiles".into());
            }
            if matches!(scene, Scene::FullLend) && tiles.len() != actions::MAX_TILES {
                check.fail(format!("{} of {} drop tiles are rendered", tiles.len(), actions::MAX_TILES));
            }
            check.in_window("drop-cancel", true);
            check.inside("drop-cancel", "drop-tiles", true);
            for tile in &tiles {
                check.in_window(tile, true);
                check.inside(tile, "drop-tiles", true);
            }
            let reach = f32::from(check.window.bottom()) - space::PAGE - EPS;
            if let Some(last) = check.find("drop-cancel")
                && f32::from(last.bottom()) < reach
            {
                check.fail(format!("the tiles end at {last:?} instead of filling the window down to {reach}"));
            }
            let mut all = tiles.clone();
            all.push("drop-cancel".into());
            all.push("title-bar".into());
            check.apart(&all);
        }
        Scene::Drop | Scene::FullDrop => {
            if check.find("drop-cancel").is_some() {
                check.fail("a drag from the computer shows the cancel tile".into());
            }
            check.in_window("drop-zone", true);
            let tiles = check.present(names(0..actions::MAX_TILES, "drop-tile:"));
            if tiles.is_empty() {
                check.fail("no drop tiles".into());
            }
            if matches!(scene, Scene::FullDrop) && tiles.len() != actions::MAX_TILES {
                check.fail(format!("{} of {} drop tiles are rendered", tiles.len(), actions::MAX_TILES));
            }
            for tile in &tiles {
                check.in_window(tile, true);
            }
            let mut all = tiles.clone();
            all.push("drop-zone".into());
            all.push("title-bar".into());
            check.apart(&all);
        }
    }
}

fn run(cx: &mut TestAppContext, scenes: &[Scene]) {
    run_with_scale(cx, scenes, store::Settings::default().scale);
}

fn run_with_scale(cx: &mut TestAppContext, scenes: &[Scene], scale: u16) {
    let mut failures = Vec::new();
    for language in Language::ALL {
        let _language = crate::testing::language(language);
        for &scene in scenes {
            for (width, height) in SIZES {
                let empty = matches!(scene, Scene::Empty);
                let mut h = open_with(cx, true, move |sandbox, store, fake| {
                    store.settings_mut().scale = scale;
                    if !empty {
                        rich(sandbox, store, fake);
                    }
                });
                h.cx.simulate_resize(size(px(width), px(height)));
                h.settle();
                prepare(&mut h, scene);
                let (min_width, min_height) = h.min_size();
                if min_width > width || min_height > height {
                    h.cx.simulate_resize(size(px(width.max(min_width)), px(height.max(min_height))));
                }
                let ids = h.read(|shelf| shelf.visible_items().iter().map(|item| item.id.clone()).collect::<Vec<_>>());
                h.cx.update(|window, _| window.refresh());
                h.cx.run_until_parked();
                let window = h.cx.update(|window, _| Bounds::new(point(px(0.), px(0.)), window.viewport_size()));
                let mut check = Check {
                    h: &mut h,
                    window,
                    context: format!("{scene:?} {} {width}x{height} scale {scale}", language.native_name()),
                    failures: Vec::new(),
                };
                verify(&mut check, scene, &ids);
                failures.extend(check.failures);
                h.fake.drop_jobs();
            }
        }
    }
    assert!(failures.is_empty(), "{} layout failures:\n{}", failures.len(), failures.join("\n"));
}

#[gpui::test]
fn shelf_layouts_keep_elements_inside_and_apart(cx: &mut TestAppContext) {
    run(
        cx,
        &[Scene::Empty, Scene::List, Scene::Grid, Scene::GridSelection, Scene::Selection, Scene::Notes, Scene::Search],
    );
}

#[gpui::test]
fn status_states_keep_elements_inside_and_apart(cx: &mut TestAppContext) {
    run(cx, &[Scene::Alert, Scene::Job, Scene::Confirm, Scene::Preview, Scene::Note]);
}

#[gpui::test]
fn side_pages_keep_elements_inside_and_apart(cx: &mut TestAppContext) {
    run(
        cx,
        &[
            Scene::Settings,
            Scene::Languages,
            Scene::Actions,
            Scene::Editor,
            Scene::Drop,
            Scene::Lend,
            Scene::Share,
            Scene::SharePending,
        ],
    );
}

#[gpui::test]
fn largest_scale_keeps_elements_inside_and_apart(cx: &mut TestAppContext) {
    let largest = store::SCALES.into_iter().max().expect("scales");
    run_with_scale(
        cx,
        &[Scene::List, Scene::Grid, Scene::Selection, Scene::Settings, Scene::Actions, Scene::Editor, Scene::Drop],
        largest,
    );
}

#[gpui::test]
fn full_tile_list_keeps_rows_and_drop_tiles_inside_and_apart(cx: &mut TestAppContext) {
    run(cx, &[Scene::FullActions, Scene::FullDrop, Scene::FullLend]);
}

#[gpui::test]
fn result_pages_keep_elements_inside_and_apart(cx: &mut TestAppContext) {
    run(
        cx,
        &[
            Scene::Hashes,
            Scene::HashesPending,
            Scene::Qr,
            Scene::QrPending,
            Scene::Rename,
            Scene::RenamePending,
            Scene::Media,
            Scene::MediaPending,
            Scene::Password,
            Scene::Toast,
            Scene::ToastBar,
            Scene::AlertBar,
            Scene::HashEditor,
            Scene::OcrEditor,
            Scene::VideoEditor,
            Scene::PluginEditor,
        ],
    );
}

#[gpui::test]
fn placeholders_keep_the_layout_of_the_result(cx: &mut TestAppContext) {
    let mut failures = Vec::new();
    for (scale, (width, height)) in store::SCALES.into_iter().flat_map(|scale| SIZES.map(|size| (scale, size))) {
        for opened in [Opened::Share, Opened::Hashes, Opened::Qr, Opened::Rename, Opened::Media] {
            let mut h = open_with(cx, true, move |sandbox, store, fake| {
                rich(sandbox, store, fake);
                store.settings_mut().scale = scale;
            });
            h.cx.simulate_resize(size(px(width), px(height)));
            h.settle();
            let (index, payload) = page_tile(&mut h, opened);
            let marks = page_marks(opened);
            let layout = |h: &mut Harness| {
                h.cx.update(|window, _| window.refresh());
                h.cx.run_until_parked();
                marks.iter().map(|mark| h.cx.debug_bounds(interned(mark))).collect::<Vec<_>>()
            };
            h.pending(|shelf, _, cx| shelf.perform(index, payload, cx));
            let waiting = layout(&mut h);
            h.settle();
            let ready = layout(&mut h);
            for (mark, (before, after)) in marks.iter().zip(waiting.iter().zip(&ready)) {
                let kept = match (before, after) {
                    (Some(before), Some(after)) => {
                        (f32::from(before.top()) - f32::from(after.top())).abs() <= EPS
                            && (f32::from(before.size.height) - f32::from(after.size.height)).abs() <= EPS
                    }
                    _ => false,
                };
                if !kept {
                    failures.push(format!("{opened:?} {width}x{height} scale {scale} {mark}: {before:?} -> {after:?}"));
                }
            }
            h.fake.drop_jobs();
        }
    }
    assert!(failures.is_empty(), "placeholders moved the page:\n{}", failures.join("\n"));
}

#[test]
fn skipped_rows_take_the_device_pixels_of_built_rows() {
    let device = |logical: f32, scale: f32| (logical * scale).round();
    for scale in [1., 1.25, 1.5, 1.75, 2.] {
        for (height, gap) in [(50., 0.), (157.6, 12.), (163., 10.)] {
            let rows = super::super::items::Rows::snapped(2, 12., height, gap, scale);
            let (row, between) = (device(height, scale), device(gap, scale));
            for count in 1..500 {
                let skipped = device(count as f32 * rows.pitch - rows.gap, scale);
                let built = count as f32 * row + (count - 1) as f32 * between;
                assert_eq!(skipped, built, "scale {scale}: {count} rows of {height} with gap {gap}");
            }
        }
    }
}

#[gpui::test]
fn empty_category_stays_in_place_when_the_layout_changes(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, notes(&["note"]));
    h.act(|shelf, _, cx| shelf.set_filter(Filter::Files, cx));
    let empty = |h: &mut Harness| {
        h.cx.update(|window, _| window.refresh());
        h.cx.run_until_parked();
        h.cx.debug_bounds("empty-list").expect("the empty category is shown")
    };
    let list = empty(&mut h);
    h.act(|shelf, _, cx| shelf.toggle_layout(cx));
    let grid = empty(&mut h);
    assert_eq!((list.top(), list.bottom()), (grid.top(), grid.bottom()), "list {list:?}, grid {grid:?}");
}

#[gpui::test]
fn scrolled_shelf_renders_the_rows_in_view(cx: &mut TestAppContext) {
    for (grid, scale) in [(false, 100), (true, 100), (false, 150), (true, 150)] {
        let mut h = open_with(cx, true, move |_, store, _| {
            store.settings_mut().scale = scale;
            for index in 0..80 {
                store.add_text(format!("note {index}")).expect("note");
            }
        });
        h.cx.simulate_resize(size(px(540.), px(540.)));
        h.settle();
        if grid {
            h.act(|shelf, _, cx| shelf.toggle_layout(cx));
        }
        h.act(|shelf, _, cx| {
            shelf.scrolls[Page::Shelf as usize].set_offset(point(px(0.), px(-1500.)));
            cx.notify();
        });
        h.cx.update(|window, _| window.refresh());
        h.cx.run_until_parked();
        let area = h.cx.debug_bounds("items").expect("the list is rendered");
        let ids = h.read(|shelf| shelf.visible_items().iter().map(|item| item.id.clone()).collect::<Vec<_>>());
        let rendered: Vec<Bounds<Pixels>> =
            ids.iter().filter_map(|id| h.cx.debug_bounds(interned(&format!("item:{id}")))).collect();
        let top = rendered.iter().map(|bounds| f32::from(bounds.top())).fold(f32::INFINITY, f32::min);
        let bottom = rendered.iter().map(|bounds| f32::from(bounds.bottom())).fold(f32::NEG_INFINITY, f32::max);
        assert!(
            top <= f32::from(area.top()) + EPS && bottom >= f32::from(area.bottom()) - EPS,
            "grid {grid}, scale {scale}: rendered rows span {top}..{bottom}, the scrolled list shows {area:?}"
        );
        assert!(rendered.len() < ids.len(), "grid {grid}, scale {scale}: rows far outside the list are not built");
    }
}

fn row_height(cx: &mut TestAppContext, scale: u16) -> f32 {
    let mut h = open_with(cx, true, move |_, store, _| {
        store.settings_mut().scale = scale;
        store.add_text("note".to_string()).expect("note");
    });
    h.cx.update(|window, _| window.refresh());
    h.cx.run_until_parked();
    let id = h.read(|shelf| shelf.visible_items().first().map(|item| item.id.clone())).expect("a row");
    f32::from(h.cx.debug_bounds(interned(&format!("item:{id}"))).expect("the row is rendered").size.height)
}

#[gpui::test]
fn scale_scales_the_interface(cx: &mut TestAppContext) {
    let regular = row_height(cx, 100);
    let largest = row_height(cx, 150);
    assert!((largest - regular * 1.5).abs() < 1., "row {regular} at 100%, {largest} at 150%");
}

#[gpui::test]
fn keyboard_navigation_keeps_the_hovered_row_in_view(cx: &mut TestAppContext) {
    fn assert_in_view(h: &mut Harness, what: &str) {
        let hovered = h.read(|shelf| shelf.hovered_item()).expect("an item is hovered");
        let area = h.cx.debug_bounds("items").expect("the list is rendered");
        let row = h.cx.debug_bounds(interned(&format!("item:{hovered}"))).expect("the hovered row is rendered");
        assert!(
            row.top() >= area.top() - px(EPS) && row.bottom() <= area.bottom() + px(EPS),
            "{what}: the hovered row {row:?} is inside the list {area:?}"
        );
    }
    for grid in [false, true] {
        let mut h = open_with(cx, true, |_, store, _| {
            for index in 0..80 {
                store.add_text(format!("note {index}")).expect("note");
            }
        });
        h.cx.simulate_resize(size(px(540.), px(540.)));
        h.settle();
        if grid {
            h.act(|shelf, _, cx| shelf.toggle_layout(cx));
        }
        h.keys(&["down"; 40].join(" "));
        assert_in_view(&mut h, &format!("grid {grid}, stepping down"));
        h.keys(&["up"; 30].join(" "));
        assert_in_view(&mut h, &format!("grid {grid}, stepping up"));
        h.act(|shelf, _, _| shelf.hover.clear());
        h.keys("up");
        assert_in_view(&mut h, &format!("grid {grid}, jumping to the last item"));
    }
}

#[gpui::test]
fn markers_are_inert_outside_the_test_profile(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, rich);
    h.render();
    assert!(!crate::diagnostics::enabled());
    assert!(!h.sandbox.join("root").join(crate::diagnostics::FILE).exists());
}
