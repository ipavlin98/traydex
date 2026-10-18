use super::tiles::{load, reset, save};
use super::*;
use crate::i18n::Language;
use crate::store::Payload;
use crate::store::{ImageData, Scratch, save_virtual};
use crate::system::Offered;
use crate::testing::{Call, Fake, Invocation, Sandbox, language, output, png};
use std::fs::File;
use std::io::{self, Read as _};

struct Bench {
    sandbox: Sandbox,
    fake: Fake,
    storage: Storage,
}

impl Bench {
    fn new() -> Self {
        let sandbox = Sandbox::new();
        let fake = Fake::new(&sandbox);
        let storage = sandbox.store().storage().clone();
        Self { sandbox, fake, storage }
    }

    fn run(&self, kind: Kind, payload: Payload) -> Result<Outcome> {
        run(&Action::new(kind), payload, &Progress::default(), &self.storage, &self.fake)
    }

    fn paths(&self, paths: &[&PathBuf]) -> Payload {
        Payload::Paths(paths.iter().map(|path| (*path).clone()).collect())
    }

    fn invocations(&self) -> Vec<Invocation> {
        self.fake
            .calls()
            .into_iter()
            .filter_map(|call| match call {
                Call::Output(invocation) | Call::Launch(invocation) => Some(invocation),
                _ => None,
            })
            .collect()
    }

    fn opened(&self) -> Vec<String> {
        self.fake
            .calls()
            .into_iter()
            .filter_map(|call| match call {
                Call::ShellOpen(target) => Some(target),
                _ => None,
            })
            .collect()
    }

    fn content(&self) -> usize {
        fs::read_dir(self.storage.content()).map_or(0, |entries| entries.count())
    }

    fn respond(&self, result: io::Result<std::process::Output>) {
        self.fake.state().outputs.push_back(result);
    }
}

fn shared(outcome: Outcome) -> Server {
    match outcome.show {
        Some(Show::Share(server)) => server,
        _ => panic!("expected sharing"),
    }
}

fn error(result: Result<Outcome>) -> String {
    match result {
        Ok(outcome) => panic!("expected an error, got {:?}", outcome.message),
        Err(error) => error.to_string(),
    }
}

fn entries(archive: &Path) -> Vec<String> {
    let zip = zip::ZipArchive::new(File::open(archive).expect("zip")).expect("archive");
    let mut names: Vec<String> = zip.file_names().map(|name| name.expect("name").into_owned()).collect();
    names.sort();
    names
}

fn read_entry(archive: &Path, name: &str) -> String {
    let mut zip = zip::ZipArchive::new(File::open(archive).expect("zip")).expect("archive");
    let mut text = String::new();
    zip.by_name(name).expect("entry").read_to_string(&mut text).expect("read");
    text
}

#[test]
fn encode_escapes_everything_but_unreserved_bytes() {
    assert_eq!(encode("AZaz09-._~"), "AZaz09-._~");
    assert_eq!(encode("a b&c=d/e?f#g+h"), "a%20b%26c%3Dd%2Fe%3Ff%23g%2Bh");
    assert_eq!(encode("ё"), "%D1%91");
    assert_eq!(encode("🙂"), "%F0%9F%99%82");
    assert_eq!(encode(""), "");
}

#[test]
fn readable_name_uses_first_meaningful_line() {
    assert_eq!(readable_name("  \n\nHello: world?.  \nnext", "fallback"), "Hello world");
    assert_eq!(readable_name(r#"a\b/c:d*e?f"g<h>i|j"#, "x"), "abcdefghij");
    assert_eq!(readable_name("...", "fallback"), "fallback");
    assert_eq!(readable_name("", "fallback"), "fallback");
    assert_eq!(readable_name("???", "fallback"), "fallback");
    assert_eq!(readable_name(&"я".repeat(100), "x").chars().count(), 40);
    assert_eq!(readable_name("tab\there", "x"), "tabhere");
    assert_eq!(readable_name("name. . .", "x"), "name");
}

#[test]
fn last_line_returns_final_meaningful_line() {
    assert_eq!(last_line(b"first\r\nerror: broken pipe\r\n\r\n  "), "error: broken pipe");
    assert_eq!(last_line(b"  only  "), "only");
    assert_eq!(last_line(b""), "");
    assert_eq!(last_line(b"\r\n\n"), "");
    assert_eq!(last_line(b"bad \xff byte"), "bad \u{fffd} byte");
}

#[test]
fn path_helpers_handle_windows_paths() {
    assert_eq!(normalized(Path::new(r"C:\Users\Name\Desktop\")), r"c:\users\name\desktop");
    assert_eq!(normalized(Path::new("C:/Data//")), r"c:\data");
    assert_eq!(normalized(Path::new(r"\\Server\Share\Папка")), r"\\server\share\папка");
    assert_eq!(base(Path::new(r"C:\a\report.final.pdf")), "report.final.pdf");
    assert_eq!(base(Path::new(r"C:\")), r"C:\");
    assert_eq!(extension(Path::new("A.JPEG")), "jpeg");
    assert_eq!(extension(Path::new("README")), "");
    assert_eq!(stem(Path::new("archive.tar.gz")), "archive.tar");
    assert_eq!(stem(Path::new(".gitignore")), ".gitignore");
}

#[test]
fn progress_reports_fraction_only_for_batches() {
    let progress = Progress::default();
    progress.step(0, 1);
    assert_eq!(progress.get().fraction, None);
    progress.step(1, 4);
    assert_eq!(progress.get().fraction, Some(0.25));
    let clone = progress.clone();
    assert!(!progress.cancelled());
    clone.cancel();
    assert!(progress.cancelled());
}

#[test]
fn known_engines_have_names() {
    assert_eq!(engine_name(ENGINES[0]), Some("Google"));
    assert_eq!(engine_name(&format!("  {}  ", ENGINES[2])), Some("Bing"));
    assert_eq!(engine_name(ENGINES[1]), Some(tr().yandex));
    assert_eq!(engine_name(ENGINES[3]), Some("DuckDuckGo"));
    assert_eq!(engine_name(ENGINES[4]), Some("YouTube"));
    assert_eq!(engine_name("https://example.com/?q=%s"), None);
    assert!(ENGINES.iter().all(|engine| engine.contains("%s")));
}

#[test]
fn titles_prefer_custom_name() {
    let mut action = Action::new(Kind::Zip);
    assert_eq!(action.title(), fill(tr().to_format, "ZIP"));
    action.name = "   ".into();
    assert_eq!(action.title(), fill(tr().to_format, "ZIP"));
    action.name = "Pack".into();
    assert_eq!(action.title(), "Pack");
}

#[test]
fn default_titles_describe_targets() {
    let title = |kind: Kind| Action::new(kind).default_title();
    assert_eq!(title(Kind::Move { folder: r"D:\Inbox\".into() }), "Inbox");
    assert_eq!(title(Kind::Copy { folder: r"D:\Архив".into() }), "Архив");
    assert_eq!(title(Kind::Run { program: r"C:\Tools\convert.ps1".into() }), "convert");
    assert_eq!(title(Kind::Png), fill(tr().to_format, "PNG"));
    assert_eq!(title(Kind::Jpeg), fill(tr().to_format, "JPEG"));
    assert_eq!(title(Kind::Gif), fill(tr().to_format, "GIF"));
    assert_eq!(title(Kind::CopyPath), tr().path_title);
    assert_eq!(title(Kind::Imgur { client_id: String::new() }), fill(tr().to_service, "Imgur"));
    let upload = Upload { url: "https://www.files.example.org/api".into(), ..Upload::default() };
    assert_eq!(title(Kind::Upload(upload)), fill(tr().to_service, "files.example.org"));
    assert_eq!(title(Kind::Upload(Upload::default())), tr().add_upload);
    assert_eq!(title(Kind::Search { url: ENGINES[3].into() }), "DuckDuckGo");
    assert_eq!(title(Kind::Search { url: "https://wiki.example/?s=%s".into() }), "wiki.example");
    assert_eq!(title(Kind::Search { url: "  ".into() }), tr().add_search);
}

#[test]
fn details_and_verbs_follow_kind() {
    let action = Action::new(Kind::Move { folder: r"D:\Inbox".into() });
    assert_eq!(action.detail(), format!("{} · D:\\Inbox", tr().verb_move));
    assert_eq!(Action::new(Kind::Zip).detail(), tr().verb_zip);
    let search = Action::new(Kind::Search { url: ENGINES[0].into() });
    assert_eq!(search.detail(), format!("{} · google.com", tr().verb_search));
    assert_eq!(Action::new(Kind::Png).job_label(), format!("{}…", tr().job_convert));
    assert_eq!(Action::new(Kind::Ytdlp(Fetch::default())).job_label(), format!("{}…", tr().job_ytdlp));
    assert_eq!(search.job_label(), format!("{}… · Google", tr().job_search), "the engine says where it goes");
    let named = Action { name: "Архив для бухгалтерии".into(), ..Action::new(Kind::Zip) };
    assert_eq!(named.job_label(), format!("{}… · Архив для бухгалтерии", tr().job_zip), "a custom name is kept");
    assert_eq!(Action::new(Kind::Gif).activity(), tr().job_gif);
}

#[test]
fn tile_description_falls_back_to_the_verb() {
    assert_eq!(Action::new(Kind::Zip).description(), tr().verb_zip);
    let blank = Action {
        description: " 
 "
        .into(),
        ..Action::new(Kind::Zip)
    };
    assert_eq!(blank.description(), tr().verb_zip);
    let custom = Action {
        description: "Для бухгалтерии
и архива"
            .into(),
        ..Action::new(Kind::Zip)
    };
    assert_eq!(
        custom.description(),
        "Для бухгалтерии
и архива"
    );
}

#[test]
fn readiness_requires_configuration() {
    assert!(!Action::new(Kind::Imgur { client_id: "  ".into() }).ready());
    assert!(Action::new(Kind::Imgur { client_id: "abc".into() }).ready());
    let mut upload = Upload { url: "https://x.example".into(), ..Upload::default() };
    assert!(!Action::new(Kind::Upload(upload.clone())).ready());
    upload.field = "file".into();
    assert!(Action::new(Kind::Upload(upload)).ready());
    assert!(!Action::new(Kind::Search { url: " ".into() }).ready());
    assert!(Action::new(Kind::Zip).ready());
    assert!(Action::new(Kind::Move { folder: PathBuf::new() }).ready());
}

#[test]
fn editable_fields_map_to_action_text() {
    let mut search = Action::new(Kind::Search { url: ENGINES[0].into() });
    assert!(search.fields() == [Field::Name, Field::Description]);
    *search.text_mut(Field::SearchUrl).expect("url") = "https://custom.example/?q=%s".into();
    assert!(search.fields() == [Field::Name, Field::Description, Field::SearchUrl]);
    let mut upload = Action::new(Kind::Upload(Upload::default()));
    assert_eq!(upload.fields().len(), 6);
    for (field, value) in [
        (Field::Url, "u"),
        (Field::File, "f"),
        (Field::Header, "h"),
        (Field::Link, "/l"),
        (Field::Name, "n"),
        (Field::Description, "d"),
    ] {
        *upload.text_mut(field).expect("field") = value.into();
        assert_eq!(upload.text(field), value);
    }
    assert!(upload.text_mut(Field::ClientId).is_none());
    assert_eq!(upload.text(Field::SearchUrl), "");
    let mut imgur = Action::new(Kind::Imgur { client_id: String::new() });
    assert!(imgur.fields() == [Field::Name, Field::Description, Field::ClientId]);
    *imgur.text_mut(Field::ClientId).expect("client") = "id".into();
    assert!(imgur.ready());
    let all = [Field::Name, Field::ClientId, Field::Url, Field::File, Field::Header, Field::Link, Field::SearchUrl];
    assert!(all.iter().all(|field| !field.secret()));
    let mut hash = Action::new(Kind::Hash { algorithm: Algorithm::Sha256, virustotal: String::new() });
    assert!(hash.fields() == [Field::Name, Field::Description, Field::ApiKey]);
    *hash.text_mut(Field::ApiKey).expect("key") = "vt".into();
    assert!(matches!(hash.kind, Kind::Hash { ref virustotal, .. } if virustotal == "vt"));
    assert!(Field::ApiKey.secret());
    assert!(hash.ready());
}

#[test]
fn slots_identify_duplicates() {
    let a = Action::new(Kind::Move { folder: r"C:\Data\Inbox\".into() });
    let b = Action::new(Kind::Copy { folder: r"c:\data\inbox".into() });
    assert!(a.slot() == b.slot());
    assert!(Action::new(Kind::Move { folder: "C:/Data/Inbox/".into() }).slot() == a.slot());
    assert!(Action::new(Kind::Run { program: r"C:\Data\Inbox".into() }).slot() != a.slot());
    let upload = |url: &str| Action::new(Kind::Upload(Upload { url: url.into(), ..Upload::default() })).slot();
    assert!(upload(" HTTPS://X.example/up ") == upload("https://x.example/up"));
    assert!(Action::new(Kind::Jpeg).slot() != Action::new(Kind::Png).slot());
    assert!(
        Action::new(Kind::Imgur { client_id: "a".into() }).slot()
            == Action::new(Kind::Imgur { client_id: "b".into() }).slot()
    );
    let hash = |algorithm, key: &str| Action::new(Kind::Hash { algorithm, virustotal: key.into() }).slot();
    assert!(hash(Algorithm::Sha256, "") == hash(Algorithm::Md5, "key"));
    assert!(
        Action::new(Kind::Compress { format: Compression::Jpeg }).slot()
            == Action::new(Kind::Compress { format: Compression::Webp }).slot()
    );
    assert!(Action::new(Kind::Pdf).slot() != Action::new(Kind::Zip).slot());
}

#[test]
fn paths_and_tools_are_exposed_for_editing() {
    let mut action = Action::new(Kind::Run { program: r"C:\a.exe".into() });
    assert_eq!(action.path(), Some(Path::new(r"C:\a.exe")));
    *action.path_mut().expect("path") = r"C:\b.exe".into();
    assert_eq!(action.path(), Some(Path::new(r"C:\b.exe")));
    assert!(Action::new(Kind::Zip).path().is_none());
    assert!(Action::new(Kind::Gif).tools() == [Tool::Ffmpeg]);
    assert!(Action::new(Kind::Png).tools().is_empty());
}

#[test]
fn every_kind_has_an_icon() {
    let kinds = [
        Kind::Move { folder: PathBuf::new() },
        Kind::Copy { folder: PathBuf::new() },
        Kind::Zip,
        Kind::Jpeg,
        Kind::Png,
        Kind::Gif,
        Kind::Imgur { client_id: String::new() },
        Kind::CopyPath,
        Kind::Phone,
        Kind::Upload(Upload::default()),
        Kind::Run { program: PathBuf::new() },
        Kind::Search { url: String::new() },
        Kind::DataUri,
        Kind::Hash { algorithm: Algorithm::Sha256, virustotal: String::new() },
        ocr(""),
        Kind::Compress { format: Compression::Jpeg },
        Kind::Pdf,
        Kind::Rename,
        Kind::Qr,
        Kind::Ytdlp(Fetch::default()),
    ];
    for kind in kinds {
        let icon = Action::new(kind).icon();
        assert!(gpui::AssetSource::load(&crate::icons::Assets, icon).expect("asset").is_some(), "{icon}");
    }
}

#[test]
fn saved_action_format_is_stable() {
    let action = Action::new(Kind::Move { folder: r"D:\Inbox".into() });
    assert_eq!(
        serde_json::to_value(&action).expect("json"),
        serde_json::json!({"type": "move", "folder": r"D:\Inbox"})
    );
    let named = Action { name: "Desk".into(), ..Action::new(Kind::CopyPath) };
    assert_eq!(serde_json::to_value(&named).expect("json"), serde_json::json!({"name": "Desk", "type": "copy_path"}));
    assert_eq!(serde_json::to_value(Action::new(Kind::Phone)).expect("json"), serde_json::json!({"type": "phone"}));
    let labelled = serde_json::json!({"description": "В архив", "icon_only": true, "type": "zip"});
    let read: Action = serde_json::from_value(labelled.clone()).expect("labelled");
    assert!(read.icon_only && read.description == "В архив" && matches!(read.kind, Kind::Zip));
    assert_eq!(serde_json::to_value(&read).expect("json"), labelled);
    let upload: Action =
        serde_json::from_str(r#"{"type": "upload", "url": "https://x", "field": "file"}"#).expect("upload");
    let Kind::Upload(upload) = upload.kind else {
        panic!("upload");
    };
    assert!(upload.header.is_empty() && upload.form.is_empty() && upload.link.is_empty());
    assert_eq!(
        serde_json::to_value(Action::new(Kind::DataUri)).expect("json"),
        serde_json::json!({"type": "data_uri"})
    );
    let hash = Action::new(Kind::Hash { algorithm: Algorithm::Md5, virustotal: "key".into() });
    assert_eq!(
        serde_json::to_value(&hash).expect("json"),
        serde_json::json!({"type": "hash", "algorithm": "md5", "virustotal": "key"})
    );
    let plain: Action = serde_json::from_str(r#"{"type": "hash"}"#).expect("hash");
    assert!(matches!(plain.kind, Kind::Hash { algorithm: Algorithm::Sha256, ref virustotal } if virustotal.is_empty()));
    assert_eq!(serde_json::to_value(&plain).expect("json"), serde_json::json!({"type": "hash", "algorithm": "sha256"}));
    let webp = Action::new(Kind::Compress { format: Compression::Webp });
    assert_eq!(serde_json::to_value(&webp).expect("json"), serde_json::json!({"type": "compress", "format": "webp"}));
    let compress: Action = serde_json::from_str(r#"{"type": "compress"}"#).expect("compress");
    assert!(matches!(compress.kind, Kind::Compress { format: Compression::Jpeg }));
    let english = Action::new(ocr("en-US"));
    assert_eq!(serde_json::to_value(&english).expect("json"), serde_json::json!({"type": "ocr", "language": "en-US"}));
    let profile: Action = serde_json::from_str(r#"{"type": "ocr"}"#).expect("ocr");
    assert!(matches!(profile.kind, Kind::Ocr { ref language } if language.is_empty()));
    assert_eq!(serde_json::to_value(&profile).expect("json"), serde_json::json!({"type": "ocr"}));
    let bare: Action = serde_json::from_str(r#"{"type": "ytdlp"}"#).expect("ytdlp");
    assert!(matches!(bare.kind, Kind::Ytdlp(fetch) if fetch == Fetch::default()));
    let chosen = Fetch {
        mode: Mode::Audio,
        quality: Quality::Height(720),
        subtitles: true,
        cover: false,
        sponsors: true,
        playlist: true,
    };
    let saved = serde_json::to_value(Action::new(Kind::Ytdlp(chosen))).expect("json");
    assert_eq!(
        saved,
        serde_json::json!({"type": "ytdlp", "mode": "audio", "quality": {"height": 720}, "subtitles": true,
            "cover": false, "sponsors": true})
    );
    let restored: Action = serde_json::from_value(saved).expect("restored");
    assert!(matches!(restored.kind, Kind::Ytdlp(fetch) if fetch == Fetch { playlist: false, ..chosen }));
    for broken in
        [r#"{"type": "teleport"}"#, r#"{"type": "move"}"#, r#"{"folder": "x"}"#, r#"{"type": "upload", "url": "x"}"#]
    {
        assert!(serde_json::from_str::<Action>(broken).is_err(), "{broken}");
    }
}

#[test]
fn first_load_creates_default_tiles() {
    let bench = Bench::new();
    let desktop = bench.sandbox.folder("Desktop");
    bench.fake.state().desktop = Some(desktop.clone());
    let actions = load(&bench.storage, &bench.fake).expect("load");
    assert_eq!(actions.len(), 5);
    assert!(matches!(&actions[0].kind, Kind::Copy { folder } if *folder == desktop));
    assert_eq!(actions[0].name, "Shown Desktop");
    assert!(matches!(actions[1].kind, Kind::Zip));
    assert!(matches!(actions[2].kind, Kind::Png));
    assert!(matches!(&actions[3].kind, Kind::Search { url } if url == ENGINES[0]));
    assert!(matches!(actions[4].kind, Kind::Cancel));
    assert!(bench.storage.root().join("actions.json").exists());
    assert!(!bench.storage.root().join("actions.tmp").exists());
    bench.fake.state().desktop = None;
    let again = load(&bench.storage, &bench.fake).expect("again");
    assert_eq!(again.len(), 5);
}

#[test]
fn defaults_without_desktop_skip_the_copy_tile() {
    let bench = Bench::new();
    let actions = reset(&bench.storage, &bench.fake).expect("reset");
    assert_eq!(actions.len(), 4);
    assert!(matches!(actions[0].kind, Kind::Zip));
}

#[test]
fn defaults_are_returned_even_if_they_cannot_be_saved() {
    let sandbox = Sandbox::new();
    let fake = Fake::new(&sandbox);
    let storage = Storage::new(sandbox.join("missing"));
    assert_eq!(load(&storage, &fake).expect("load").len(), 4);
    assert!(!storage.root().exists());
}

#[test]
fn corrupt_actions_are_backed_up_and_reported() {
    let bench = Bench::new();
    let path = bench.storage.root().join("actions.json");
    fs::write(&path, "[{\"type\": ").expect("write");
    assert!(load(&bench.storage, &bench.fake).is_err());
    assert_eq!(fs::read_to_string(bench.storage.root().join("actions.broken.json")).expect("backup"), "[{\"type\": ");
    assert_eq!(fs::read_to_string(&path).expect("kept"), "[{\"type\": ");
}

#[test]
fn unreadable_actions_are_reported_without_backup() {
    let bench = Bench::new();
    fs::create_dir(bench.storage.root().join("actions.json")).expect("dir");
    assert!(load(&bench.storage, &bench.fake).is_err());
    assert!(!bench.storage.root().join("actions.broken.json").exists());
}

#[test]
fn duplicate_tiles_are_removed_and_saved() {
    let bench = Bench::new();
    let actions = vec![
        Action::new(Kind::Move { folder: r"D:\Inbox".into() }),
        Action::new(Kind::Zip),
        Action::new(Kind::Copy { folder: r"d:\inbox\".into() }),
        Action::new(Kind::Zip),
    ];
    save(&bench.storage, &actions).expect("save");
    let loaded = load(&bench.storage, &bench.fake).expect("load");
    assert_eq!(loaded.len(), 3);
    assert!(matches!(loaded[0].kind, Kind::Move { .. }));
    assert!(matches!(loaded[2].kind, Kind::Cancel));
    let saved: Vec<Action> =
        serde_json::from_slice(&fs::read(bench.storage.root().join("actions.json")).expect("read")).expect("json");
    assert_eq!(saved.len(), 2);
}

fn search(index: usize) -> Action {
    Action::new(Kind::Search { url: format!("https://find{index}.example/?q=%s") })
}

fn empty_tiles(bench: &Bench) -> Tiles {
    save(&bench.storage, &[]).expect("save");
    let mut tiles = Tiles::new(bench.storage.clone());
    tiles.load(&bench.fake).expect("load");
    tiles
}

fn order(tiles: &Tiles) -> Vec<TileId> {
    tiles.actions().map(|tile| tile.id).collect()
}

#[test]
fn moved_tile_takes_the_place_of_the_target_and_persists() {
    let bench = Bench::new();
    let mut tiles = empty_tiles(&bench);
    let [a, b, c, d] =
        [Kind::Zip, Kind::Png, Kind::Pdf, Kind::Qr].map(|kind| tiles.add(Action::new(kind)).expect("added"));
    assert!(tiles.move_to(a, d), "down to the last place");
    assert_eq!(order(&tiles), [b, c, d, a]);
    assert!(tiles.move_to(a, b), "up to the first place");
    assert_eq!(order(&tiles), [a, b, c, d]);
    assert!(tiles.move_to(c, b), "one step up");
    assert_eq!(order(&tiles), [a, c, b, d]);
    assert!(tiles.move_to(c, b), "one step down");
    assert_eq!(order(&tiles), [a, b, c, d]);
    assert!(!tiles.move_to(c, c));
    tiles.remove(d).expect("removed");
    assert!(!tiles.move_to(a, d), "a removed target");
    assert!(!tiles.move_to(d, a), "a removed tile");
    assert!(tiles.move_to(a, c));
    tiles.save().expect("save");
    let mut loaded = Tiles::new(bench.storage.clone());
    loaded.load(&bench.fake).expect("load");
    let kinds: Vec<&Kind> = loaded.actions().map(|tile| &tile.action.kind).collect();
    assert!(matches!(kinds[..], [Kind::Png, Kind::Pdf, Kind::Zip]), "the saved order is loaded back");
}

#[test]
fn tiles_stop_at_the_limit_until_one_is_removed() {
    let bench = Bench::new();
    let mut tiles = empty_tiles(&bench);
    let added: Vec<TileId> = (0..MAX_TILES).map(|index| tiles.add(search(index)).expect("below the limit")).collect();
    assert!(tiles.full());
    assert!(tiles.add(search(MAX_TILES)).is_none());
    assert!(tiles.add(Action::new(Kind::Run { program: r"C:\Tools\tool.exe".into() })).is_none());
    assert_eq!(tiles.len(), MAX_TILES);
    tiles.remove(added[3]).expect("removed");
    assert!(!tiles.full());
    assert!(tiles.add(Action::new(Kind::Zip)).is_some());
    assert!(tiles.add(Action::new(Kind::Png)).is_none());
}

#[test]
fn tiles_above_the_limit_from_an_older_file_are_kept() {
    let bench = Bench::new();
    let many: Vec<Action> = (0..MAX_TILES + 7).map(search).collect();
    save(&bench.storage, &many).expect("save");
    let mut tiles = Tiles::new(bench.storage.clone());
    tiles.load(&bench.fake).expect("load");
    assert_eq!(tiles.len(), MAX_TILES + 7);
    assert!(tiles.full());
    assert!(tiles.add(Action::new(Kind::Zip)).is_none());
    tiles.save().expect("save");
    let reloaded = load(&bench.storage, &bench.fake).expect("reload");
    assert_eq!(reloaded.iter().filter(|action| !action.cancels()).count(), MAX_TILES + 7);
}

#[test]
fn a_tool_scan_applies_only_while_nothing_newer_is_known() {
    let bench = Bench::new();
    let mut tiles = empty_tiles(&bench);
    let found = |presence: Presence| Tool::ALL.into_iter().map(|tool| (tool, presence)).collect();
    let older = tiles.begin_scan();
    let newer = tiles.begin_scan();
    assert!(tiles.finish_scan(newer, found(Presence::Bundled), Vec::new()));
    assert!(!tiles.finish_scan(older, found(Presence::Missing), Vec::new()), "an older scan arriving late is dropped");
    assert!(tiles.presence(Tool::Ffmpeg) == Presence::Bundled);
    let started = tiles.begin_scan();
    tiles.set_plugins(Vec::new());
    assert!(!tiles.finish_scan(started, found(Presence::Missing), Vec::new()), "plugins applied after it began win");
    assert!(tiles.presence(Tool::Ffmpeg) == Presence::Bundled);
}

#[test]
fn older_files_get_the_cancel_tile_which_moves_but_cannot_be_removed() {
    let bench = Bench::new();
    save(&bench.storage, &[Action::new(Kind::Zip), Action::new(Kind::Png)]).expect("save");
    let mut tiles = Tiles::new(bench.storage.clone());
    tiles.load(&bench.fake).expect("load");
    let kinds: Vec<&Kind> = tiles.iter().map(|tile| &tile.action.kind).collect();
    assert!(matches!(kinds[..], [Kind::Zip, Kind::Png, Kind::Cancel]), "appended after the tiles of an older file");
    tiles.add(Action::new(Kind::Pdf)).expect("added");
    let kinds: Vec<&Kind> = tiles.iter().map(|tile| &tile.action.kind).collect();
    assert!(matches!(kinds[..], [Kind::Zip, Kind::Png, Kind::Pdf, Kind::Cancel]), "a new tile goes before it");
    let cancel = tiles.find(&Action::new(Kind::Cancel).slot()).expect("cancel tile");
    assert!(tiles.remove(cancel).is_none());
    assert!(tiles.add(Action::new(Kind::Cancel)).is_none(), "only one cancel tile");
    let first = tiles.iter().next().map(|tile| tile.id).expect("first");
    assert!(tiles.move_to(cancel, first), "it moves like any tile");
    tiles.save().expect("save");
    let reloaded = load(&bench.storage, &bench.fake).expect("reload");
    assert!(matches!(reloaded.first(), Some(Action { kind: Kind::Cancel, .. })), "its place is kept");
}

#[test]
fn unconfigured_action_fails_before_any_effect() {
    let bench = Bench::new();
    let file = bench.sandbox.file("a.txt", "x");
    let failure = error(bench.run(Kind::Imgur { client_id: String::new() }, bench.paths(&[&file])));
    assert_eq!(failure, tr().not_configured);
    assert!(bench.fake.calls().is_empty());
}

#[test]
fn empty_input_is_unsupported() {
    let bench = Bench::new();
    assert_eq!(error(bench.run(Kind::Zip, Payload::Paths(Vec::new()))), tr().unsupported);
}

#[test]
fn tiles_accept_only_drags_they_can_process() {
    let file = |name: &str| Offer::Files(vec![Offered { path: PathBuf::from(name), folder: false }]);
    let folder = || Offer::Files(vec![Offered { path: PathBuf::from(r"C:\Мои фото.png"), folder: true }]);
    let mixed = || {
        Offer::Files(vec![
            Offered { path: PathBuf::from(r"C:\Папка"), folder: true },
            Offered { path: PathBuf::from(r"C:\Папка\отчёт.docx"), folder: false },
        ])
    };
    let hash = || Kind::Hash { algorithm: Algorithm::Sha256, virustotal: String::new() };
    let cases = [
        (Kind::Png, file(r"C:\Фото\снимок.JPG"), true),
        (Kind::Png, file("a.PNG"), false),
        (Kind::Png, Offer::Image, false),
        (Kind::Png, Offer::Text, false),
        (Kind::Png, folder(), false),
        (Kind::Png, Offer::Unknown, true),
        (Kind::Jpeg, Offer::Image, true),
        (Kind::Jpeg, file("photo.jpeg"), false),
        (Kind::Gif, file("clip.MP4"), true),
        (Kind::Gif, file("a.png"), false),
        (Kind::Imgur { client_id: String::new() }, Offer::Image, true),
        (Kind::Imgur { client_id: String::new() }, Offer::Text, false),
        (Kind::Compress { format: Compression::Jpeg }, file("a.bmp"), true),
        (Kind::Compress { format: Compression::Jpeg }, file("a.gif"), false),
        (Kind::Ocr { language: String::new() }, file("скан.pdf"), true),
        (Kind::Ocr { language: String::new() }, Offer::Text, false),
        (Kind::Pdf, Offer::Text, true),
        (Kind::Pdf, file("notes.md"), true),
        (Kind::Pdf, file("a.docx"), false),
        (Kind::Unpack, file("архив.7z"), true),
        (Kind::Unpack, file("a.txt"), false),
        (Kind::Qr, Offer::Text, true),
        (Kind::Qr, file("link.url"), true),
        (Kind::Qr, Offer::Image, false),
        (Kind::Ytdlp(Fetch::default()), Offer::Image, false),
        (hash(), folder(), false),
        (hash(), mixed(), true),
        (hash(), Offer::Text, true),
        (Kind::Phone, folder(), false),
        (Kind::DataUri, folder(), false),
        (Kind::CopyPath, folder(), true),
        (Kind::CopyPath, Offer::Text, false),
        (Kind::CopyPath, Offer::Image, false),
        (Kind::CopyPath, Offer::Unknown, true),
        (Kind::Rename, Offer::Image, false),
        (Kind::Move { folder: PathBuf::from(r"C:\Цель") }, Offer::Text, true),
        (Kind::Move { folder: PathBuf::from(r"C:\Цель") }, folder(), true),
        (Kind::Search { url: ENGINES[0].into() }, folder(), true),
        (Kind::Zip, Offer::Image, true),
    ];
    for (kind, offer, expected) in cases {
        let action = Action::new(kind);
        assert_eq!(action.accepts(&offer), expected, "{} with {offer:?}", action.title());
    }
}

#[test]
fn copy_path_needs_files_on_disk() {
    let bench = Bench::new();
    assert_eq!(error(bench.run(Kind::CopyPath, Payload::Text("x".into()))), tr().needs_disk_files);
    assert_eq!(bench.content(), 0);
    let one = bench.sandbox.file("a b.txt", "x");
    let two = bench.sandbox.folder("Папка");
    let single = bench.run(Kind::CopyPath, bench.paths(&[&one])).expect("single");
    assert_eq!(single.clipboard.as_deref(), Some(one.to_string_lossy().as_ref()));
    assert_eq!(single.message, tr().path_copied);
    let both = bench.run(Kind::CopyPath, bench.paths(&[&one, &two])).expect("both");
    assert_eq!(both.clipboard, Some(format!("{}\r\n{}", one.display(), two.display())));
    assert_eq!(both.message, fill(tr().paths_copied, 2));
}

#[test]
fn text_payload_becomes_a_temporary_file_that_is_released() {
    let bench = Bench::new();
    let outcome = bench.run(Kind::Zip, Payload::Text("Shopping list:\nmilk".into())).expect("zip");
    let archive = &outcome.created[0];
    assert!(bench.storage.owned(archive));
    assert_eq!(entries(archive), ["Shopping list.txt"]);
    assert_eq!(read_entry(archive, "Shopping list.txt"), "Shopping list:\nmilk");
    assert_eq!(bench.content(), 1);
}

#[test]
fn image_payload_is_saved_as_png_input() {
    let _english = language(Language::English);
    let bench = Bench::new();
    let outcome = bench.run(Kind::Jpeg, Payload::Image(ImageData::Png(png(3, 2)))).expect("convert");
    assert_eq!(outcome.created.len(), 1);
    let created = &outcome.created[0];
    assert_eq!(created.file_name().and_then(|name| name.to_str()), Some("Image 3 × 2.jpg"));
    assert_eq!(store::open_image(created).expect("jpeg").width(), 3);
    assert_eq!(bench.content(), 1);
}

#[test]
fn broken_image_payload_leaves_nothing_behind() {
    let bench = Bench::new();
    assert!(bench.run(Kind::Png, Payload::Image(ImageData::Png(b"junk".to_vec()))).is_err());
    assert_eq!(bench.content(), 0);
}

#[test]
fn stored_payload_is_released_after_the_action() {
    let bench = Bench::new();
    let stored = save_virtual(&bench.storage, "mail.txt", b"attachment").expect("virtual");
    let mut scratch = Scratch::new(bench.storage.clone());
    scratch.push(stored.clone());
    let outcome = bench.run(Kind::Zip, Payload::Stored(scratch)).expect("zip");
    assert!(!stored.exists());
    assert!(outcome.created[0].exists());
}

#[test]
fn phone_shares_only_files_and_keeps_temporary_inputs() {
    let bench = Bench::new();
    let file = bench.sandbox.file("a.txt", "a");
    let folder = bench.sandbox.folder("Folder");
    let server = shared(bench.run(Kind::Phone, bench.paths(&[&folder, &file])).expect("share"));
    assert_eq!(server.files, std::slice::from_ref(&file));
    assert_eq!(bench.fake.calls(), [Call::Serve(vec![file])]);
    let note = shared(bench.run(Kind::Phone, Payload::Text("hello".into())).expect("note"));
    assert_eq!(fs::read_to_string(&note.files[0]).expect("kept"), "hello");
}

#[test]
fn phone_refuses_folders_and_reports_server_failures() {
    let bench = Bench::new();
    let folder = bench.sandbox.folder("Folder");
    assert_eq!(error(bench.run(Kind::Phone, bench.paths(&[&folder]))), tr().phone_needs_files);
    assert!(bench.fake.calls().is_empty());
    bench.fake.state().serve_error = Some("port busy".into());
    let file = bench.sandbox.file("a.txt", "a");
    assert_eq!(error(bench.run(Kind::Phone, bench.paths(&[&file]))), "port busy");
}

#[test]
fn launched_program_keeps_its_temporary_inputs() {
    let bench = Bench::new();
    let program = bench.sandbox.file("tool.exe", "MZ");
    let outcome = bench.run(Kind::Run { program: program.clone() }, Payload::Text("hello".into()));
    assert!(outcome.is_ok());
    let invocation = &bench.invocations()[0];
    let input = PathBuf::from(&invocation.args[0]);
    assert!(input.exists());
    assert_eq!(fs::read_to_string(&input).expect("input"), "hello");
    assert_eq!(invocation.dir.as_deref(), input.parent());
}

#[test]
fn transfer_validates_target_and_delegates_to_shell() {
    let bench = Bench::new();
    let target = bench.sandbox.folder("Target");
    let file = bench.sandbox.file("docs/a.txt", "x");
    let already = bench.sandbox.file("TARGET/b.txt", "x");
    let missing = bench.sandbox.join("nowhere");
    assert_eq!(
        error(bench.run(Kind::Move { folder: missing.clone() }, bench.paths(&[&file]))),
        fill(tr().folder_missing, missing.display())
    );
    assert_eq!(error(bench.run(Kind::Move { folder: target.clone() }, bench.paths(&[&already]))), tr().already_there);
    assert!(bench.fake.calls().is_empty());
    let mut action = Action::new(Kind::Copy { folder: target.clone() });
    action.name = "Archive".into();
    let outcome =
        run(&action, bench.paths(&[&file, &already]), &Progress::default(), &bench.storage, &bench.fake).expect("copy");
    assert_eq!(outcome.message, fill(tr().copied_to, "Archive"));
    assert_eq!(bench.fake.calls(), [Call::Transfer(vec![file.clone()], target.clone(), true)]);
    let moved = bench.run(Kind::Move { folder: target.clone() }, bench.paths(&[&file])).expect("move");
    assert_eq!(moved.message, fill(tr().moved_to, "Target"));
    assert!(file.exists());
}

#[test]
fn transfer_refuses_moving_a_folder_into_itself() {
    let bench = Bench::new();
    let folder = bench.sandbox.folder("Projects");
    let inner = bench.sandbox.folder("Projects/Inner");
    let sibling = bench.sandbox.folder("Projects2");
    assert_eq!(error(bench.run(Kind::Move { folder: inner.clone() }, bench.paths(&[&folder]))), tr().inside_itself);
    let typed = PathBuf::from(inner.to_string_lossy().replace('\\', "/"));
    assert_eq!(error(bench.run(Kind::Move { folder: typed.clone() }, bench.paths(&[&folder]))), tr().inside_itself);
    let file = bench.sandbox.file("Projects/Inner/a.txt", "x");
    assert_eq!(error(bench.run(Kind::Copy { folder: typed }, bench.paths(&[&file]))), tr().already_there);
    assert!(bench.run(Kind::Move { folder: sibling }, bench.paths(&[&folder])).is_ok());
}

#[test]
fn transfer_failure_is_reported() {
    let bench = Bench::new();
    let target = bench.sandbox.folder("Target");
    let file = bench.sandbox.file("a.txt", "x");
    bench.fake.state().transfer_error = Some("code 1223".into());
    assert_eq!(error(bench.run(Kind::Copy { folder: target }, bench.paths(&[&file]))), "code 1223");
}

#[test]
fn zip_of_one_file_lands_next_to_it() {
    let bench = Bench::new();
    let file = bench.sandbox.file("docs/отчёт.txt", "data");
    let outcome = bench.run(Kind::Zip, bench.paths(&[&file])).expect("zip");
    let archive = bench.sandbox.join("docs/отчёт.zip");
    assert_eq!(outcome.created, std::slice::from_ref(&archive));
    assert_eq!(outcome.clipboard, Some(archive.display().to_string()));
    assert_eq!(outcome.message, tr().zipped);
    assert_eq!(entries(&archive), ["отчёт.txt"]);
    assert_eq!(read_entry(&archive, "отчёт.txt"), "data");
}

#[test]
fn zip_of_folder_keeps_structure_and_avoids_name_conflicts() {
    let bench = Bench::new();
    bench.sandbox.file("Project/a.txt", "a");
    bench.sandbox.file("Project/sub/b.txt", "b");
    bench.sandbox.folder("Project/empty");
    bench.sandbox.file("Project.zip", "existing");
    let folder = bench.sandbox.join("Project");
    let outcome = bench.run(Kind::Zip, bench.paths(&[&folder])).expect("zip");
    let archive = bench.sandbox.join("Project (2).zip");
    assert_eq!(outcome.created, std::slice::from_ref(&archive));
    assert_eq!(entries(&archive), ["Project/", "Project/a.txt", "Project/empty/", "Project/sub/", "Project/sub/b.txt"]);
    assert_eq!(fs::read_to_string(bench.sandbox.join("Project.zip")).expect("kept"), "existing");
}

#[test]
fn zip_of_several_files_uses_archive_name() {
    let bench = Bench::new();
    let a = bench.sandbox.file("a.txt", "a");
    let b = bench.sandbox.file("b.txt", "b");
    let outcome = bench.run(Kind::Zip, bench.paths(&[&a, &b])).expect("zip");
    assert_eq!(outcome.created, [bench.sandbox.join(format!("{}.zip", tr().archive))]);
    assert_eq!(entries(&outcome.created[0]), ["a.txt", "b.txt"]);
}

#[test]
fn failed_zip_removes_partial_archive() {
    let bench = Bench::new();
    let a = bench.sandbox.file("a.txt", "a");
    let gone = bench.sandbox.join("gone.txt");
    assert!(bench.run(Kind::Zip, bench.paths(&[&a, &gone])).is_err());
    let leftovers: Vec<_> = fs::read_dir(bench.sandbox.path())
        .expect("sandbox")
        .flatten()
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "zip"))
        .collect();
    assert!(leftovers.is_empty());
}

#[test]
fn zip_of_stored_file_stays_in_storage() {
    let bench = Bench::new();
    let stored = save_virtual(&bench.storage, "note.txt", b"x").expect("virtual");
    let outcome = bench.run(Kind::Zip, bench.paths(&[&stored])).expect("zip");
    assert!(bench.storage.owned(&outcome.created[0]));
    assert!(stored.exists());
}

#[test]
fn conversion_writes_next_to_source_and_flattens_transparency() {
    let bench = Bench::new();
    let image = image::RgbaImage::from_pixel(2, 2, image::Rgba([0, 0, 0, 0]));
    let source = bench.sandbox.join("pics/clear.png");
    fs::create_dir_all(source.parent().expect("parent")).expect("dir");
    image.save(&source).expect("png");
    let outcome = bench.run(Kind::Jpeg, bench.paths(&[&source])).expect("convert");
    let target = bench.sandbox.join("pics/clear.jpg");
    assert_eq!(outcome.created, std::slice::from_ref(&target));
    let pixel = store::open_image(&target).expect("jpeg").to_rgb8().get_pixel(0, 0).0;
    assert!(pixel.iter().all(|channel| *channel > 240), "{pixel:?}");
}

#[test]
fn conversion_skips_same_format_and_non_images() {
    let bench = Bench::new();
    let png_file = bench.sandbox.file("a.png", png(1, 1));
    let text = bench.sandbox.file("b.txt", "x");
    assert_eq!(error(bench.run(Kind::Png, bench.paths(&[&png_file]))), fill(tr().already_format, "PNG"));
    assert_eq!(error(bench.run(Kind::Png, bench.paths(&[&text]))), tr().needs_images);
    assert_eq!(error(bench.run(Kind::Png, bench.paths(&[&png_file, &text]))), tr().needs_images);
    let jpeg = bench.sandbox.join("c.JPG");
    image::RgbImage::new(2, 2).save_with_format(&jpeg, image::ImageFormat::Jpeg).expect("jpeg");
    let outcome = bench.run(Kind::Png, bench.paths(&[&png_file, &jpeg, &text])).expect("partial");
    assert_eq!(outcome.created, [bench.sandbox.join("c.png")]);
    assert_eq!(outcome.message, format!("{} · {}", fill(tr().converted, 1), fill(tr().skipped, 2)));
    assert_eq!(error(bench.run(Kind::Jpeg, bench.paths(&[&jpeg]))), fill(tr().already_format, "JPG"));
}

#[test]
fn corrupt_images_are_reported_with_file_name() {
    let bench = Bench::new();
    let broken = bench.sandbox.file("broken.png", "not a png");
    let good = bench.sandbox.file("good.png", png(1, 1));
    let failure = format!("{:#}", bench.run(Kind::Jpeg, bench.paths(&[&broken])).err().expect("error"));
    assert!(failure.starts_with("broken.png"), "{failure}");
    assert!(!bench.sandbox.join("broken.jpg").exists());
    let partial = bench.run(Kind::Jpeg, bench.paths(&[&broken, &good])).expect("partial");
    assert_eq!(partial.created, [bench.sandbox.join("good.jpg")]);
}

#[test]
fn conversion_reports_progress_for_batches() {
    let bench = Bench::new();
    let files: Vec<PathBuf> = (0..4).map(|n| bench.sandbox.file(&format!("{n}.png"), png(1, 1))).collect();
    let progress = Progress::default();
    run(&Action::new(Kind::Jpeg), Payload::Paths(files), &progress, &bench.storage, &bench.fake).expect("convert");
    assert_eq!(progress.get().fraction, Some(0.75));
}

#[test]
fn gif_requires_videos_and_ffmpeg() {
    let bench = Bench::new();
    let text = bench.sandbox.file("a.txt", "x");
    let video = bench.sandbox.file("clip.MP4", "video");
    assert_eq!(error(bench.run(Kind::Gif, bench.paths(&[&text]))), tr().needs_video);
    assert_eq!(error(bench.run(Kind::Gif, bench.paths(&[&video]))), fill(tr().tool_missing, "ffmpeg"));
    assert!(bench.invocations().is_empty());
}

#[test]
fn gif_runs_bundled_ffmpeg_with_expected_arguments() {
    let bench = Bench::new();
    let ffmpeg = bench.sandbox.file("root/tools/ffmpeg.exe", "MZ");
    let video = bench.sandbox.file("clips/my clip.mov", "video");
    let text = bench.sandbox.file("clips/readme.txt", "x");
    bench.respond(Ok(output(0, "", "")));
    let outcome = bench.run(Kind::Gif, bench.paths(&[&video, &text])).expect("gif");
    let target = bench.sandbox.join("clips/my clip.gif");
    assert_eq!(outcome.created, std::slice::from_ref(&target));
    assert_eq!(outcome.message, format!("{} · {}", fill(tr().converted, 1), fill(tr().skipped, 1)));
    let invocation = &bench.invocations()[0];
    assert_eq!(invocation.program, ffmpeg);
    assert_eq!(invocation.input, None);
    let args = invocation.args();
    assert_eq!(args[..4], ["-v", "error", "-y", "-i"]);
    assert_eq!(args[4], video.display().to_string());
    assert_eq!(args[5], "-vf");
    assert!(args[6].contains("palettegen"));
    assert_eq!(args[7..9], ["-loop", "0"]);
    assert_eq!(args[9], target.display().to_string());
}

#[test]
fn gif_uses_ffmpeg_from_search_path() {
    let bench = Bench::new();
    let ffmpeg = bench.sandbox.file("bin/ffmpeg.exe", "MZ");
    bench.fake.state().search_path =
        Some(std::env::join_paths([bench.sandbox.join("empty"), bench.sandbox.join("bin")]).expect("path"));
    let video = bench.sandbox.file("clip.webm", "video");
    bench.respond(Ok(output(0, "", "")));
    bench.run(Kind::Gif, bench.paths(&[&video])).expect("gif");
    assert_eq!(bench.invocations()[0].program, ffmpeg);
}

#[test]
fn failed_gif_reports_ffmpeg_error_and_removes_output() {
    let bench = Bench::new();
    bench.sandbox.file("root/tools/ffmpeg.exe", "MZ");
    let video = bench.sandbox.file("clip.mkv", "video");
    bench.respond(Ok(output(1, "", "frame=1\r\nInvalid data found when processing input\r\n")));
    let failure = format!("{:#}", bench.run(Kind::Gif, bench.paths(&[&video])).err().expect("error"));
    assert!(failure.ends_with("Invalid data found when processing input"), "{failure}");
    assert!(!bench.sandbox.join("clip.gif").exists());
    bench.respond(Err(io::ErrorKind::PermissionDenied.into()));
    assert!(bench.run(Kind::Gif, bench.paths(&[&video])).is_err());
    assert!(!bench.sandbox.join("clip.gif").exists());
}

#[test]
fn imgur_upload_copies_the_image_link() {
    let bench = Bench::new();
    let image = bench.sandbox.file("my photo.png", png(1, 1));
    bench.respond(Ok(output(
        0,
        r#"{"data":{"id":"x","link":"https://i.imgur.com/x.png"},"success":true}
200"#,
        "",
    )));
    let outcome = bench.run(Kind::Imgur { client_id: " abc ".into() }, bench.paths(&[&image])).expect("upload");
    assert_eq!(outcome.clipboard.as_deref(), Some("https://i.imgur.com/x.png"));
    assert_eq!(outcome.message, tr().link_copied);
    let invocation = &bench.invocations()[0];
    assert!(invocation.program.ends_with("curl.exe"));
    assert_eq!(invocation.input.as_deref(), Some(png(1, 1).as_slice()));
    let args = invocation.args();
    assert!(args.windows(2).any(|pair| pair == ["-F", "image=@-;filename=my_photo.png"]), "{args:?}");
    assert!(args.windows(2).any(|pair| pair == ["-H", "Authorization: Client-ID abc"]), "{args:?}");
    assert_eq!(args.last().map(String::as_str), Some(IMGUR_URL));
}

#[test]
fn imgur_accepts_only_images_within_limit() {
    let bench = Bench::new();
    let text = bench.sandbox.file("a.txt", "x");
    assert_eq!(error(bench.run(Kind::Imgur { client_id: "id".into() }, bench.paths(&[&text]))), tr().needs_images);
    let big = bench.sandbox.join("big.png");
    File::create(&big).expect("big").set_len(IMGUR_LIMIT + 1).expect("size");
    assert_eq!(
        error(bench.run(Kind::Imgur { client_id: "id".into() }, bench.paths(&[&big]))),
        fill(tr().too_big, "big.png")
    );
    assert!(bench.invocations().is_empty());
}

#[test]
fn custom_upload_sends_form_fields_and_extracts_link() {
    let bench = Bench::new();
    let first = bench.sandbox.file("a.pdf", "pdf");
    let second = bench.sandbox.file("b.pdf", "pdf");
    let upload = Upload {
        url: " https://up.example/api ".into(),
        field: " file ".into(),
        header: "  ".into(),
        form: [("expires".to_string(), "1 day".to_string())].into(),
        link: "/files/0/url".into(),
    };
    bench.respond(Ok(output(0, "{\"files\":[{\"url\":\"https://up.example/a\"}]}\n201", "")));
    bench.respond(Ok(output(0, "{\"files\":[]}\n200", "")));
    let outcome = bench.run(Kind::Upload(upload), bench.paths(&[&first, &second])).expect("upload");
    assert_eq!(outcome.clipboard.as_deref(), Some("https://up.example/a"));
    assert_eq!(outcome.message, format!("{} · {}", tr().link_copied, fill(tr().skipped, 1)));
    let args = bench.invocations()[0].args();
    assert!(args.windows(2).any(|pair| pair == ["--form-string", "expires=1 day"]), "{args:?}");
    assert!(args.windows(2).any(|pair| pair == ["-F", "file=@-;filename=a.pdf"]), "{args:?}");
    assert!(!args.contains(&"-H".to_string()));
    assert_eq!(args.last().map(String::as_str), Some("https://up.example/api"));
}

#[test]
fn upload_without_link_pointer_returns_plain_body() {
    let bench = Bench::new();
    let file = bench.sandbox.file("a.txt", "x");
    let upload = Upload { url: "https://paste.example".into(), field: "f".into(), ..Upload::default() };
    bench.respond(Ok(output(0, "  https://paste.example/abc \n200", "")));
    let outcome = bench.run(Kind::Upload(upload), bench.paths(&[&file])).expect("upload");
    assert_eq!(outcome.clipboard.as_deref(), Some("https://paste.example/abc"));
}

#[test]
fn upload_errors_are_translated() {
    let bench = Bench::new();
    let file = bench.sandbox.file("a.txt", "x");
    let upload = Kind::Upload(Upload { url: "https://up.example".into(), field: "f".into(), ..Upload::default() });
    let cases = [
        (Ok(output(0, "denied\n401", "")), tr().key_rejected.to_string()),
        (Ok(output(0, "denied\n403", "")), tr().key_rejected.to_string()),
        (Ok(output(0, "oops\n500", "")), "HTTP 500: oops".to_string()),
        (Ok(output(6, "", "")), tr().no_connection.to_string()),
        (Ok(output(7, "", "")), tr().no_connection.to_string()),
        (Ok(output(28, "", "")), tr().timed_out.to_string()),
        (Ok(output(35, "", "curl: (35) TLS failure\n")), "curl: (35) TLS failure".to_string()),
    ];
    for (response, expected) in cases {
        bench.respond(response);
        assert_eq!(error(bench.run(upload.clone(), bench.paths(&[&file]))), expected);
    }
    bench.respond(Err(io::ErrorKind::NotFound.into()));
    assert!(bench.run(upload, bench.paths(&[&file])).is_err());
}

#[test]
fn long_error_bodies_are_truncated() {
    let bench = Bench::new();
    let file = bench.sandbox.file("a.txt", "x");
    let upload = Kind::Upload(Upload { url: "https://up.example".into(), field: "f".into(), ..Upload::default() });
    bench.respond(Ok(output(0, &format!("{}\n502", "é".repeat(500)), "")));
    let failure = error(bench.run(upload, bench.paths(&[&file])));
    assert_eq!(failure, format!("HTTP 502: {}", "é".repeat(160)));
}

#[test]
fn search_builds_query_from_names_and_notes() {
    let bench = Bench::new();
    let file = bench.sandbox.file("my_report.final.pdf", "x");
    let folder = bench.sandbox.folder("Old.Photos");
    bench.run(Kind::Search { url: ENGINES[0].into() }, bench.paths(&[&file, &folder])).expect("search");
    bench
        .run(Kind::Search { url: " https://example.com/find/ ".into() }, Payload::Text("  привет,\n мир & co  ".into()))
        .expect("text");
    assert_eq!(
        bench.opened(),
        [
            "https://www.google.com/search?q=my%20report%20final%20Old%20Photos".to_string(),
            "https://example.com/find/%D0%BF%D1%80%D0%B8%D0%B2%D0%B5%D1%82%2C%20%D0%BC%D0%B8%D1%80%20%26%20co"
                .to_string(),
        ]
    );
    assert_eq!(bench.content(), 0);
}

#[test]
fn search_reads_only_small_stored_notes() {
    let bench = Bench::new();
    let external = bench.sandbox.file("secret.txt", "do not read");
    let stored = save_virtual(&bench.storage, "note.txt", b"stored words").expect("note");
    let large = save_virtual(&bench.storage, "large_note.txt", "w ".repeat(40_000).as_bytes()).expect("large");
    bench.run(Kind::Search { url: ENGINES[0].into() }, bench.paths(&[&external, &stored, &large])).expect("search");
    assert_eq!(bench.opened(), ["https://www.google.com/search?q=secret%20stored%20words%20large%20note"]);
}

#[test]
fn search_query_is_limited_and_must_not_be_empty() {
    let bench = Bench::new();
    bench
        .run(Kind::Search { url: "https://s.example/?q=%s&x=%s".into() }, Payload::Text("a".repeat(5000)))
        .expect("long");
    let opened = &bench.opened()[0];
    assert_eq!(opened, &format!("https://s.example/?q={0}&x={0}", "a".repeat(2000)));
    let blank = save_virtual(&bench.storage, "blank.txt", b" \n\t ").expect("blank");
    assert_eq!(error(bench.run(Kind::Search { url: ENGINES[0].into() }, bench.paths(&[&blank]))), tr().needs_text);
}

#[test]
fn image_search_with_google_opens_local_lens_page() {
    let bench = Bench::new();
    let image = bench.sandbox.file("Фото 1.png", png(1, 1));
    bench.run(Kind::Search { url: ENGINES[0].into() }, bench.paths(&[&image])).expect("lens");
    let page = bench.fake.temp_dir().join("traydex-lens.html");
    assert_eq!(bench.opened(), [page.to_string_lossy().into_owned()]);
    let html = fs::read_to_string(&page).expect("page");
    assert!(html.contains("https://lens.google.com/v3/upload"));
    assert!(html.contains("\"_____1.png\""));
    assert!(html.contains("image/png"));
    assert!(bench.invocations().is_empty());
}

#[test]
fn image_search_with_yandex_follows_redirect_or_reads_id() {
    let bench = Bench::new();
    let image = bench.sandbox.file("pic.jpg", "jpeg bytes");
    let yandex = Kind::Search { url: ENGINES[1].into() };
    bench.respond(Ok(output(0, "\nhttps://yandex.ru/images/search?cbir_id=1\n302", "")));
    bench.run(yandex.clone(), bench.paths(&[&image])).expect("redirect");
    bench.respond(Ok(output(0, "{\"blocks\":[{\"params\":{\"cbirId\":\"a/b\"}}]}\n\n200", "")));
    bench.run(yandex.clone(), bench.paths(&[&image])).expect("json");
    bench.respond(Ok(output(0, "{}\n\n200", "")));
    assert!(bench.run(yandex, bench.paths(&[&image])).is_err());
    let opened = bench.opened();
    assert_eq!(opened[0], "https://yandex.ru/images/search?cbir_id=1");
    assert!(opened[1].ends_with("&cbir_id=a%2Fb"), "{}", opened[1]);
    assert_eq!(opened.len(), 2);
    let invocation = &bench.invocations()[0];
    assert_eq!(invocation.input.as_deref(), Some(b"jpeg bytes".as_slice()));
    assert!(invocation.args().contains(&"upfile=@-;filename=pic.jpg".to_string()));
}

#[test]
fn image_search_with_bing_sends_base64() {
    let bench = Bench::new();
    let image = bench.sandbox.file("pic.gif", "abc");
    bench.respond(Ok(output(0, "\nhttps://www.bing.com/images/result\n302", "")));
    bench.run(Kind::Search { url: ENGINES[2].into() }, bench.paths(&[&image])).expect("bing");
    assert_eq!(bench.invocations()[0].input.as_deref(), Some(b"YWJj".as_slice()));
    assert_eq!(bench.opened(), ["https://www.bing.com/images/result"]);
}

#[test]
fn run_launches_program_with_files_in_their_folder() {
    let bench = Bench::new();
    let program = bench.sandbox.file("tools/resize.exe", "MZ");
    let a = bench.sandbox.file("pics/a.png", "x");
    let b = bench.sandbox.file("other/b.png", "x");
    let outcome = bench.run(Kind::Run { program: program.clone() }, bench.paths(&[&a, &b])).expect("run");
    assert_eq!(outcome.message, fill(tr().launched, "resize"));
    let invocation = &bench.invocations()[0];
    assert_eq!(invocation.program, program);
    assert_eq!(invocation.args, [a.clone().into_os_string(), b.into_os_string()]);
    assert_eq!(invocation.dir, a.parent().map(Path::to_path_buf));
}

#[test]
fn run_starts_powershell_scripts_through_powershell() {
    let bench = Bench::new();
    let script = bench.sandbox.file("script.PS1", "Write-Host hi");
    let file = bench.sandbox.file("a.txt", "x");
    bench.run(Kind::Run { program: script.clone() }, bench.paths(&[&file])).expect("run");
    let invocation = &bench.invocations()[0];
    assert_eq!(invocation.program, PathBuf::from("powershell.exe"));
    assert_eq!(
        invocation.args(),
        [
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
            &script.display().to_string(),
            &file.display().to_string()
        ]
    );
}

#[test]
fn run_reports_missing_program_and_launch_failure() {
    let bench = Bench::new();
    let file = bench.sandbox.file("a.txt", "x");
    let missing = bench.sandbox.join("gone.exe");
    assert_eq!(
        error(bench.run(Kind::Run { program: missing.clone() }, bench.paths(&[&file]))),
        fill(tr().program_missing, missing.display())
    );
    let program = bench.sandbox.file("tool.exe", "MZ");
    bench.fake.state().launch_error = Some(io::ErrorKind::PermissionDenied);
    assert!(bench.run(Kind::Run { program }, bench.paths(&[&file])).is_err());
}

#[test]
fn report_without_results_uses_first_error_or_fallback() {
    let report = Report::default();
    assert_eq!(report.finish("nothing".into()).err().expect("error").to_string(), "nothing");
    let mut report = Report::default();
    report.fail(Path::new(r"C:\a.png"), anyhow!("first"));
    report.fail(Path::new(r"C:\b.png"), anyhow!("second"));
    assert_eq!(report.skipped, 2);
    assert_eq!(format!("{:#}", report.finish("x".into()).err().expect("error")), "a.png: first");
}

fn shown(outcome: Outcome) -> Show {
    outcome.show.expect("a page to show")
}

fn noise(width: u32, height: u32) -> image::RgbaImage {
    image::RgbaImage::from_fn(width, height, |x, y| {
        let value = (x.wrapping_mul(7919) ^ y.wrapping_mul(104_729)).wrapping_mul(2_654_435_761) >> 24;
        image::Rgba([value as u8, (value >> 1) as u8, (x + y) as u8, 255])
    })
}

const ROTATE_90_EXIF: [u8; 26] = [
    0x49, 0x49, 0x2A, 0x00, 0x08, 0x00, 0x00, 0x00, 0x01, 0x00, 0x12, 0x01, 0x03, 0x00, 0x01, 0x00, 0x00, 0x00, 0x06,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

#[test]
fn data_uri_carries_the_media_type_and_contents() {
    let bench = Bench::new();
    let svg = bench.sandbox.file("icons/logo мой.svg", "<svg/>");
    let note = bench.sandbox.file("note.txt", "hi");
    let outcome = bench.run(Kind::DataUri, bench.paths(&[&svg])).expect("svg");
    assert_eq!(outcome.clipboard.as_deref(), Some("data:image/svg+xml;base64,PHN2Zy8+"));
    assert!(outcome.created.is_empty());
    let both = bench.run(Kind::DataUri, bench.paths(&[&svg, &note])).expect("both");
    assert_eq!(
        both.clipboard.as_deref(),
        Some("data:image/svg+xml;base64,PHN2Zy8+\r\ndata:text/plain;charset=utf-8;base64,aGk=")
    );
    assert_eq!(both.message, fill(tr().base64_many, 2));
    let pasted = bench.run(Kind::DataUri, Payload::Image(ImageData::Png(png(1, 1)))).expect("image");
    assert!(pasted.clipboard.expect("uri").starts_with("data:image/png;base64,iVBORw0KGgo"));
}

#[test]
fn data_uri_refuses_folders_and_huge_files() {
    let bench = Bench::new();
    let folder = bench.sandbox.folder("Folder");
    assert_eq!(error(bench.run(Kind::DataUri, bench.paths(&[&folder]))), tr().needs_file_input);
    let huge = bench.sandbox.join("huge.bin");
    File::create(&huge).expect("file").set_len(10 * 1024 * 1024 + 1).expect("size");
    assert_eq!(error(bench.run(Kind::DataUri, bench.paths(&[&huge]))), fill(tr().base64_too_big, "huge.bin"));
    let limit = bench.sandbox.join("limit.bin");
    File::create(&limit).expect("file").set_len(10 * 1024 * 1024).expect("size");
    assert!(bench.run(Kind::DataUri, bench.paths(&[&limit])).is_ok());
}

#[test]
fn compression_writes_a_smaller_jpeg_next_to_the_original() {
    let bench = Bench::new();
    let source = bench.sandbox.join("фото/снимок экрана.png");
    fs::create_dir_all(source.parent().expect("parent")).expect("dir");
    noise(800, 600).save(&source).expect("png");
    let before = fs::read(&source).expect("source");
    let outcome = bench.run(Kind::Compress { format: Compression::Jpeg }, bench.paths(&[&source])).expect("compress");
    let target = bench.sandbox.join("фото/снимок экрана-min.jpg");
    assert_eq!(outcome.created, std::slice::from_ref(&target));
    assert!(fs::metadata(&target).expect("target").len() < before.len() as u64);
    assert_eq!(fs::read(&source).expect("source"), before);
    let decoded = image::ImageReader::open(&target).expect("open").with_guessed_format().expect("format");
    assert_eq!(decoded.format(), Some(image::ImageFormat::Jpeg));
    let decoded = decoded.decode().expect("decode");
    assert_eq!((decoded.width(), decoded.height()), (800, 600));
    assert!(outcome.message.contains(" → "), "{}", outcome.message);
}

#[test]
fn compression_limits_the_long_side_and_follows_exif_rotation() {
    use image::ImageEncoder;
    let bench = Bench::new();
    let wide = bench.sandbox.join("wide.png");
    noise(3000, 20).save(&wide).expect("png");
    let rotated = bench.sandbox.join("rotated.jpg");
    let pixels = image::DynamicImage::ImageRgba8(noise(400, 200)).into_rgb8();
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(File::create(&rotated).expect("file"), 100);
    encoder.set_exif_metadata(ROTATE_90_EXIF.to_vec()).expect("exif");
    encoder.write_image(pixels.as_raw(), 400, 200, image::ExtendedColorType::Rgb8).expect("jpeg");
    let outcome =
        bench.run(Kind::Compress { format: Compression::Jpeg }, bench.paths(&[&wide, &rotated])).expect("compress");
    let sizes: Vec<(u32, u32)> = outcome
        .created
        .iter()
        .map(|path| store::open_image(path).expect("image"))
        .map(|image| (image.width(), image.height()))
        .collect();
    assert_eq!(sizes, [(2560, 17), (200, 400)]);
}

#[test]
fn webp_compression_keeps_transparency() {
    let bench = Bench::new();
    let source = bench.sandbox.join("logo.png");
    let mut image = noise(300, 300);
    for (x, _, pixel) in image.enumerate_pixels_mut() {
        if x < 150 {
            pixel[3] = 0;
        }
    }
    image.save(&source).expect("png");
    let outcome = bench.run(Kind::Compress { format: Compression::Webp }, bench.paths(&[&source])).expect("webp");
    assert_eq!(outcome.created, [bench.sandbox.join("logo-min.webp")]);
    let decoded = store::open_image(&outcome.created[0]).expect("webp").to_rgba8();
    assert!(decoded.get_pixel(10, 10)[3] < 32);
    assert!(decoded.get_pixel(290, 10)[3] > 223);
}

#[test]
fn already_compact_images_are_left_alone() {
    let bench = Bench::new();
    let tiny = bench.sandbox.file("tiny.png", png(1, 1));
    let text = bench.sandbox.file("a.txt", "x");
    let animation = bench.sandbox.file("a.gif", "gif");
    assert_eq!(
        error(bench.run(Kind::Compress { format: Compression::Jpeg }, bench.paths(&[&tiny]))),
        tr().already_compressed
    );
    assert!(!bench.sandbox.join("tiny-min.jpg").exists());
    assert_eq!(
        error(bench.run(Kind::Compress { format: Compression::Webp }, bench.paths(&[&text, &animation]))),
        tr().compress_needs
    );
}

fn ocr(language: &str) -> Kind {
    Kind::Ocr { language: language.into() }
}

#[test]
fn recognition_reads_every_page_in_the_chosen_language() {
    let bench = Bench::new();
    let picture = bench.sandbox.file("a.png", png(2, 2));
    let scan = bench.sandbox.file("scan.pdf", "%PDF");
    bench.fake.state().pages = Ok(vec![noise(10, 10), noise(10, 10)]);
    bench.fake.state().recognized.extend((0..4).map(|n| Ok(format!("text {n}"))));
    bench.run(ocr("en-US"), bench.paths(&[&scan, &picture])).expect("chosen");
    bench.run(ocr(""), bench.paths(&[&picture])).expect("profile");
    assert_eq!(bench.fake.state().read_in, ["en-US", "en-US", "en-US", ""]);
}

#[test]
fn recognized_text_from_images_and_pdf_pages_goes_to_clipboard() {
    let bench = Bench::new();
    let picture = bench.sandbox.file("a.png", png(1500, 1500));
    let scan = bench.sandbox.file("scan.PDF", "%PDF");
    let archive = bench.sandbox.file("b.zip", "zip");
    bench.fake.state().pages = Ok(vec![noise(2000, 1000), noise(5000, 100), noise(10, 10)]);
    bench.fake.state().recognized.extend([
        Ok("  Page one ".to_string()),
        Ok("\r\n".into()),
        Ok("Page two".into()),
        Ok("Картинка".into()),
    ]);
    let outcome = bench.run(ocr(""), bench.paths(&[&scan, &archive, &picture])).expect("ocr");
    let expected = "Page one\r\n\r\nPage two\r\n\r\nКартинка";
    assert_eq!(outcome.clipboard.as_deref(), Some(expected));
    assert_eq!(outcome.message, fill(tr().ocr_copied, expected.chars().count()));
    assert!(bench.fake.calls().contains(&Call::RenderPdf(scan)));
    let sizes: Vec<(u32, u32)> = bench.fake.state().scanned.iter().map(|image| image.dimensions()).collect();
    assert_eq!(sizes, [(2000, 1000), (4000, 80), (20, 20), (1500, 1500)]);
}

#[test]
fn recognition_sees_transparent_images_on_white() {
    let bench = Bench::new();
    let clear = bench.sandbox.join("clear.png");
    image::RgbaImage::from_pixel(4, 4, image::Rgba([0, 0, 0, 0])).save(&clear).expect("png");
    bench.fake.state().recognized.push_back(Ok("text".into()));
    bench.run(ocr(""), bench.paths(&[&clear])).expect("ocr");
    let state = bench.fake.state();
    assert_eq!(state.scanned[0].get_pixel(0, 0).0, [255, 255, 255, 255]);
}

#[test]
fn recognition_failures_are_reported() {
    let bench = Bench::new();
    let picture = bench.sandbox.file("a.png", png(2, 2));
    let archive = bench.sandbox.file("b.zip", "zip");
    assert_eq!(error(bench.run(ocr(""), bench.paths(&[&archive]))), tr().ocr_needs);
    bench.fake.state().recognized.push_back(Ok(" \r\n ".into()));
    assert_eq!(error(bench.run(ocr(""), bench.paths(&[&picture]))), tr().ocr_nothing);
    bench.fake.state().recognized.push_back(Err(anyhow!(tr().ocr_unavailable)));
    assert_eq!(error(bench.run(ocr(""), bench.paths(&[&picture]))), tr().ocr_unavailable);
    let scan = bench.sandbox.file("scan.pdf", "broken");
    bench.fake.state().pages = Err(tr().pdf_unreadable.into());
    assert_eq!(error(bench.run(ocr(""), bench.paths(&[&scan]))), tr().pdf_unreadable);
}

#[test]
fn qr_takes_text_links_and_shortcuts() {
    let bench = Bench::new();
    let text = |outcome: Outcome| match shown(outcome) {
        Show::Qr(text) => text,
        _ => panic!("expected a QR code"),
    };
    assert_eq!(
        text(bench.run(Kind::Qr, Payload::Text("  https://example.com/a?b=1 \n".into())).expect("link")),
        "https://example.com/a?b=1"
    );
    let shortcut = bench.sandbox.file("Ссылка.url", "[InternetShortcut]\r\nURL=https://ya.ru/\r\nIconIndex=0\r\n");
    assert_eq!(text(bench.run(Kind::Qr, bench.paths(&[&shortcut])).expect("shortcut")), "https://ya.ru/");
    let note = bench.sandbox.file("заметка.TXT", "Привет, мир");
    assert_eq!(text(bench.run(Kind::Qr, bench.paths(&[&note])).expect("note")), "Привет, мир");
}

#[test]
fn qr_refuses_empty_and_oversized_text() {
    let bench = Bench::new();
    let picture = bench.sandbox.file("a.png", png(1, 1));
    assert_eq!(error(bench.run(Kind::Qr, bench.paths(&[&picture]))), tr().qr_needs_text);
    assert_eq!(error(bench.run(Kind::Qr, Payload::Text(" \t ".into()))), tr().qr_needs_text);
    assert_eq!(error(bench.run(Kind::Qr, Payload::Text("x".repeat(3000)))), tr().qr_too_long);
    let big = bench.sandbox.file("big.txt", "y".repeat(9000));
    assert_eq!(error(bench.run(Kind::Qr, bench.paths(&[&big]))), tr().qr_too_long);
    assert!(bench.run(Kind::Qr, Payload::Text("z".repeat(2000))).is_ok());
}

#[test]
fn qr_image_has_a_quiet_zone_and_finder_patterns() {
    let code = qrcodegen::QrCode::encode_text("https://example.com", qrcodegen::QrCodeEcc::Medium).expect("code");
    let image = image::load_from_memory(&qr_png(&code).expect("png")).expect("decode").to_luma8();
    let side = (code.size() as u32 + 8) * 10;
    assert_eq!(image.dimensions(), (side, side));
    assert_eq!(image.get_pixel(5, 5).0, [255]);
    assert_eq!(image.get_pixel(45, 45).0, [0]);
    assert_eq!(image.get_pixel(side - 45, 45).0, [0]);
    assert_eq!(image.get_pixel(side - 5, side - 5).0, [255]);
}

#[test]
fn rename_needs_files_on_disk() {
    let bench = Bench::new();
    assert_eq!(error(bench.run(Kind::Rename, Payload::Text("x".into()))), tr().needs_disk_files);
    let file = bench.sandbox.file("a.txt", "x");
    assert!(
        matches!(shown(bench.run(Kind::Rename, bench.paths(&[&file])).expect("batch")), Show::Rename(batch) if batch.plan(Rule::Date, "").len() == 1)
    );
}

#[test]
fn pages_know_what_to_show_before_the_work_is_done() {
    let sandbox = Sandbox::new();
    let ten = sandbox.file("10.txt", "x");
    let two = sandbox.file("2.txt", "x");
    let photo = sandbox.file("photo.jpg", "x");
    let folder = sandbox.folder("Folder");
    let paths = |paths: &[&PathBuf]| Payload::Paths(paths.iter().map(|path| (*path).clone()).collect());
    let pending = |kind: Kind, payload: Payload| pending(&Action::new(kind), &payload);
    let hash = |key: &str| Kind::Hash { algorithm: Algorithm::Sha256, virustotal: key.into() };
    assert_eq!(pending(Kind::Zip, paths(&[&ten])), None);
    assert_eq!(pending(Kind::Phone, paths(&[&folder, &ten])), Some(Pending::Share(Some(vec![ten.clone()]))));
    assert_eq!(pending(Kind::Phone, paths(&[&folder])), None, "only an error follows, no page");
    assert_eq!(pending(Kind::Phone, Payload::Text("note".into())), Some(Pending::Share(None)));
    assert_eq!(
        pending(hash(" key "), paths(&[&photo])),
        Some(Pending::Hashes { files: Some(vec![photo.clone()]), checking: true })
    );
    assert_eq!(
        pending(hash(""), paths(&[&photo])),
        Some(Pending::Hashes { files: Some(vec![photo.clone()]), checking: false })
    );
    assert_eq!(pending(Kind::Qr, paths(&[&photo])), None, "a picture gives an error, not a code");
    assert_eq!(pending(Kind::Qr, paths(&[&photo, &two])), Some(Pending::Qr));
    assert_eq!(pending(Kind::Qr, Payload::Text("https://example.com".into())), Some(Pending::Qr));
    assert_eq!(
        pending(Kind::Rename, paths(&[&ten, &folder, &two])),
        Some(Pending::Rename(vec![two.clone(), ten.clone(), folder.clone()])),
        "the same order as the finished preview"
    );
    assert_eq!(pending(Kind::Rename, Payload::Text("x".into())), None);
}

#[test]
fn stopping_removes_what_the_action_made_and_keeps_the_sources() {
    let bench = Bench::new();
    bench.sandbox.file("root/tools/ffmpeg.exe", "MZ");
    let first = bench.sandbox.file("clips/first.mov", "video");
    let second = bench.sandbox.file("clips/second.mov", "video");
    bench.respond(Ok(output(0, "", "")));
    bench.respond(Ok(output(0, "", "")));
    bench.fake.state().stop_on_output = true;
    assert_eq!(error(bench.run(Kind::Gif, bench.paths(&[&first, &second]))), tr().stopped);
    assert_eq!(bench.invocations().len(), 1, "the second video is never started");
    assert!(!bench.sandbox.join("clips/first.gif").exists(), "a finished part of a stopped batch is removed");
    assert!(first.exists() && second.exists());
}

#[test]
fn stopped_archive_leaves_no_partial_file() {
    let bench = Bench::new();
    let file = bench.sandbox.file("docs/report.txt", "x");
    let progress = Progress::default();
    progress.cancel();
    let result = run(&Action::new(Kind::Zip), bench.paths(&[&file]), &progress, &bench.storage, &bench.fake);
    assert_eq!(error(result), tr().stopped);
    assert!(!bench.sandbox.join("docs/report.zip").exists());
    assert!(file.exists());
}

struct StopsAfter<'a> {
    sent: usize,
    stop_at: usize,
    size: usize,
    progress: &'a Progress,
}

impl io::Read for StopsAfter<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.sent >= self.stop_at {
            self.progress.cancel();
        }
        let count = buf.len().min(self.size - self.sent);
        self.sent += count;
        Ok(count)
    }
}

#[test]
fn stop_cuts_a_large_file_mid_stream() {
    let progress = Progress::default();
    let mut source = progress.guard(StopsAfter { sent: 0, stop_at: 64 * 1024, size: 16 << 20, progress: &progress });
    let mut sink = Counted(0);
    let copied = io::copy(&mut source, &mut sink);
    assert_eq!(copied.err().map(|error| error.to_string()), Some(tr().stopped.to_string()));
    assert!(sink.0 < 128 * 1024, "the copy ends right after the stop, not at the end of the file");
}

struct Counted(usize);

impl io::Write for Counted {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0 += buf.len();
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn zip_reports_progress_by_bytes() {
    let bench = Bench::new();
    let folder = bench.sandbox.folder("docs");
    bench.sandbox.file("docs/a.txt", vec![1u8; 300_000]);
    bench.sandbox.file("docs/вложенная папка/b.txt", vec![2u8; 100_000]);
    let progress = Progress::default();
    run(&Action::new(Kind::Zip), bench.paths(&[&folder]), &progress, &bench.storage, &bench.fake).expect("zip");
    assert_eq!(progress.get().fraction, Some(1.));
}
