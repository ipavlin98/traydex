use super::en::EN;
use super::{Language, Strings, current, fill, set, tr};
use crate::testing::language;
use serde_json::{Map, Value};

fn fields(strings: &Strings) -> Map<String, Value> {
    match serde_json::to_value(strings).expect("strings") {
        Value::Object(map) => map,
        other => panic!("strings must serialize to an object, got {other}"),
    }
}

fn table(language: Language) -> Map<String, Value> {
    fields(language.strings())
}

#[test]
fn every_language_defines_every_string() {
    for language in Language::ALL {
        for (name, value) in table(language) {
            let text = value.as_str().expect("string");
            assert!(!text.trim().is_empty(), "{} {name}", language.native_name());
        }
    }
}

#[test]
fn placeholders_match_english_in_every_language() {
    let english = fields(&EN);
    for language in Language::ALL {
        for (name, value) in table(language) {
            let text = value.as_str().expect("string");
            let reference = english[&name].as_str().expect("string");
            assert_eq!(
                text.matches("{}").count(),
                reference.matches("{}").count(),
                "{} {name}: {text}",
                language.native_name()
            );
        }
    }
}

#[test]
fn templates_use_only_positional_placeholders() {
    for language in Language::ALL {
        for (name, value) in table(language) {
            let text = value.as_str().expect("string").replace("{}", "");
            assert!(!text.contains('{') && !text.contains('}'), "{name}: {text}");
        }
    }
}

#[test]
fn fill_replaces_only_the_first_placeholder() {
    assert_eq!(fill("Image {} × {}", 640), "Image 640 × {}");
    assert_eq!(fill(&fill("Image {} × {}", 640), 480), "Image 640 × 480");
    assert_eq!(fill("no placeholder", 1), "no placeholder");
    assert_eq!(fill("{}", "{}"), "{}");
    assert_eq!(fill("Путь: {}", "C:\\Папка"), "Путь: C:\\Папка");
}

#[test]
fn language_switch_changes_strings_and_is_restored() {
    let before = current();
    {
        let _switched = language(Language::Russian);
        assert!(current() == Language::Russian);
        assert!(std::ptr::eq(tr(), Language::Russian.strings()));
        set(Language::French);
        assert!(current() == Language::French);
    }
    assert!(current() == before);
}

#[test]
fn language_switched_by_a_parallel_test_does_not_leak() {
    let before = current();
    std::thread::spawn(|| set(if current() == Language::Hindi { Language::Spanish } else { Language::Hindi }))
        .join()
        .expect("thread");
    assert!(current() == before);
}

#[test]
fn language_names_and_fonts_are_distinct() {
    let names: std::collections::HashSet<_> = Language::ALL.iter().map(|l| l.native_name()).collect();
    assert_eq!(names.len(), Language::ALL.len());
    assert_eq!(Language::Chinese.font(), "Microsoft YaHei UI");
    assert_eq!(Language::Hindi.font(), "Nirmala UI");
    assert_eq!(Language::Russian.font(), "Segoe UI");
}

#[test]
fn saved_language_names_are_stable() {
    let names: Vec<String> = Language::ALL.iter().map(|l| serde_json::to_string(l).expect("json")).collect();
    assert_eq!(names, ["\"English\"", "\"Chinese\"", "\"Hindi\"", "\"Spanish\"", "\"French\"", "\"Russian\""]);
    assert!(serde_json::from_str::<Language>("\"Klingon\"").is_err());
}
