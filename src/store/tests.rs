use super::files::{copy_path, save_note};
use super::storage::related;
use super::*;
use crate::i18n::Language;
use crate::keys::Command;
use crate::testing::{Sandbox, item, language, png};
use crate::theme::ThemeMode;
use gpui::WindowBackgroundAppearance;
use std::collections::HashSet;

fn ids(store: &Store) -> Vec<ItemId> {
    store.items.iter().map(|item| item.id.clone()).collect()
}

fn keys(items: &[&ItemId]) -> HashSet<ItemId> {
    items.iter().map(|id| (*id).clone()).collect()
}

fn clear(store: &mut Store) {
    let all = store.items.iter().map(|item| item.id.clone()).collect();
    store.remove(&all);
}

fn shelf(sandbox: &Sandbox, json: &str) {
    let root = sandbox.join("root");
    fs::create_dir_all(root.join("content")).expect("root");
    fs::write(root.join("shelf.json"), json).expect("shelf");
}

fn entry(id: &str, kind: &str, path: &Path, managed: bool) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "kind": kind,
        "title": id,
        "path": path,
        "text": null,
        "managed": managed,
    })
}

#[test]
fn slot_is_the_first_folder_inside_content() {
    let storage = Storage::new(PathBuf::from(r"C:\data\Traydex"));
    let content = storage.content();
    assert_eq!(content, PathBuf::from(r"C:\data\Traydex\content"));
    assert_eq!(storage.slot(&content.join("files-1").join("a.txt")), Some(content.join("files-1")));
    assert_eq!(storage.slot(&content.join("note-1.txt")), Some(content.join("note-1.txt")));
    assert_eq!(storage.slot(&content), None);
    assert_eq!(storage.slot(Path::new(r"C:\data\Traydex\shelf.json")), None);
    assert_eq!(storage.slot(Path::new(r"D:\elsewhere\a.txt")), None);
    assert_eq!(storage.slot(&content.join("..").join("shelf.json")), None);
    assert!(!storage.owned(Path::new(r"\\server\share\content\a.txt")));
}

#[test]
fn related_paths_ignore_case_and_compare_whole_components() {
    let a = Path::new(r"C:\Data\content\files-1");
    assert!(related(a, Path::new(r"c:\data\CONTENT\files-1\a.txt")));
    assert!(related(Path::new(r"c:\data\content\files-1\a.txt"), a));
    assert!(related(a, a));
    assert!(!related(a, Path::new(r"C:\Data\content\files-10")));
    assert!(!related(Path::new(r"C:\Привет\Файл"), Path::new(r"C:\Привет\Файл2")));
    assert!(related(Path::new(r"C:\ПРИВЕТ\файл"), Path::new(r"c:\привет\ФАЙЛ\x")));
}

#[test]
fn ids_are_unique() {
    let ids: HashSet<String> = (0..5000).map(|_| id()).collect();
    assert_eq!(ids.len(), 5000);
}

#[test]
fn allocate_and_unique_stay_inside_content() {
    let sandbox = Sandbox::new();
    let storage = sandbox.storage();
    let allocated = storage.allocate("report.pdf").expect("allocate");
    assert!(allocated.parent().expect("folder").is_dir());
    assert_eq!(allocated.file_name().and_then(|name| name.to_str()), Some("report.pdf"));
    assert!(storage.owned(&allocated));
    let first = storage.unique("note", "txt");
    let second = storage.unique("note", "txt");
    assert_ne!(first, second);
    assert_eq!(first.parent(), Some(storage.content().as_path()));
    assert!(
        first
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("note-") && name.ends_with(".txt"))
    );
}

#[test]
fn delete_refuses_paths_outside_storage() {
    let sandbox = Sandbox::new();
    let storage = sandbox.storage();
    let external = sandbox.file("external.txt", "keep me");
    let shelf_file = sandbox.file("root/shelf.json", "{}");
    storage.delete(&external);
    storage.delete(&shelf_file);
    storage.delete(&storage.content());
    assert!(external.exists());
    assert!(shelf_file.exists());
}

#[test]
fn delete_removes_files_and_folders_inside_storage() {
    let sandbox = Sandbox::new();
    let storage = sandbox.storage();
    let file = sandbox.file("root/content/note-1.txt", "x");
    let nested = sandbox.file("root/content/files-1/deep/a.txt", "x");
    storage.delete(&file);
    storage.delete(&storage.content().join("files-1"));
    storage.delete(&storage.content().join("missing.txt"));
    assert!(!file.exists());
    assert!(!nested.exists());
    assert!(!storage.content().join("files-1").exists());
}

#[test]
fn release_removes_the_whole_slot() {
    let sandbox = Sandbox::new();
    let storage = sandbox.storage();
    let nested = sandbox.file("root/content/files-1/a.txt", "x");
    let sibling = sandbox.file("root/content/files-2/b.txt", "x");
    storage.release(&nested);
    assert!(!storage.content().join("files-1").exists());
    assert!(sibling.exists());
}

#[test]
fn discard_removes_owned_slot_or_external_temp_file() {
    let sandbox = Sandbox::new();
    let storage = sandbox.storage();
    let owned = sandbox.file("root/content/files-1/a.txt", "x");
    let temp = sandbox.file("temp.png", "x");
    storage.discard(&owned);
    storage.discard(&temp);
    storage.discard(&sandbox.join("never-existed.png"));
    assert!(!storage.content().join("files-1").exists());
    assert!(!temp.exists());
}

#[test]
fn collect_keeps_referenced_content_only() {
    let sandbox = Sandbox::new();
    let storage = sandbox.storage();
    let note = sandbox.file("root/content/note-1.txt", "x");
    let orphan = sandbox.file("root/content/note-2.txt", "x");
    let kept = sandbox.file("root/content/files-1/a.txt", "x");
    let stale = sandbox.file("root/content/files-1/b.txt", "x");
    let whole = sandbox.file("root/content/files-2/folder/inner.txt", "x");
    let folder = storage.content().join("files-2").join("folder");
    let empty = sandbox.folder("root/content/files-3");
    let external = sandbox.file("outside.txt", "x");
    let referenced = [note.as_path(), kept.as_path(), folder.as_path(), external.as_path()];
    storage.collect(&referenced);
    assert!(note.exists() && kept.exists() && whole.exists() && external.exists());
    assert!(!orphan.exists() && !stale.exists() && !empty.exists());
}

#[test]
fn collect_matches_references_case_insensitively() {
    let sandbox = Sandbox::new();
    let storage = sandbox.storage();
    let note = sandbox.file("root/content/Note-1.txt", "x");
    let reference = PathBuf::from(note.to_string_lossy().to_uppercase());
    storage.collect(&[reference.as_path()]);
    assert!(note.exists());
}

#[test]
fn collect_without_content_folder_is_harmless() {
    let sandbox = Sandbox::new();
    sandbox.storage().collect(&[]);
    assert!(!sandbox.storage().content().exists());
}

#[test]
fn scratch_releases_unclaimed_files_on_drop() {
    let sandbox = Sandbox::new();
    let storage = sandbox.storage();
    let first = save_virtual(&storage, "a.txt", b"a").expect("save");
    let second = save_virtual(&storage, "b.txt", b"b").expect("save");
    let external = sandbox.file("external.txt", "x");
    let mut scratch = Scratch::new(storage.clone());
    assert!(scratch.is_empty());
    scratch.push(first.clone());
    scratch.push(external.clone());
    assert_eq!(scratch.paths(), [first.clone(), external.clone()]);
    drop(scratch);
    assert!(!first.parent().expect("slot").exists());
    assert!(external.exists());
    let mut kept = Scratch::new(storage);
    kept.push(second.clone());
    assert_eq!(kept.keep(), vec![second.clone()]);
    assert!(second.exists());
}

#[test]
fn fresh_store_has_default_settings() {
    let sandbox = Sandbox::new();
    let store = sandbox.store();
    assert!(store.items.is_empty());
    assert!(
        store.settings.automatic
            && store.settings.autostart
            && store.settings.context_menu
            && store.settings.double_click_preview
            && store.settings.separate_notes
            && store.settings.tray_icon
    );
    assert!(!store.settings.blur && !store.settings.pinned && store.settings.shake == ShakeLevel::Off);
    assert!(!store.settings.window_animations && !store.settings.interface_animations);
    assert_eq!(store.settings.transparency, 0);
    assert!(store.settings.theme == ThemeMode::System);
    assert!(store.settings.layout == Layout::List);
    assert!(
        store.settings.language.is_none()
            && store.settings.corner.is_none()
            && store.settings.position.is_none()
            && store.settings.size.is_none()
    );
    assert_eq!(store.storage, sandbox.storage());
    assert!(sandbox.storage().content().is_dir());
    assert!(!sandbox.join("root").join("shelf.json").exists());
}

#[test]
fn saved_store_round_trips() {
    let sandbox = Sandbox::new();
    let mut store = sandbox.store();
    let file = sandbox.file("docs/a.txt", "x");
    store.add_paths(vec![file.clone()]);
    store.add_text("hello".into()).expect("text");
    store.settings.automatic = false;
    store.settings.autostart = false;
    store.settings.context_menu = false;
    store.settings.double_click_preview = false;
    store.settings.separate_notes = false;
    store.settings.tray_icon = false;
    store.settings.theme = ThemeMode::Dark;
    store.settings.layout = Layout::Grid;
    store.settings.transparency = 30;
    store.settings.blur = true;
    store.settings.window_animations = true;
    store.settings.interface_animations = true;
    store.settings.language = Some(Language::Russian);
    store.settings.pinned = true;
    store.settings.shake = ShakeLevel::High;
    store.settings.corner = Some(Corner::BottomLeft);
    store.settings.position = Some((-1200, 40));
    store.settings.size = Some((700., 650.));
    let custom = crate::keys::Shortcut { alt: true, key: 0x54, ..Default::default() };
    store.settings.shortcuts.set(Command::Toggle, custom);
    store.save().expect("save");
    assert!(!sandbox.join("root").join("shelf.tmp").exists());
    let json = fs::read_to_string(sandbox.join("root").join("shelf.json")).expect("json");
    assert!(!json.contains("storage") && !json.contains("doomed"));
    let restored = sandbox.store();
    assert_eq!(ids(&restored), ids(&store));
    assert_eq!(restored.items[1].path, file);
    assert_eq!(restored.items[0].text.as_deref(), Some("hello"));
    assert!(
        !restored.settings.automatic
            && !restored.settings.autostart
            && !restored.settings.context_menu
            && !restored.settings.double_click_preview
            && !restored.settings.separate_notes
            && !restored.settings.tray_icon
    );
    assert!(restored.settings.theme == ThemeMode::Dark && restored.settings.layout == Layout::Grid);
    assert_eq!(restored.settings.transparency, 30);
    assert!(restored.settings.blur && restored.settings.pinned && restored.settings.shake == ShakeLevel::High);
    assert!(restored.settings.window_animations && restored.settings.interface_animations);
    assert!(restored.settings.language == Some(Language::Russian));
    assert_eq!(restored.settings.corner, Some(Corner::BottomLeft));
    assert_eq!(restored.settings.position, Some((-1200, 40)));
    assert_eq!(restored.settings.size, Some((700., 650.)));
    assert_eq!(restored.settings.shortcuts.get(Command::Toggle), custom);
}

#[test]
fn old_format_without_settings_uses_defaults() {
    let sandbox = Sandbox::new();
    shelf(&sandbox, r#"{"items": [], "obsolete_option": 5, "tile_icons": true}"#);
    let store = sandbox.store();
    assert!(
        store.settings.automatic
            && store.settings.autostart
            && store.settings.context_menu
            && store.settings.double_click_preview
            && store.settings.separate_notes
            && store.settings.tray_icon
    );
    assert!(!store.settings.blur && !store.settings.window_animations && !store.settings.interface_animations);
    assert_eq!(store.settings.tile_input, TileInput::Clipboard);
    assert_eq!(store.settings.font_size, 13);
    assert!(!sandbox.join("root").join("shelf.broken.json").exists());
}

#[test]
fn shake_switch_from_older_versions_becomes_a_level() {
    for (value, expected) in [
        ("true", ShakeLevel::Low),
        ("false", ShakeLevel::Off),
        (r#""High""#, ShakeLevel::High),
        (r#""Max""#, ShakeLevel::Off),
    ] {
        let sandbox = Sandbox::new();
        shelf(&sandbox, &format!(r#"{{"items": [], "shake": {value}}}"#));
        assert_eq!(sandbox.store().settings.shake, expected, "{value}");
        assert!(!sandbox.join("root").join("shelf.broken.json").exists(), "{value}");
    }
}

#[test]
fn font_size_outside_the_choices_falls_back_to_the_default() {
    for (value, expected) in [("15", 15), ("40", 13), ("-1", 13), ("\"big\"", 13), ("null", 13)] {
        let sandbox = Sandbox::new();
        shelf(&sandbox, &format!(r#"{{"items": [], "font_size": {value}}}"#));
        assert_eq!(sandbox.store().settings.font_size, expected, "{value}");
        assert!(!sandbox.join("root").join("shelf.broken.json").exists(), "{value}");
    }
}

#[test]
fn blur_accepts_old_string_levels_and_booleans() {
    let cases = [
        ("true", true),
        ("false", false),
        ("\"Off\"", false),
        ("\"Light\"", true),
        ("\"Strong\"", true),
        ("42", false),
        ("null", false),
    ];
    for (value, expected) in cases {
        let sandbox = Sandbox::new();
        shelf(&sandbox, &format!(r#"{{"items": [], "blur": {value}}}"#));
        assert_eq!(sandbox.store().settings.blur, expected, "{value}");
    }
}

#[test]
fn unknown_enum_values_reject_the_file() {
    let sandbox = Sandbox::new();
    shelf(&sandbox, r#"{"items": [], "theme": "Neon"}"#);
    let store = sandbox.store();
    assert!(store.settings.theme == ThemeMode::System);
    assert!(sandbox.join("root").join("shelf.broken.json").exists());
}

#[test]
fn corrupt_store_is_backed_up_and_its_content_kept() {
    let sandbox = Sandbox::new();
    let broken = r#"{"items": [{"id": "1", "kind": "Text", "title": "x", "path": "#;
    shelf(&sandbox, broken);
    let note = sandbox.file("root/content/note-1.txt", "precious");
    let folder = sandbox.file("root/content/files-1/photo.png", "precious");
    let store = sandbox.store();
    assert!(store.items.is_empty());
    assert_eq!(fs::read_to_string(sandbox.join("root").join("shelf.broken.json")).expect("backup"), broken);
    assert!(note.exists());
    assert!(folder.exists());
}

#[test]
fn unreadable_store_keeps_content() {
    let sandbox = Sandbox::new();
    sandbox.folder("root/shelf.json");
    let note = sandbox.file("root/content/note-1.txt", "precious");
    let store = sandbox.store();
    assert!(store.items.is_empty());
    assert!(note.exists());
}

#[test]
fn empty_object_is_treated_as_corrupt() {
    let sandbox = Sandbox::new();
    shelf(&sandbox, "{}");
    assert!(sandbox.store().items.is_empty());
    assert!(sandbox.join("root").join("shelf.broken.json").exists());
}

#[test]
fn opening_removes_unreferenced_content_and_recomputes_ownership() {
    let sandbox = Sandbox::new();
    let storage = sandbox.storage();
    let note = sandbox.file("root/content/note-1.txt", "x");
    let orphan = sandbox.file("root/content/note-2.txt", "x");
    let external = sandbox.file("docs/a.txt", "x");
    let items = serde_json::json!({
        "items": [entry("1", "Text", &note, false), entry("2", "File", &external, true)]
    });
    shelf(&sandbox, &items.to_string());
    let store = sandbox.store();
    assert!(store.items[0].managed);
    assert!(!store.items[1].managed);
    assert!(note.exists() && external.exists());
    assert!(!orphan.exists());
    assert_eq!(storage, store.storage);
}

#[test]
fn legacy_folder_is_migrated_with_paths_rewritten() {
    let sandbox = Sandbox::new();
    let legacy = sandbox.join("legacy");
    let note = sandbox.file("legacy/content/note-1.txt", "old note");
    let external = sandbox.file("docs/a.txt", "x");
    let items = serde_json::json!({
        "items": [entry("1", "Text", &note, true), entry("2", "File", &external, false)]
    });
    sandbox.file("legacy/shelf.json", items.to_string());
    let store = sandbox.store();
    let moved = sandbox.join("root").join("content").join("note-1.txt");
    assert!(!legacy.exists());
    assert_eq!(store.items[0].path, moved);
    assert!(store.items[0].managed);
    assert_eq!(fs::read_to_string(&moved).expect("note"), "old note");
    assert_eq!(store.items[1].path, external);
    let saved = fs::read_to_string(sandbox.join("root").join("shelf.json")).expect("saved");
    assert!(!saved.contains("legacy"));
}

#[test]
fn notes_are_kept_in_their_own_folder_apart_from_pasted_text() {
    let sandbox = Sandbox::new();
    let mut store = sandbox.store();
    let key = store.add_note();
    store.edit_text(&key, "Заметка о встрече");
    store.add_text("вставленный текст".into()).expect("text");
    store.save().expect("save");
    let notes = sandbox.join("root").join("content").join("notes");
    let note = store.item(&key).expect("note");
    assert_eq!(note.path.parent(), Some(notes.as_path()));
    assert_eq!(fs::read_to_string(&note.path).expect("note file"), "Заметка о встрече");
    let text = store.items.iter().find(|item| !item.note()).expect("text");
    assert_eq!(text.path.parent(), Some(sandbox.join("root").join("content").as_path()));
}

#[test]
fn notes_from_older_versions_move_to_the_notes_folder() {
    let sandbox = Sandbox::new();
    let note = sandbox.file("root/content/note-1.txt", "старая заметка");
    let text = sandbox.file("root/content/note-2.txt", "вставленный текст");
    let mut old = entry("1", "Note", &note, true);
    old["text"] = "старая заметка".into();
    let mut pasted = entry("2", "Text", &text, true);
    pasted["text"] = "вставленный текст".into();
    shelf(&sandbox, &serde_json::json!({ "items": [old, pasted] }).to_string());
    let store = sandbox.store();
    let moved = sandbox.join("root").join("content").join("notes").join("note-1.txt");
    assert_eq!(store.items[0].path, moved);
    assert_eq!(fs::read_to_string(&moved).expect("moved note"), "старая заметка");
    assert!(!note.exists());
    assert_eq!(store.items[1].path, text);
    assert!(text.exists());
    assert_eq!(sandbox.store().items[0].path, moved, "the new place is saved");
}

#[test]
fn removing_one_note_keeps_the_other_notes() {
    let sandbox = Sandbox::new();
    let mut store = sandbox.store();
    let first = store.add_note();
    store.edit_text(&first, "первая");
    let second = store.add_note();
    store.edit_text(&second, "вторая");
    store.save().expect("save");
    let kept = store.item(&first).expect("first").path.clone();
    store.remove(&keys(&[&second]));
    store.save().expect("save");
    assert_eq!(fs::read_to_string(&kept).expect("the other note survives"), "первая");
    let reopened = sandbox.store();
    assert_eq!(fs::read_to_string(&reopened.item(&first).expect("first").path).expect("after sweep"), "первая");
}

#[test]
fn migration_is_skipped_when_new_folder_exists_or_legacy_is_empty() {
    let sandbox = Sandbox::new();
    sandbox.file("legacy/shelf.json", r#"{"items": []}"#);
    sandbox.folder("root");
    assert_eq!(migrate(&sandbox.join("root"), &sandbox.join("legacy")), None);
    assert!(sandbox.join("legacy").join("shelf.json").exists());
    let other = Sandbox::new();
    other.folder("legacy/content");
    assert_eq!(migrate(&other.join("root"), &other.join("legacy")), None);
    assert!(!other.join("root").exists());
}

#[test]
fn add_paths_classifies_and_skips_missing_or_duplicate() {
    let sandbox = Sandbox::new();
    let mut store = sandbox.store();
    let folder = sandbox.folder("docs");
    let photo = sandbox.file("PHOTO.JPG", "x");
    let file = sandbox.file("notes.md", "x");
    let missing = sandbox.join("missing.txt");
    assert_eq!(store.add_paths(vec![folder.clone(), photo.clone(), file.clone(), missing, photo.clone()]), 3);
    let kinds: Vec<Kind> = store.items.iter().map(|item| item.kind).collect();
    assert!(kinds == [Kind::Folder, Kind::Image, Kind::File]);
    assert_eq!(store.items[1].title, "PHOTO.JPG");
    assert!(store.items.iter().all(|item| !item.managed));
    let later = sandbox.file("later.txt", "x");
    assert_eq!(store.add_paths(vec![file, later.clone()]), 1);
    assert_eq!(store.items[0].path, later);
}

#[test]
fn add_paths_marks_storage_files_as_managed() {
    let sandbox = Sandbox::new();
    let mut store = sandbox.store();
    let stored = save_virtual(&store.storage, "a.txt", b"x").expect("save");
    store.add_paths(vec![stored]);
    assert!(store.items[0].managed);
}

#[test]
fn add_text_detects_links_and_builds_titles() {
    let sandbox = Sandbox::new();
    let mut store = sandbox.store();
    assert_eq!(store.add_text("   \n\t".into()).expect("blank"), 0);
    assert_eq!(store.add_text("  https://example.com/a?b=1  ".into()).expect("link"), 1);
    assert!(store.items[0].kind == Kind::Link);
    assert_eq!(store.items[0].title, "https://example.com/a?b=1");
    for text in ["https://example.com and more", "ftp://example.com", "see http://x.y"] {
        store.add_text(text.into()).expect("text");
        assert!(store.items[0].kind == Kind::Text, "{text}");
    }
    store.add_text("first   line\n\nsecond\tline".into()).expect("text");
    assert_eq!(store.items[0].title, "first line second line");
    let long = "я".repeat(200);
    store.add_text(long.clone()).expect("text");
    assert!(store.items[0].title.chars().count() <= 96);
    assert!(store.items[0].title.ends_with("я…"), "{}", store.items[0].title);
    assert_eq!(store.items[0].text.as_deref(), Some(long.as_str()));
    assert!(store.items[0].managed);
    assert_eq!(fs::read_to_string(&store.items[0].path).expect("note"), long);
    assert!(store.storage.owned(&store.items[0].path));
}

#[test]
fn edited_note_rewrites_its_file_on_save_and_follows_with_an_automatic_title() {
    let sandbox = Sandbox::new();
    let mut store = sandbox.store();
    let key = store.add_note();
    assert!(store.items[0].editable() && store.items[0].title.is_empty());
    store.edit_text(
        &key,
        "Купить хлеб
и молоко",
    );
    assert_eq!(store.items[0].title, "Купить хлеб и молоко");
    store.save().expect("save");
    assert_eq!(
        fs::read_to_string(&store.items[0].path).expect("note"),
        "Купить хлеб
и молоко"
    );
}

#[test]
fn edited_text_turns_into_a_link_but_a_note_stays_a_note() {
    let sandbox = Sandbox::new();
    let mut store = sandbox.store();
    store.add_text("текст".into()).expect("text");
    let text = store.items[0].id.clone();
    let note = store.add_note();
    store.edit_text(&text, " https://example.com/a ");
    store.edit_text(&note, " https://example.com/a ");
    let kind = |key: &ItemId| store.items.iter().find(|item| item.id == *key).expect("item").kind;
    assert!(kind(&text) == Kind::Link);
    assert!(kind(&note) == Kind::Note);
    store.edit_text(&text, "https://example.com/a и текст");
    assert!(store.items.iter().any(|item| item.id == text && item.kind == Kind::Text));
}

#[test]
fn renamed_note_keeps_its_title_until_cleared() {
    let sandbox = Sandbox::new();
    let mut store = sandbox.store();
    store.add_text("старый текст".into()).expect("text");
    let key = store.items[0].id.clone();
    assert!(store.rename(&key, "  Мой   список  "));
    assert_eq!(store.items[0].title, "Мой список");
    store.edit_text(&key, "новый текст");
    assert_eq!(store.items[0].title, "Мой список");
    assert!(store.rename(&key, "   "));
    assert_eq!(store.items[0].title, "новый текст");
    assert!(store.items[0].auto_title());
    assert!(store.rename(&key, "новый текст"));
    assert!(!store.items[0].auto_title());
    assert!(!store.rename(&key, "новый текст"));
}

#[test]
fn title_typed_as_the_start_of_the_text_stays_custom() {
    let sandbox = Sandbox::new();
    let mut store = sandbox.store();
    let sentence = "Длинное предложение из текстового файла, которое вставили в название целиком. ".repeat(2);
    store.add_text(sentence.clone()).expect("text");
    let key = store.items[0].id.clone();
    assert!(store.items[0].auto_title());
    assert!(store.rename(&key, &sentence));
    assert!(!store.items[0].auto_title());
    let title = store.items[0].title.clone();
    store.save().expect("save");
    let mut reopened = sandbox.store();
    assert!(!reopened.items[0].auto_title());
    reopened.edit_text(&key, "совсем другой текст");
    assert_eq!(reopened.items[0].title, title);
}

#[test]
fn long_custom_title_is_cut_with_an_ellipsis() {
    let sandbox = Sandbox::new();
    let mut store = sandbox.store();
    store.add_text("текст".into()).expect("text");
    let key = store.items[0].id.clone();
    assert!(store.rename(&key, &"очень длинное название ".repeat(10)));
    let title = &store.items[0].title;
    assert!(title.chars().count() <= 96 && title.ends_with("е…"), "{title}");
}

#[test]
fn cut_title_ends_on_a_letter_before_the_ellipsis() {
    let sandbox = Sandbox::new();
    let mut store = sandbox.store();
    for tail in [", дальше", ". Дальше", " (дальше", " — дальше", "«дальше"] {
        let text = format!("{}{tail}{}", "я".repeat(94), " ещё".repeat(5));
        store.add_text(text).expect("text");
        assert_eq!(store.items[0].title, format!("{}…", "я".repeat(94)), "{tail}");
    }
}

#[test]
fn titles_from_older_versions_keep_their_kind() {
    let sandbox = Sandbox::new();
    let long = "я".repeat(200);
    let old = |id: &str, title: &str, text: &str| {
        serde_json::json!({
            "id": id,
            "kind": "Text",
            "title": title,
            "path": sandbox.join(format!("{id}.txt")),
            "text": text,
            "managed": false,
        })
    };
    let items = vec![
        old("cut", &"я".repeat(96), &long),
        old("short", "короткий текст", "короткий   текст"),
        old("custom", "Свой заголовок", "текст заметки"),
        old("prefix", "короткий", "короткий текст"),
    ];
    let mut items = items;
    items.push(serde_json::json!({
        "id": "stale",
        "kind": "Note",
        "title": "текст,…",
        "path": sandbox.join("stale.txt"),
        "text": "текст, который изменился",
        "managed": false,
        "named": false,
    }));
    shelf(&sandbox, &serde_json::json!({ "items": items }).to_string());
    let store = sandbox.store();
    let item = |id: &str| store.items.iter().find(|item| item.id.to_string() == id).expect("item");
    assert!(item("cut").auto_title() && item("cut").title.ends_with('…'));
    assert!(item("short").auto_title() && item("short").title == "короткий текст");
    assert!(!item("custom").auto_title() && item("custom").title == "Свой заголовок");
    assert!(!item("prefix").auto_title() && item("prefix").title == "короткий");
    assert!(item("stale").auto_title() && item("stale").title == "текст, который изменился");
}

#[test]
fn pinned_items_stay_on_top_and_survive_a_restart() {
    let sandbox = Sandbox::new();
    let mut store = sandbox.store();
    for text in ["первый", "второй", "третий"] {
        store.add_text(text.into()).expect("text");
    }
    let key = |store: &Store, title: &str| store.items.iter().find(|item| item.title == title).expect(title).id.clone();
    let titles = |store: &Store| store.items.iter().map(|item| item.title.clone()).collect::<Vec<_>>();
    assert!(store.set_pinned(&HashSet::from([key(&store, "первый")]), true));
    assert!(store.set_pinned(&HashSet::from([key(&store, "второй")]), true));
    assert!(!store.set_pinned(&HashSet::from([key(&store, "второй")]), true));
    assert_eq!(titles(&store), ["второй", "первый", "третий"]);
    store.add_text("новый".into()).expect("text");
    store.add_note();
    assert_eq!(&titles(&store)[..3], ["второй", "первый", ""]);
    assert!(store.set_pinned(&HashSet::from([key(&store, "второй")]), false));
    assert_eq!(&titles(&store)[..2], ["первый", "второй"]);
    store.save().expect("save");
    let reopened = sandbox.store();
    assert_eq!(titles(&reopened), titles(&store));
    assert!(reopened.items[0].pinned && !reopened.items[1].pinned);
}

#[test]
fn pinned_items_from_a_file_are_moved_to_the_top() {
    let sandbox = Sandbox::new();
    let mut items =
        vec![entry("a", "File", &sandbox.join("a.txt"), false), entry("b", "File", &sandbox.join("b.txt"), false)];
    items[1]["pinned"] = serde_json::json!(true);
    shelf(&sandbox, &serde_json::json!({ "items": items }).to_string());
    let store = sandbox.store();
    assert_eq!(store.items.iter().map(|item| item.id.to_string()).collect::<Vec<_>>(), ["b", "a"]);
}

#[test]
fn files_and_images_are_not_edited_as_notes() {
    let sandbox = Sandbox::new();
    let mut store = sandbox.store();
    let file = sandbox.file("plan.txt", "original");
    store.add_paths(vec![file.clone()]);
    let key = store.items[0].id.clone();
    assert!(!store.items[0].editable());
    store.edit_text(&key, "changed");
    assert!(!store.rename(&key, "Другое имя"));
    assert_eq!(fs::read_to_string(&file).expect("file"), "original");
    assert_eq!(store.items[0].title, "plan.txt");
}

#[test]
fn add_text_skips_exact_duplicates_only() {
    let sandbox = Sandbox::new();
    let mut store = sandbox.store();
    assert_eq!(store.add_text("note".into()).expect("text"), 1);
    assert_eq!(store.add_text("note".into()).expect("text"), 0);
    assert_eq!(store.add_text(" note".into()).expect("text"), 1);
    assert_eq!(store.items.len(), 2);
}

#[test]
fn add_text_fails_cleanly_without_storage_folder() {
    let sandbox = Sandbox::new();
    let storage = Storage::new(sandbox.join("nowhere"));
    assert!(save_note(&storage, "x").is_err());
    assert!(!storage.content().exists());
}

#[test]
fn adopt_keeps_added_files_and_releases_the_rest() {
    let sandbox = Sandbox::new();
    let mut store = sandbox.store();
    let file = save_virtual(&store.storage, "a.txt", b"x").expect("save");
    let unwritten = store.storage.allocate("b.txt").expect("allocate");
    let mut scratch = Scratch::new(store.storage.clone());
    scratch.push(file.clone());
    scratch.push(unwritten.clone());
    assert_eq!(store.adopt(scratch), 1);
    assert!(file.exists());
    assert!(!unwritten.parent().expect("slot").exists());
    let mut again = Scratch::new(store.storage.clone());
    again.push(file.clone());
    assert_eq!(store.adopt(again), 0);
    assert!(file.exists());
}

#[test]
fn removing_items_deletes_managed_content_only_after_save() {
    let sandbox = Sandbox::new();
    let mut store = sandbox.store();
    store.settings.recycle = false;
    let external = sandbox.file("docs/a.txt", "x");
    store.add_paths(vec![external.clone()]);
    store.add_text("note".into()).expect("text");
    let note = store.items[0].path.clone();
    let (note_id, external_id) = (store.items[0].id.clone(), store.items[1].id.clone());
    store.remove(&keys(&[&note_id, &external_id]));
    assert!(store.items.is_empty());
    assert!(note.exists());
    store.save().expect("save");
    assert!(!note.exists());
    assert!(external.exists());
}

#[test]
fn removing_one_file_of_a_shared_folder_keeps_the_other() {
    let sandbox = Sandbox::new();
    let mut store = sandbox.store();
    store.settings.recycle = false;
    let first = sandbox.file("root/content/files-1/a.txt", "a");
    let second = sandbox.file("root/content/files-1/b.txt", "b");
    store.add_paths(vec![first.clone(), second.clone()]);
    let first_id = store.items.iter().find(|item| item.path == first).expect("first").id.clone();
    store.remove(&keys(&[&first_id]));
    store.save().expect("save");
    assert!(!first.exists());
    assert!(second.exists());
    clear(&mut store);
    store.save().expect("save");
    assert!(!sandbox.join("root/content/files-1").exists());
}

#[test]
fn removing_a_file_inside_a_kept_folder_item_deletes_nothing() {
    let sandbox = Sandbox::new();
    let mut store = sandbox.store();
    let folder = sandbox.folder("root/content/files-1/project");
    let inner = sandbox.file("root/content/files-1/project/readme.txt", "x");
    store.add_paths(vec![folder.clone(), inner.clone()]);
    let inner_id = store.items.iter().find(|item| item.path == inner).expect("inner").id.clone();
    store.remove(&keys(&[&inner_id]));
    store.save().expect("save");
    assert!(inner.exists());
}

#[test]
fn clear_keeps_external_originals() {
    let sandbox = Sandbox::new();
    let mut store = sandbox.store();
    store.settings.recycle = false;
    let external = sandbox.file("docs/a.txt", "x");
    let folder = sandbox.folder("docs/folder");
    store.add_paths(vec![external.clone(), folder.clone()]);
    store.add_text("note".into()).expect("text");
    let mut forged = item(Kind::File, "forged", external.clone(), None);
    forged.managed = true;
    store.items.push(forged);
    clear(&mut store);
    store.save().expect("save");
    assert!(external.exists() && folder.exists());
    assert_eq!(fs::read_dir(store.storage.content()).expect("content").count(), 0);
}

#[test]
fn failed_save_defers_deletion_until_next_success() {
    let sandbox = Sandbox::new();
    let mut store = sandbox.store();
    store.settings.recycle = false;
    store.add_text("note".into()).expect("text");
    let note = store.items[0].path.clone();
    clear(&mut store);
    let blocker = sandbox.folder("root/shelf.tmp");
    assert!(store.save().is_err());
    assert!(note.exists());
    fs::remove_dir(blocker).expect("unblock");
    store.save().expect("save");
    assert!(!note.exists());
}

#[test]
fn reset_settings_keeps_items_and_storage() {
    let sandbox = Sandbox::new();
    let mut store = sandbox.store();
    store.add_text("note".into()).expect("text");
    store.settings.automatic = false;
    store.settings.transparency = 20;
    store.settings.blur = true;
    store.settings.pinned = true;
    store.settings.corner = Some(Corner::TopLeft);
    store.settings.position = Some((1, 2));
    store.settings.size = Some((800., 600.));
    store.settings.language = Some(Language::French);
    store.settings.layout = Layout::Grid;
    store
        .settings
        .shortcuts
        .set(Command::Toggle, crate::keys::Shortcut::plain(windows::Win32::UI::Input::KeyboardAndMouse::VK_F2));
    store.reset_settings();
    assert_eq!(store.items.len(), 1);
    assert_eq!(store.storage, sandbox.storage());
    assert!(store.settings.automatic && !store.settings.blur && !store.settings.pinned);
    assert_eq!(store.settings.transparency, 0);
    assert!(
        store.settings.corner.is_none()
            && store.settings.position.is_none()
            && store.settings.size.is_none()
            && store.settings.language.is_none()
    );
    assert!(store.settings.layout == Layout::List);
    assert!(!store.settings.shortcuts.customized(Command::Toggle));
}

#[test]
fn opacity_is_limited_by_maximum_transparency() {
    let sandbox = Sandbox::new();
    let mut store = sandbox.store();
    for (transparency, opacity) in [(0, 1.), (25, 0.75), (40, 0.6), (200, 0.6)] {
        store.settings.transparency = transparency;
        assert!((store.settings.opacity() - opacity).abs() < 1e-6, "{transparency}");
    }
}

#[test]
fn blur_applies_only_to_translucent_window() {
    let sandbox = Sandbox::new();
    let mut store = sandbox.store();
    let opaque = |store: &Store| matches!(store.settings.backdrop(), WindowBackgroundAppearance::Opaque);
    store.settings.blur = true;
    assert!(opaque(&store));
    store.settings.transparency = 10;
    assert!(matches!(store.settings.backdrop(), WindowBackgroundAppearance::LightBlurred));
    store.settings.window_animations = true;
    assert!(opaque(&store), "window animation and blur don't work together");
    store.settings.window_animations = false;
    store.settings.blur = false;
    assert!(opaque(&store));
}

#[test]
fn window_size_is_clamped() {
    let sandbox = Sandbox::new();
    let mut store = sandbox.store();
    assert_eq!(store.settings.window_size(), DEFAULT_SIZE);
    store.settings.size = Some((10., 10.));
    assert_eq!(store.settings.window_size(), MIN_SIZE);
    store.settings.size = Some((5000., 900.));
    assert_eq!(store.settings.window_size(), (2000., 900.));
}

#[test]
fn describe_reports_existence_and_details() {
    let sandbox = Sandbox::new();
    let file = sandbox.file("report.pdf", vec![0u8; 2048]);
    let bare = sandbox.file("README", "abc");
    let folder = sandbox.folder("docs");
    let text = item(Kind::Text, "t", PathBuf::new(), Some("привет"));
    assert!(text.describe().exists);
    assert_eq!(text.describe().detail, fill(tr().text_detail, 6));
    let link = item(Kind::Link, "l", PathBuf::new(), Some(" https://www.example.com/page "));
    assert_eq!(link.describe().detail, fill(tr().link_detail, "example.com"));
    let pdf = item(Kind::File, "f", file.clone(), None).describe();
    assert!(pdf.exists);
    assert_eq!(pdf.detail, format!("PDF · {}", human_size(2048)));
    assert_eq!(pdf.location, Some(Location::Folder(sandbox.path().to_path_buf())));
    let mut kept = item(Kind::File, "f", file, None);
    kept.managed = true;
    assert_eq!(kept.describe().location, Some(Location::Storage));
    assert_eq!(text.describe().location, None, "text has no folder of its own");
    assert!(item(Kind::File, "f", bare, None).describe().detail.starts_with(tr().file));
    let docs = item(Kind::Folder, "d", folder, None).describe();
    assert!(docs.exists);
    assert_eq!(docs.location, Some(Location::Folder(sandbox.path().to_path_buf())));
    assert!(!item(Kind::Folder, "d", sandbox.join("gone"), None).describe().exists);
    let missing = item(Kind::Image, "i", sandbox.join("gone.png"), None).describe();
    assert!(!missing.exists);
    assert!(missing.detail.is_empty());
}

#[test]
<<<<<<< HEAD
fn recycled_cards_come_back_when_their_files_return() {
    let sandbox = Sandbox::new();
    let mut store = sandbox.store();
    let first = sandbox.file("root/content/files-1/снимок экрана.png", "a");
    let second = sandbox.file("root/content/files-2/отчёт.pdf", "b");
    store.add_paths(vec![first.clone(), second.clone()]);
    let pinned: HashSet<ItemId> =
        store.items.iter().filter(|item| item.path == second).map(|item| item.id.clone()).collect();
    store.set_pinned(&pinned, true);
    let everything: HashSet<ItemId> = store.items.iter().map(|item| item.id.clone()).collect();
    store.remove(&everything);
    let snapshot = store.snapshot().expect("snapshot");
    snapshot.write().expect("write");
    assert!(first.exists() && second.exists(), "files wait for the Recycle Bin, not for deletion");
    let mut remembered = store.recycled();
    remembered.sort();
    assert_eq!(remembered, [first.clone(), second.clone()]);
    assert_eq!(store.bring_back(std::slice::from_ref(&second)), 1);
    assert_eq!(
        store.items.iter().map(|item| (item.title.as_str(), item.pinned)).collect::<Vec<_>>(),
        [("отчёт.pdf", true)]
    );
    assert!(store.items[0].managed);
    assert_eq!(store.recycled(), std::slice::from_ref(&first), "only the returned card is forgotten");
    store.save().expect("save");
    let reopened = Store::open(sandbox.storage(), &sandbox.join("legacy"));
    let titles: Vec<&str> = reopened.items.iter().map(|item| item.title.as_str()).collect();
    assert_eq!(titles, ["отчёт.pdf", "снимок экрана.png"]);
    assert!(first.exists(), "a file restored while Traydex was closed is not swept as an orphan");
    assert!(reopened.recycled().is_empty());
}

#[test]
fn only_the_latest_recycled_cards_are_remembered() {
    let sandbox = Sandbox::new();
    let mut store = sandbox.store();
    for index in 0..105 {
        store.add_text(format!("note {index}")).expect("text");
        let key = store.items[0].id.clone();
        store.remove(&HashSet::from([key]));
    }
    let remembered = store.recycled();
    assert_eq!(remembered.len(), 100);
    assert_eq!(store.recycled.last().and_then(|item| item.text.as_deref()), Some("note 104"));
    assert_eq!(store.recycled.first().and_then(|item| item.text.as_deref()), Some("note 5"));
}

#[test]
=======
>>>>>>> origin/main
fn long_paths_keep_the_drive_and_the_innermost_folders() {
    let path = Path::new(r"C:\Users\Игорь\AppData\Local\Temp\мои файлы");
    let within = |limit: usize| move |text: &str| text.chars().count() <= limit;
    assert_eq!(shorten_path(path, within(100)), r"C:\Users\Игорь\AppData\Local\Temp\мои файлы");
    assert_eq!(shorten_path(path, within(33)), r"C:\…\AppData\Local\Temp\мои файлы");
    assert_eq!(shorten_path(path, within(19)), r"C:\…\Temp\мои файлы");
    assert_eq!(shorten_path(path, within(3)), r"C:\…\мои файлы", "the folder itself stays even when nothing fits");
    let share = Path::new(r"\\server\share\отдел\отчёты 2026");
    assert_eq!(shorten_path(share, within(30)), r"\\server\share\…\отчёты 2026");
    assert_eq!(shorten_path(Path::new(r"D:\Фото"), within(1)), r"D:\Фото");
}

#[test]
fn icons_follow_kind_and_archive_extension() {
    let icon = |kind: Kind, path: &str| item(kind, "x", PathBuf::from(path), None).icon();
    assert_eq!(icon(Kind::File, "a.ZIP"), "file-archive");
    assert_eq!(icon(Kind::File, "a.tar.gz"), "file-archive");
    assert_eq!(icon(Kind::File, "a.txt"), "file");
    assert_eq!(icon(Kind::Folder, "a.zip"), "folder");
    assert_eq!(icon(Kind::Image, "a.png"), "image");
    assert_eq!(icon(Kind::Text, ""), "text");
    assert_eq!(icon(Kind::Link, ""), "link");
}

#[test]
fn host_extracts_domain() {
    assert_eq!(host("https://www.example.com/path?x=1"), "example.com");
    assert_eq!(host("  http://sub.example.org#frag "), "sub.example.org");
    assert_eq!(host("example.com/page"), "example.com");
    assert_eq!(host("https://user@host:8080/x"), "user@host:8080");
    assert_eq!(host("https://пример.рф/путь"), "пример.рф");
    assert_eq!(host(""), "");
}

#[test]
fn human_size_uses_binary_units() {
    let t = tr();
    assert_eq!(human_size(0), format!("0 {}", t.unit_b));
    assert_eq!(human_size(1023), format!("1023 {}", t.unit_b));
    assert_eq!(human_size(1024), format!("1 {}", t.unit_kb));
    assert_eq!(human_size(1025), format!("2 {}", t.unit_kb));
    assert_eq!(human_size(1 << 20), format!("1.0 {}", t.unit_mb));
    assert_eq!(human_size(3 << 29), format!("1.5 {}", t.unit_gb));
}

#[test]
fn file_kinds_are_detected_by_extension() {
    for name in ["a.png", "b.JPG", "c.jpeg", "d.webp", "e.gif", "f.bmp", "g.ico"] {
        assert!(is_image(Path::new(name)), "{name}");
    }
    for name in ["a.tiff", "png", ".png", "a.png.txt", "a"] {
        assert!(!is_image(Path::new(name)), "{name}");
    }
    for name in
        ["a.zip", "b.RAR", "c.7z", "d.tar", "e.tar.gz", "f.tgz", "g.bz2", "h.xz", "i.zst", "j.cab", "k.lz", "l.lzma"]
    {
        assert!(is_archive(Path::new(name)), "{name}");
    }
    for name in ["a.zipx", "zip", "a.docx"] {
        assert!(!is_archive(Path::new(name)), "{name}");
    }
}

#[test]
fn vacant_finds_free_numbered_name() {
    let sandbox = Sandbox::new();
    let free = sandbox.join("free.txt");
    assert_eq!(vacant(free.clone()), free);
    let taken = sandbox.file("a.txt", "x");
    assert_eq!(vacant(taken.clone()), sandbox.join("a (2).txt"));
    sandbox.file("a (2).txt", "x");
    assert_eq!(vacant(taken), sandbox.join("a (3).txt"));
    let bare = sandbox.file("README", "x");
    assert_eq!(vacant(bare), sandbox.join("README (2)"));
    let dotfile = sandbox.file(".gitignore", "x");
    assert_eq!(vacant(dotfile), sandbox.join(".gitignore (2)"));
    let folder = sandbox.folder("Папка");
    assert_eq!(vacant(folder), sandbox.join("Папка (2)"));
}

#[test]
fn restore_note_recreates_missing_folders() {
    let sandbox = Sandbox::new();
    let path = sandbox.join("root/content/note-1.txt");
    restore_note(&path, "back").expect("restore");
    assert_eq!(fs::read_to_string(path).expect("note"), "back");
}

#[test]
fn failed_note_write_keeps_the_old_file_and_is_retried_on_the_next_save() {
    let sandbox = Sandbox::new();
    let mut store = sandbox.store();
    let key = store.add_note();
    store.edit_text(&key, "старый текст");
    store.save().expect("save");
    let path = store.items[0].path.clone();
    let name = path.file_name().expect("name").to_string_lossy().into_owned();
    let block = path.with_file_name(format!(".{name}.tmp"));
    fs::create_dir_all(&block).expect("block");
    store.edit_text(&key, "новый текст");
    assert!(store.save().is_err());
    assert_eq!(fs::read_to_string(&path).expect("note"), "старый текст");
    fs::remove_dir(&block).expect("unblock");
    store.save().expect("save");
    assert_eq!(fs::read_to_string(&path).expect("note"), "новый текст");
}

#[test]
fn virtual_file_write_failure_leaves_nothing() {
    let sandbox = Sandbox::new();
    let storage = sandbox.storage();
    fs::create_dir_all(storage.content()).expect("content");
    assert!(save_virtual(&storage, "bad?.txt", b"x").is_err());
    assert_eq!(fs::read_dir(storage.content()).expect("content").count(), 0);
    let saved = save_virtual(&storage, "отчёт 2024.txt", b"data").expect("save");
    assert_eq!(fs::read(saved).expect("read"), b"data");
}

#[test]
fn copy_path_copies_trees_and_refuses_recursion() {
    let sandbox = Sandbox::new();
    sandbox.file("src/a.txt", "a");
    sandbox.file("src/sub/b.txt", "b");
    copy_path(&sandbox.join("src"), &sandbox.join("dest")).expect("copy");
    assert_eq!(fs::read_to_string(sandbox.join("dest/sub/b.txt")).expect("b"), "b");
    assert!(copy_path(&sandbox.join("src"), &sandbox.join("src/sub/inner")).is_err());
    assert!(!sandbox.join("src/sub/inner").exists());
    assert!(copy_path(&sandbox.join("missing"), &sandbox.join("x")).is_err());
    let file = sandbox.file("one.txt", "1");
    copy_path(&file, &sandbox.join("two.txt")).expect("file");
    assert_eq!(fs::read_to_string(sandbox.join("two.txt")).expect("two"), "1");
}

#[test]
fn png_and_dib_round_trip() {
    let sandbox = Sandbox::new();
    let path = sandbox.file("pic.png", png(3, 2));
    let image = decode(ImageData::Png(png_bytes(&path).expect("png"))).expect("decode");
    assert_eq!(image.dimensions(), (3, 2));
    let dib = dib_bytes(&path).expect("dib");
    let restored = decode(ImageData::Dib(dib)).expect("dib decode");
    assert_eq!(restored.dimensions(), (3, 2));
    assert_eq!(restored.get_pixel(1, 1), image.get_pixel(1, 1));
}

#[test]
fn png_bytes_reads_png_as_is_and_converts_other_formats() {
    let sandbox = Sandbox::new();
    let original = png(2, 2);
    let path = sandbox.file("pic.PNG", &original);
    assert_eq!(png_bytes(&path).expect("png"), original);
    let bmp = sandbox.join("pic.bmp");
    image::load_from_memory(&original).expect("load").save(&bmp).expect("bmp");
    let converted = png_bytes(&bmp).expect("convert");
    assert!(converted.starts_with(b"\x89PNG"));
    assert!(png_bytes(&sandbox.file("fake.jpg", "not an image")).is_err());
}

#[test]
fn malformed_images_are_rejected() {
    assert!(decode(ImageData::Png(b"garbage".to_vec())).is_err());
    assert!(decode(ImageData::Dib(vec![0; 10])).is_err());
    let mut oversized = vec![0u8; 40];
    oversized[..4].copy_from_slice(&1000u32.to_le_bytes());
    assert!(decode(ImageData::Dib(oversized)).is_err());
    let mut palette = vec![0u8; 40];
    palette[..4].copy_from_slice(&40u32.to_le_bytes());
    palette[14..16].copy_from_slice(&8u16.to_le_bytes());
    palette[32..36].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(decode(ImageData::Dib(palette)).is_err());
}

#[test]
fn image_title_shows_dimensions() {
    let _english = language(Language::English);
    let image = image::RgbaImage::new(640, 480);
    assert_eq!(title(&image), "Image 640 × 480");
}

#[test]
fn saved_image_lands_in_storage() {
    let sandbox = Sandbox::new();
    let store = sandbox.store();
    let (path, name) = save_image(&store.storage, ImageData::Png(png(4, 5))).expect("save");
    assert!(store.storage.owned(&path));
    assert_eq!(open_image(&path).expect("open").to_rgba8().dimensions(), (4, 5));
    assert!(!name.is_empty());
}

#[test]
fn failed_image_write_is_cleaned_up() {
    let sandbox = Sandbox::new();
    let store = sandbox.store();
    let target = store.storage.content().join("missing-folder").join("x.png");
    assert!(write_png(&store.storage, &image::RgbaImage::new(1, 1), &target).is_err());
    assert!(!target.exists());
    assert!(save_image(&store.storage, ImageData::Png(b"bad".to_vec())).is_err());
    assert_eq!(fs::read_dir(store.storage.content()).expect("content").count(), 0);
}

#[test]
fn payload_subject_names_what_is_processed() {
    let paths = |names: &[&str]| Payload::Paths(names.iter().map(|name| PathBuf::from("C:/a b").join(name)).collect());
    assert_eq!(paths(&["отчёт.pdf"]).subject().as_deref(), Some("отчёт.pdf"));
    assert_eq!(paths(&["отчёт.pdf", "b.txt", "c"]).subject(), Some(format!("отчёт.pdf {}", fill(tr().and_more, 2))));
    assert_eq!(Payload::Text("  первая\n строка ".into()).subject().as_deref(), Some("первая строка"));
    assert_eq!(Payload::Text(" \n".into()).subject(), None);
}

#[test]
fn a_single_text_item_is_handed_over_as_text_and_the_rest_as_files() {
    let note = item(Kind::Note, "заметка", PathBuf::from(r"C:\Хранилище\note 1.txt"), Some("Привет, мир"));
    let blank = item(Kind::Note, "", PathBuf::from(r"C:\Хранилище\note 2.txt"), Some("  \n"));
    let file = item(Kind::File, "отчёт", PathBuf::from(r"C:\Мои файлы\отчёт.pdf"), None);
    assert!(Payload::of(&[]).is_none());
    assert!(matches!(Payload::of(std::slice::from_ref(&note)), Some(Payload::Text(text)) if text == "Привет, мир"));
    assert!(matches!(
        Payload::of(std::slice::from_ref(&blank)),
        Some(Payload::Paths(paths)) if paths == [PathBuf::from(r"C:\Хранилище\note 2.txt")]
    ));
    assert!(matches!(
        Payload::of(&[note, file]),
        Some(Payload::Paths(paths))
            if paths == [PathBuf::from(r"C:\Хранилище\note 1.txt"), PathBuf::from(r"C:\Мои файлы\отчёт.pdf")]
    ));
}

#[test]
fn watched_folder_listing_skips_hidden_and_unfinished_files() {
    use windows::Win32::Storage::FileSystem::{FILE_ATTRIBUTE_HIDDEN, SetFileAttributesW};
    use windows::core::HSTRING;
    let sandbox = Sandbox::new();
    sandbox.file("Загрузки/отчёт.pdf", "pdf");
    sandbox.file("Загрузки/фильм.mp4.crdownload", "partial");
    sandbox.file("Загрузки/архив.zip.PART", "partial");
    let hidden = sandbox.file("Загрузки/desktop.ini", "ini");
    unsafe { SetFileAttributesW(&HSTRING::from(hidden.as_path()), FILE_ATTRIBUTE_HIDDEN) }.expect("hide");
    sandbox.folder("Загрузки/Папка");
    let mut names: Vec<String> = list_folder(&sandbox.join("Загрузки"))
        .expect("listing")
        .iter()
        .map(|path| path.file_name().expect("name").to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(names, ["Папка", "отчёт.pdf"]);
}
