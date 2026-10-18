use super::Index;
use crate::search::{Grade, Query};
use crate::store::Store;
use crate::testing::Sandbox;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

fn refresh(index: &mut Index, store: &Store) {
    let pass = index.start(store.items()).expect("no pass is running");
    assert!(!index.finish(Some(pass.run())));
}

fn indexed(store: &Store) -> Index {
    let mut index = Index::default();
    refresh(&mut index, store);
    index
}

fn found(index: &Index, store: &Store, query: &str) -> Vec<String> {
    let query = Query::parse(query).expect("query");
    store
        .items()
        .iter()
        .filter(|item| query.find(&item.title, index.get(&item.id), Grade::Fragment).is_some())
        .map(|item| item.title.clone())
        .collect()
}

fn zipped(sandbox: &Sandbox, name: &str, parts: &[(&str, &str)]) -> PathBuf {
    let path = sandbox.join(name);
    let mut writer = ZipWriter::new(File::create(&path).expect("archive"));
    for (part, xml) in parts {
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        writer.start_file(*part, options).expect("part");
        writer.write_all(xml.as_bytes()).expect("xml");
    }
    writer.finish().expect("finished archive");
    path
}

#[test]
fn text_files_are_found_by_content_in_any_encoding() {
    let sandbox = Sandbox::new();
    let mut store = sandbox.store();
    let utf16: Vec<u8> =
        [0xFF, 0xFE].into_iter().chain("Квартальная ВЫРУЧКА".encode_utf16().flat_map(u16::to_le_bytes)).collect();
    store.add_paths(vec![
        sandbox.file("годовой отчёт/итоги.txt", "Итоговая выручка за год"),
        sandbox.file("квартал.md", utf16),
        sandbox.file("main.rs", "fn main() {}"),
        sandbox.file("track.mp3", "выручка"),
    ]);
    let index = indexed(&store);
    let mut titles = found(&index, &store, "выручка");
    titles.sort();
    assert_eq!(titles, ["итоги.txt", "квартал.md"]);
    assert_eq!(found(&index, &store, "fn main"), ["main.rs"]);
}

#[test]
fn office_documents_are_found_by_their_text() {
    let sandbox = Sandbox::new();
    let mut store = sandbox.store();
    store.add_paths(vec![
        zipped(
            &sandbox,
            "letter.docx",
            &[
                ("word/styles.xml", "<w:style>stylesonly</w:style>"),
                ("word/document.xml", "<w:p><w:r><w:t>Hel</w:t></w:r><w:r><w:t>lo AT&amp;T&#x21;</w:t></w:r></w:p>"),
            ],
        ),
        zipped(&sandbox, "table.xlsx", &[("xl/sharedStrings.xml", "<sst><si><t>Бюджет</t></si></sst>")]),
        zipped(&sandbox, "deck.pptx", &[("ppt/slides/slide2.xml", "<p:sld><a:t>Roadmap</a:t></p:sld>")]),
        zipped(&sandbox, "draft.odt", &[("content.xml", "<text:p>Черновик &lt;v2&gt;</text:p>")]),
    ]);
    let index = indexed(&store);
    assert_eq!(found(&index, &store, "hello at&t!"), ["letter.docx"]);
    assert_eq!(found(&index, &store, "бюджет"), ["table.xlsx"]);
    assert_eq!(found(&index, &store, "roadmap"), ["deck.pptx"]);
    assert_eq!(found(&index, &store, "<v2>"), ["draft.odt"]);
    assert!(found(&index, &store, "stylesonly").is_empty());
}

#[test]
fn paragraphs_of_a_document_do_not_merge_into_one_word() {
    let sandbox = Sandbox::new();
    let mut store = sandbox.store();
    let xml = "<w:p><w:r><w:t>end</w:t></w:r></w:p><w:p><w:r><w:t>start</w:t></w:r></w:p>";
    store.add_paths(vec![zipped(&sandbox, "story.docx", &[("word/document.xml", xml)])]);
    let index = indexed(&store);
    assert_eq!(found(&index, &store, "end start"), ["story.docx"]);
    assert!(found(&index, &store, "endstart").is_empty());
}

#[test]
fn broken_and_missing_files_are_not_found() {
    let sandbox = Sandbox::new();
    let mut store = sandbox.store();
    let gone = sandbox.file("gone.txt", "vanished words");
    store.add_paths(vec![sandbox.file("broken.docx", "not a zip, but words"), gone.clone()]);
    std::fs::remove_file(gone).expect("removed");
    let index = indexed(&store);
    assert!(found(&index, &store, "words").is_empty());
}

#[test]
fn changed_files_and_edited_texts_are_read_again() {
    let sandbox = Sandbox::new();
    let mut store = sandbox.store();
    let path = sandbox.file("plan.txt", "first draft");
    store.add_paths(vec![path.clone()]);
    let note = store.add_note();
    store.edit_text(&note, "old idea");
    let mut index = indexed(&store);
    std::fs::write(&path, "second version of the plan").expect("rewrite");
    store.edit_text(&note, "new idea");
    refresh(&mut index, &store);
    assert_eq!(found(&index, &store, "second"), ["plan.txt"]);
    assert!(found(&index, &store, "first").is_empty());
    assert_eq!(found(&index, &store, "new idea"), ["new idea"]);
    assert!(found(&index, &store, "old").is_empty());
}

#[test]
fn long_texts_are_searched_in_full() {
    let sandbox = Sandbox::new();
    let mut store = sandbox.store();
    store.add_text(format!("Header\n{}\nПоследняя строка", "filler ".repeat(50_000))).expect("text");
    let index = indexed(&store);
    assert_eq!(found(&index, &store, "последняя").len(), 1);
}

#[test]
fn a_refresh_during_a_pass_runs_once_more_after_it() {
    let sandbox = Sandbox::new();
    let store = sandbox.store();
    let mut index = Index::default();
    let pass = index.start(store.items()).expect("first pass");
    assert!(index.start(store.items()).is_none());
    assert!(index.finish(Some(pass.run())));
    let pass = index.start(store.items()).expect("second pass");
    assert!(!index.finish(Some(pass.run())));
    assert!(index.start(store.items()).is_some());
    assert!(!index.finish(None));
    assert!(index.start(store.items()).is_some());
}
