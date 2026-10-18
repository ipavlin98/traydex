use super::*;
use crate::actions::{Action, Kind, Outcome, Tiles, run};
use crate::i18n::Language;
use crate::store::Payload;
use crate::system::Offered;
use crate::testing::{Call, Fake, Invocation, Sandbox, archive, language, output, plant_plugin};

const MANIFEST_JSON: &str = r#"{
    "id": "resize",
    "name": "Уменьшить фото",
    "description": "Сжимает снимки",
    "entry": "main.ps1",
    "accepts": {"extensions": ["png", ".JPG"]},
    "settings": [
        {"key": "api_key", "label": "Ключ", "type": "secret", "required": true},
        {"key": "keep", "label": "Оставить оригинал", "type": "toggle", "default": true}
    ]
}"#;

struct Bench {
    sandbox: Sandbox,
    fake: Fake,
    storage: Storage,
}

impl Bench {
    fn new() -> Self {
        let sandbox = Sandbox::new();
        let fake = Fake::new(&sandbox);
        let storage = sandbox.storage();
        Self { sandbox, fake, storage }
    }

    fn plant(&self, manifest: &str) -> Arc<Plugin> {
        plant_plugin(&self.storage, manifest);
        scan(&self.storage).into_iter().next().expect("planted plugin")
    }

    fn bundle(&self, name: &str, entries: &[(&str, &str)]) -> PathBuf {
        let path = self.sandbox.join(name);
        let entries: Vec<(&str, &[u8])> = entries.iter().map(|(entry, data)| (*entry, data.as_bytes())).collect();
        archive(&path, &entries);
        path
    }

    fn reply(&self, code: u32, lines: &'static [&'static str], stderr: &'static str) {
        self.fake.state().streamer = Some(Box::new(move |_: &Invocation| Ok(output(code, &lines.join("\n"), stderr))));
    }

    fn run(&self, custom: &Custom, files: Vec<PathBuf>) -> (Result<Outcome>, Progress) {
        let progress = Progress::default();
        let action = Action::new(Kind::Custom(custom.clone()));
        let result = run(&action, Payload::Paths(files), &progress, &self.storage, &self.fake);
        (result, progress)
    }

    fn invocation(&self) -> Invocation {
        self.fake
            .calls()
            .into_iter()
            .find_map(|call| match call {
                Call::Output(invocation) => Some(invocation),
                _ => None,
            })
            .expect("the script was started")
    }

    fn sent(&self) -> Value {
        serde_json::from_slice(&self.invocation().input.expect("stdin")).expect("stdin json")
    }
}

fn configured(plugin: Arc<Plugin>) -> Custom {
    let mut custom = Custom::new(plugin);
    custom.set("api_key", Value::String("секрет 42".into()));
    custom
}

#[test]
fn bundle_installs_into_its_folder_and_replaces_the_previous_version() {
    let bench = Bench::new();
    let first = bench.bundle("Resize.TDX", &[(MANIFEST, MANIFEST_JSON), ("main.ps1", "exit 0"), ("lib/util.ps1", "")]);
    let plugin = install(&bench.storage, &first).expect("install");
    assert_eq!(plugin.id(), "resize");
    assert!(folder(&bench.storage).join("resize").join("lib").join("util.ps1").is_file());

    let renamed = MANIFEST_JSON.replace("Уменьшить фото", "Новая версия");
    let second = bench.bundle("resize-2.tdx", &[(MANIFEST, &renamed), ("main.ps1", "exit 0")]);
    install(&bench.storage, &second).expect("reinstall");
    let installed = scan(&bench.storage);
    assert_eq!(installed.iter().map(|plugin| plugin.name()).collect::<Vec<_>>(), ["Новая версия"]);
    assert!(!folder(&bench.storage).join("resize").join("lib").exists(), "files of the old version are gone");
    let leftovers: Vec<_> = fs::read_dir(folder(&bench.storage)).expect("custom").flatten().collect();
    assert_eq!(leftovers.len(), 1, "no staged or backup folders stay behind");
}

#[test]
fn uninstall_deletes_only_that_plugin_and_leaves_nothing_behind() {
    let bench = Bench::new();
    let other = MANIFEST_JSON.replace("\"resize\"", "\"other\"");
    plant_plugin(&bench.storage, &other);
    plant_plugin(&bench.storage, MANIFEST_JSON);
    let resize = scan(&bench.storage).into_iter().find(|plugin| plugin.id() == "resize").expect("resize");
    uninstall(&resize).expect("uninstall");
    assert_eq!(scan(&bench.storage).iter().map(|plugin| plugin.id()).collect::<Vec<_>>(), ["other"]);
    let left: Vec<_> =
        fs::read_dir(folder(&bench.storage)).expect("custom").flatten().map(|entry| entry.file_name()).collect();
    assert_eq!(left, ["other"], "no backup folder stays behind");
    uninstall(&resize).expect("a plugin that is already gone counts as uninstalled");
}

#[test]
fn bundle_escaping_the_plugin_folder_installs_nothing() {
    let bench = Bench::new();
    let evil = bench.bundle("evil.tdx", &[(MANIFEST, MANIFEST_JSON), ("main.ps1", ""), ("../../escaped.txt", "x")]);
    assert!(install(&bench.storage, &evil).is_err());
    assert!(scan(&bench.storage).is_empty());
    assert!(!bench.storage.root().join("escaped.txt").exists());
    assert!(!bench.storage.root().join("actions").join("escaped.txt").exists());
    assert_eq!(fs::read_dir(folder(&bench.storage)).expect("custom").count(), 0, "the staged copy is removed");
}

#[test]
fn unpacking_stops_at_the_size_limit() {
    let bench = Bench::new();
    let bundle = bench.bundle("big.tdx", &[("a.bin", "123456"), ("b.bin", "123456")]);
    let open = || ZipArchive::new(fs::File::open(&bundle).expect("bundle")).expect("zip");
    assert!(extract(&mut open(), &bench.sandbox.join("fits"), 12).is_ok());
    assert!(extract(&mut open(), &bench.sandbox.join("too-big"), 11).is_err());
    let written = fs::metadata(bench.sandbox.join("too-big").join("b.bin")).map_or(0, |meta| meta.len());
    assert!(written <= 6, "no more than the limit plus one byte reaches the disk");
}

#[test]
fn bundle_with_an_invalid_manifest_is_rejected_with_its_name() {
    let _ru = language(Language::Russian);
    let bench = Bench::new();
    let cases = [
        MANIFEST_JSON.replace(r#""id": "resize""#, r#""id": "../resize""#),
        MANIFEST_JSON.replace(r#""id": "resize""#, r#""id": "Resize""#),
        MANIFEST_JSON.replace("main.ps1", "../main.ps1"),
        MANIFEST_JSON.replace("main.ps1", "main.vbs"),
        MANIFEST_JSON.replace(r#""key": "keep""#, r#""key": "API_KEY""#),
        MANIFEST_JSON.replace(r#""name": "Уменьшить фото","#, ""),
        MANIFEST_JSON.replace(r#""entry""#, r#""icon": "icon.png", "entry""#),
        MANIFEST_JSON.replace(r#""entry""#, r#""icon": "icon.svg.jpg", "entry""#),
    ];
    for manifest in &cases {
        let bundle = bench.bundle("bad.tdx", &[(MANIFEST, manifest), ("main.ps1", ""), ("main.vbs", "")]);
        let error = install(&bench.storage, &bundle).expect_err(manifest);
        assert!(format!("{error:#}").starts_with(&fill(tr().plugin_install_failed, "bad.tdx")), "{error:#}");
    }
    let missing_entry = bench.bundle("bad.tdx", &[(MANIFEST, MANIFEST_JSON)]);
    assert!(install(&bench.storage, &missing_entry).is_err());
    let not_zip = bench.sandbox.file("plain.tdx", "not a zip");
    assert!(install(&bench.storage, &not_zip).is_err());
    assert!(scan(&bench.storage).is_empty());
}

#[test]
fn script_gets_files_and_settings_through_stdin_and_environment_only() {
    let bench = Bench::new();
    let plugin = bench.plant(MANIFEST_JSON);
    let photo = bench.sandbox.file("папка с пробелом/фото 1.PNG", "png");
    let note = bench.sandbox.file("заметка.txt", "text");
    bench.reply(0, &[], "");
    let (result, _) = bench.run(&configured(plugin), vec![note, photo.clone()]);
    result.expect("run");

    let sent = bench.sent();
    assert_eq!(sent["files"], json!([photo]), "only accepted files are sent");
    assert_eq!(sent["settings"], json!({"api_key": "секрет 42", "keep": true}));
    assert!(sent["output"].as_str().is_some_and(|folder| !folder.is_empty()));
    let invocation = bench.invocation();
    assert!(!invocation.args().iter().any(|arg| arg.contains("фото")), "paths never go through arguments");
    assert_eq!(invocation.env("TRAYDEX_SETTING_API_KEY"), Some("секрет 42"));
    assert_eq!(invocation.env("TRAYDEX_SETTING_KEEP"), Some("1"));
    assert_eq!(invocation.env("TRAYDEX_ACTION"), Some("resize"));
    assert_eq!(invocation.dir.as_deref(), Some(folder(&bench.storage).join("resize").as_path()));
}

#[test]
fn script_messages_drive_progress_clipboard_shelf_and_notice() {
    let bench = Bench::new();
    let plugin = bench.plant(MANIFEST_JSON);
    let photo = bench.sandbox.file("a.png", "png");
    let outside = bench.sandbox.file("готовый.png", "result");
    let outside_line: &'static str = Box::leak(json!({"type": "add", "path": outside}).to_string().into_boxed_str());
    bench.fake.state().streamer = Some(Box::new(move |invocation: &Invocation| {
        let sent: Value = serde_json::from_slice(invocation.input.as_deref().expect("stdin")).expect("json");
        let folder = PathBuf::from(sent["output"].as_str().expect("output"));
        fs::write(folder.join("small.png"), "small").expect("result");
        let lines = [
            "Начинаю работу",
            "{not json",
            r#"{"type": "unknown", "text": "x"}"#,
            r#"{"type": "progress", "value": 0.4}"#,
            r#"{"type": "progress", "value": 7}"#,
            r#"{"type": "clipboard", "text": "https://example.com/фото"}"#,
            r#"{"type": "add", "path": "small.png"}"#,
            r#"{"type": "add", "path": "missing.png"}"#,
            outside_line,
            r#"{"type": "notify", "text": "Сжато 2 фото"}"#,
        ];
        Ok(output(0, &lines.join("\r\n"), ""))
    }));
    let (result, progress) = bench.run(&configured(plugin), vec![photo]);
    let outcome = result.expect("run");
    assert_eq!(outcome.message, "Сжато 2 фото");
    assert_eq!(outcome.clipboard.as_deref(), Some("https://example.com/фото"));
    let names: Vec<String> = outcome.created.iter().map(|path| base(path)).collect();
    assert_eq!(names, ["small.png", "готовый.png"]);
    assert!(outcome.created.iter().all(|path| path.is_file()));
    assert_eq!(progress.get().fraction, Some(1.), "progress is clamped to the end");
}

#[test]
fn reported_error_or_failed_exit_fails_the_action_and_drops_its_output() {
    let _ru = language(Language::Russian);
    let bench = Bench::new();
    let custom = configured(bench.plant(MANIFEST_JSON));
    let photo = bench.sandbox.file("a.png", "png");
    let cases: [(u32, &'static [&'static str], &'static str, String); 3] = [
        (0, &[r#"{"type": "error", "text": "Нет сети"}"#], "", "Нет сети".into()),
        (3, &["working"], "Traceback\nValueError: плохой файл\n", "ValueError: плохой файл".into()),
        (2, &[], "", fill(tr().plugin_exit, 2)),
    ];
    for (code, lines, stderr, expected) in cases {
        bench.reply(code, lines, stderr);
        let (result, _) = bench.run(&custom, vec![photo.clone()]);
        let error = result.err().map(|error| error.to_string());
        assert_eq!(error.as_deref(), Some(expected.as_str()));
        let content = fs::read_dir(bench.storage.content()).map_or(0, |entries| entries.count());
        assert_eq!(content, 0, "the output folder is removed");
    }
}

#[test]
fn files_the_plugin_does_not_accept_never_start_it() {
    let _ru = language(Language::Russian);
    let bench = Bench::new();
    let custom = configured(bench.plant(MANIFEST_JSON));
    let note = bench.sandbox.file("a.txt", "text");
    let folder = bench.sandbox.folder("Фото.png");
    let (result, _) = bench.run(&custom, vec![note, folder]);
    assert_eq!(result.err().map(|error| error.to_string()).as_deref(), Some(tr().plugin_unsuitable));
    assert!(bench.fake.calls().is_empty());
}

#[test]
fn drag_offer_is_admitted_by_extension_mime_and_kind() {
    let accepts = |json: &str| serde_json::from_str::<Accepts>(json).expect("accepts");
    let file = |name: &str| Offer::Files(vec![Offered { path: PathBuf::from(name), folder: false }]);
    let folder = Offer::Files(vec![Offered { path: PathBuf::from(r"C:\Фото"), folder: true }]);

    let images = accepts(r#"{"mime": ["image/*"]}"#);
    assert!(images.admits_offer(&Offer::Image));
    assert!(images.admits_offer(&file(r"C:\a b\photo.JPEG")));
    assert!(!images.admits_offer(&Offer::Text));
    assert!(!images.admits_offer(&folder));
    assert!(images.admits_offer(&Offer::Unknown));

    let texts = accepts(r#"{"extensions": ["TXT"], "folders": true}"#);
    assert!(texts.admits_offer(&Offer::Text));
    assert!(texts.admits_offer(&folder));
    assert!(!texts.admits_offer(&file("report.pdf")));
    let mixed = Offer::Files(vec![
        Offered { path: PathBuf::from("report.pdf"), folder: false },
        Offered { path: PathBuf::from("readme.txt"), folder: false },
    ]);
    assert!(texts.admits_offer(&mixed), "one fitting item is enough");

    let only_folders = accepts(r#"{"files": false, "folders": true}"#);
    assert!(!only_folders.admits_offer(&file("a.txt")));
    assert!(accepts("{}").admits_offer(&file("anything.bin")));
    assert!(!accepts("{}").admits_offer(&folder), "folders are opt-in");
}

#[test]
fn custom_tile_is_saved_by_id_and_needs_its_plugin_and_required_settings() {
    let bench = Bench::new();
    let plugin = bench.plant(MANIFEST_JSON);
    let unset = Action::new(Kind::Custom(Custom::new(plugin.clone())));
    assert!(!unset.ready(), "the required key is empty");
    let action = Action::new(Kind::Custom(configured(plugin)));
    assert!(action.ready());
    assert_eq!(
        serde_json::to_value(&action).expect("json"),
        json!({"type": "custom", "id": "resize", "values": {"api_key": "секрет 42"}})
    );

    crate::actions::tiles::save(
        &bench.storage,
        &[action, Action::new(Kind::Custom(serde_json::from_value(json!({"id": "gone"})).expect("custom")))],
    )
    .expect("save");
    let mut tiles = Tiles::new(bench.storage.clone());
    tiles.load(&bench.fake).expect("load");
    let ready: Vec<bool> = tiles.actions().map(|tile| tile.action.ready()).collect();
    assert_eq!(ready, [true, false], "the installed plugin is attached, the missing one is not");
}

#[test]
fn example_plugins_are_valid() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples").join("plugins");
    let folders: Vec<PathBuf> = fs::read_dir(&root)
        .expect("examples")
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    assert!(!folders.is_empty(), "examples/plugins has plugins");
    for folder in folders {
        let plugin = Plugin::load(&folder).unwrap_or_else(|error| panic!("{}: {error:#}", folder.display()));
        assert_eq!(folder.file_name().and_then(|name| name.to_str()), Some(plugin.id()));
        assert!(plugin.icon().is_some(), "{}: the icon is missing or can't be drawn", plugin.id());
    }
}
