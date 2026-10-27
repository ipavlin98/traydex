use super::*;
use crate::testing::{Fake, Sandbox, item, png};
use std::io::Write as _;
use std::path::PathBuf;

fn lines(loaded: &Loaded) -> Vec<(Option<usize>, String)> {
    match &loaded.content {
        Content::Text { lines, .. } => lines.iter().map(|line| (line.number, line.text.to_string())).collect(),
        _ => panic!("expected text"),
    }
}

fn names(loaded: &Loaded) -> Vec<(String, bool, Option<u64>)> {
    match &loaded.content {
        Content::Entries(entries) => {
            entries.iter().map(|entry| (entry.name.to_string(), entry.folder, entry.size)).collect()
        }
        _ => panic!("expected entries"),
    }
}

fn mono(loaded: &Loaded) -> bool {
    matches!(loaded.content, Content::Text { mono: true, .. })
}

fn open(sandbox: &Sandbox, fake: &Fake, name: &str, bytes: impl AsRef<[u8]>) -> Loaded {
    let path = sandbox.file(name, bytes);
    load(&item(Kind::File, name, path, None), fake)
}

#[test]
fn missing_items_are_reported() {
    let sandbox = Sandbox::new();
    let fake = Fake::new(&sandbox);
    for kind in [Kind::File, Kind::Image, Kind::Folder] {
        let loaded = load(&item(kind, "x", sandbox.join("gone"), None), &fake);
        assert!(matches!(loaded.content, Content::Missing));
        assert!(loaded.detail.is_empty());
    }
}

#[test]
fn notes_and_links_show_trimmed_unnumbered_text() {
    let sandbox = Sandbox::new();
    let fake = Fake::new(&sandbox);
    let note = load(&item(Kind::Text, "n", PathBuf::new(), Some("\n  first\n\tsecond  \n\n")), &fake);
    assert_eq!(lines(&note), [(None, "first".to_string()), (None, "    second".to_string())]);
    assert!(!mono(&note));
    assert_eq!(note.detail, fill(tr().text_detail, 20));
    let link = load(&item(Kind::Link, "l", PathBuf::new(), Some("https://example.com")), &fake);
    assert_eq!(lines(&link), [(None, "https://example.com".to_string())]);
    let blank = load(&item(Kind::Text, "b", PathBuf::new(), Some("   ")), &fake);
    assert_eq!(blank.count(), 0);
}

#[test]
fn images_show_dimensions() {
    let sandbox = Sandbox::new();
    let fake = Fake::new(&sandbox);
    let good = load(&item(Kind::Image, "i", sandbox.file("a.png", png(3, 2)), None), &fake);
    assert!(matches!(good.content, Content::Image(Some(natural)) if natural == gpui::size(3., 2.)));
    assert!(good.detail.ends_with(" · 3 × 2"), "{}", good.detail);
    let broken = load(&item(Kind::Image, "i", sandbox.file("b.png", "junk"), None), &fake);
    assert!(matches!(broken.content, Content::Image(None)));
    assert!(!broken.detail.contains('×'));
}

#[test]
fn folders_list_subfolders_first_by_name() {
    let sandbox = Sandbox::new();
    let fake = Fake::new(&sandbox);
    sandbox.file("dir/b.txt", "12345");
    sandbox.file("dir/A.txt", "");
    sandbox.folder("dir/zeta");
    sandbox.folder("dir/Alpha");
    let loaded = load(&item(Kind::Folder, "d", sandbox.join("dir"), None), &fake);
    assert_eq!(
        names(&loaded),
        [
            ("Alpha".to_string(), true, None),
            ("zeta".to_string(), true, None),
            ("A.txt".to_string(), false, Some(0)),
            ("b.txt".to_string(), false, Some(5)),
        ]
    );
    assert_eq!(loaded.detail, fill(tr().preview_items, 4));
    assert!(!loaded.partial);
    let empty = load(&item(Kind::Folder, "e", sandbox.folder("empty"), None), &fake);
    assert!(names(&empty).is_empty());
}

#[test]
fn huge_folders_are_listed_partially() {
    let sandbox = Sandbox::new();
    let fake = Fake::new(&sandbox);
    let dir = sandbox.folder("many");
    for index in 0..=ENTRY_LIMIT {
        File::create(dir.join(format!("{index:05}"))).expect("file");
    }
    let loaded = load(&item(Kind::Folder, "m", dir, None), &fake);
    assert_eq!(names(&loaded).len(), ENTRY_LIMIT);
    assert!(loaded.partial);
    assert_eq!(loaded.detail, fill(tr().preview_items, ENTRY_LIMIT + 1));
}

#[test]
fn text_files_are_numbered_and_tabs_expanded() {
    let sandbox = Sandbox::new();
    let fake = Fake::new(&sandbox);
    let loaded = open(&sandbox, &fake, "main.rs", "fn main() {\r\n\tprintln!();\r\n}\r\n");
    assert!(mono(&loaded));
    assert_eq!(
        lines(&loaded),
        [(Some(1), "fn main() {".to_string()), (Some(2), "    println!();".to_string()), (Some(3), "}".to_string())]
    );
    assert!(loaded.detail.ends_with(&fill(tr().preview_lines, 3)), "{}", loaded.detail);
    assert_eq!(loaded.count(), 3);
    assert!(!loaded.partial);
}

#[test]
fn long_lines_are_split_into_chunks() {
    let sandbox = Sandbox::new();
    let fake = Fake::new(&sandbox);
    let text = format!("{}\nshort", "ж".repeat(LINE_CHUNK * 2 + 5));
    let loaded = open(&sandbox, &fake, "long.txt", text);
    let lines = lines(&loaded);
    assert_eq!(lines.len(), 4);
    assert_eq!(lines[0].0, Some(1));
    assert_eq!(lines[1].0, None);
    assert_eq!(lines[2], (None, "ж".repeat(5)));
    assert_eq!(lines[3], (Some(2), "short".to_string()));
    assert_eq!(lines[0].1.chars().count(), LINE_CHUNK);
}

#[test]
fn byte_order_marks_select_encoding() {
    let sandbox = Sandbox::new();
    let fake = Fake::new(&sandbox);
    let utf8 = open(&sandbox, &fake, "a.txt", b"\xEF\xBB\xBFhello");
    assert_eq!(lines(&utf8)[0].1, "hello");
    let le: Vec<u8> = b"\xFF\xFE".iter().copied().chain("привет".encode_utf16().flat_map(u16::to_le_bytes)).collect();
    assert_eq!(lines(&open(&sandbox, &fake, "le.txt", le))[0].1, "привет");
    let be: Vec<u8> = b"\xFE\xFF".iter().copied().chain("日本".encode_utf16().flat_map(u16::to_be_bytes)).collect();
    assert_eq!(lines(&open(&sandbox, &fake, "be.txt", be))[0].1, "日本");
    let odd = [0xFF, 0xFE, b'a', 0, b'b'];
    assert_eq!(lines(&open(&sandbox, &fake, "odd.txt", odd))[0].1, "a");
}

#[test]
fn invalid_utf8_falls_back_to_ansi_code_page() {
    let sandbox = Sandbox::new();
    let fake = Fake::new(&sandbox);
    let loaded = open(&sandbox, &fake, "legacy.txt", b"plain ascii start \xC0\x80 end");
    assert!(lines(&loaded)[0].1.starts_with("plain ascii start"));
}

#[test]
fn large_text_is_truncated_on_a_character_boundary() {
    let sandbox = Sandbox::new();
    let fake = Fake::new(&sandbox);
    let path = sandbox.join("big.log");
    let mut file = File::create(&path).expect("file");
    file.write_all(&vec![b'a'; TEXT_LIMIT - 1]).expect("write");
    file.write_all("яяя".as_bytes()).expect("write");
    drop(file);
    let loaded = load(&item(Kind::File, "big", path, None), &fake);
    assert!(loaded.partial);
    let text: String = lines(&loaded).into_iter().map(|(_, text)| text).collect();
    assert_eq!(text.len(), TEXT_LIMIT - 1);
    assert!(!text.contains('\u{fffd}'));
    let exact = open(&sandbox, &fake, "exact.log", vec![b'b'; TEXT_LIMIT]);
    assert!(!exact.partial);
}

#[test]
fn binary_data_is_not_shown_as_text() {
    let sandbox = Sandbox::new();
    let fake = Fake::new(&sandbox);
    fake.state().type_name = Some("Data file".into());
    let loaded = open(&sandbox, &fake, "data.txt", b"\x00\x01\x02binary");
    assert!(matches!(loaded.content, Content::Thumbnail(None)));
    assert_eq!(loaded.detail, format!("Data file · {}", human_size(9)));
    let controls: Vec<u8> = (0..200).map(|index| if index % 100 == 0 { 0x07 } else { b'a' }).collect();
    assert!(matches!(open(&sandbox, &fake, "bell.log", &controls).content, Content::Text { .. }));
    let noisy: Vec<u8> = (0..200).map(|index| if index % 20 == 0 { 0x07 } else { b'a' }).collect();
    assert!(matches!(open(&sandbox, &fake, "noisy.log", &noisy).content, Content::Thumbnail(_)));
}

#[test]
fn zip_like_archives_list_entries() {
    let sandbox = Sandbox::new();
    let fake = Fake::new(&sandbox);
    let path = sandbox.join("lib.JAR");
    let mut writer = zip::ZipWriter::new(File::create(&path).expect("zip"));
    let options = zip::write::SimpleFileOptions::default();
    writer.add_directory("META-INF/", options).expect("dir");
    writer.start_file("META-INF/MANIFEST.MF", options).expect("file");
    writer.write_all(b"Manifest-Version: 1.0").expect("write");
    writer.finish().expect("finish");
    let loaded = load(&item(Kind::File, "lib", path, None), &fake);
    assert_eq!(
        names(&loaded),
        [("META-INF".to_string(), true, None), ("META-INF/MANIFEST.MF".to_string(), false, Some(21))]
    );
    assert!(loaded.detail.ends_with(&fill(tr().preview_items, 2)));
}

#[test]
fn broken_archive_falls_back_to_text_or_thumbnail() {
    let sandbox = Sandbox::new();
    let fake = Fake::new(&sandbox);
    assert!(matches!(open(&sandbox, &fake, "fake.zip", "not really a zip").content, Content::Text { .. }));
    assert!(matches!(open(&sandbox, &fake, "bad.zip", b"PK\x03\x04\x00\x00").content, Content::Thumbnail(None)));
}

#[test]
fn svg_is_rendered_as_image() {
    let sandbox = Sandbox::new();
    let fake = Fake::new(&sandbox);
    assert!(matches!(open(&sandbox, &fake, "icon.SVG", "<svg/>").content, Content::Image(None)));
}

#[test]
fn shell_preview_handler_takes_priority_over_text_sniffing() {
    let sandbox = Sandbox::new();
    let fake = Fake::new(&sandbox);
    let clsid = Handler(0x12345678_1234_1234_1234_1234567890ab);
    fake.state().handler = Some(clsid);
    let loaded = open(&sandbox, &fake, "report.docx", "plain looking text");
    assert!(matches!(loaded.content, Content::Shell(found) if found == clsid));
    let text = open(&sandbox, &fake, "notes.md", "# title");
    assert!(matches!(text.content, Content::Text { .. }));
}

#[test]
fn unknown_text_like_files_are_sniffed() {
    let sandbox = Sandbox::new();
    let fake = Fake::new(&sandbox);
    let loaded = open(&sandbox, &fake, "notes.weird", "readable words");
    assert_eq!(lines(&loaded)[0].1, "readable words");
}

#[test]
fn thumbnails_come_from_the_shell() {
    let sandbox = Sandbox::new();
    let fake = Fake::new(&sandbox);
    fake.state().thumbnail = Some((2, 1, vec![255; 8]));
    assert!(matches!(open(&sandbox, &fake, "movie.mkv", [0u8; 16]).content, Content::Thumbnail(Some(_))));
    fake.state().thumbnail = Some((4, 4, vec![255; 8]));
    assert!(matches!(open(&sandbox, &fake, "movie2.mkv", [0u8; 16]).content, Content::Thumbnail(None)));
    let fallback = fallback(&sandbox.join("movie.mkv"), "detail".into(), &fake);
    assert!(matches!(fallback.content, Content::Thumbnail(None)));
    assert_eq!(fallback.detail, "detail");
}

#[test]
fn failed_result_is_empty() {
    let failed = Loaded::failed();
    assert!(matches!(failed.content, Content::Failed));
    assert!(failed.detail.is_empty() && !failed.partial && failed.count() == 0);
}

#[test]
fn split_counts_lines_even_when_blank() {
    let (lines, count) = split(" \n \n", true);
    assert!(lines.is_empty());
    assert_eq!(count, 2);
    let (lines, count) = split("", false);
    assert!(lines.is_empty());
    assert_eq!(count, 0);
}
