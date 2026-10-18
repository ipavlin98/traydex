#![expect(
    clippy::single_range_in_vec_init,
    reason = "search results are vectors of ranges, a single match is one range"
)]

use super::{Grade, Query, folded};
use std::ops::Range;

fn find(query: &str, title: &str) -> Option<Vec<Range<usize>>> {
    Query::parse(query).expect("query").find(title, None, Grade::Fragment)
}

fn word(query: &str, title: &str) -> Option<Vec<Range<usize>>> {
    Query::parse(query).expect("query").find(title, None, Grade::Word)
}

fn fuzzy(query: &str, title: &str) -> Option<Vec<Range<usize>>> {
    Query::parse(query).expect("query").find(title, None, Grade::Typo)
}

fn assert_on_boundaries(title: &str, ranges: &[Range<usize>]) {
    for range in ranges {
        assert!(range.start < range.end, "{range:?}");
        assert!(title.is_char_boundary(range.start) && title.is_char_boundary(range.end), "{range:?}");
    }
    assert!(ranges.windows(2).all(|pair| pair[0].end < pair[1].start), "{ranges:?}");
}

#[test]
fn empty_and_blank_queries_are_absent() {
    assert!(Query::parse("").is_none());
    assert!(Query::parse("   ").is_none());
    assert!(Query::parse("\t\n \u{3000}").is_none());
}

#[test]
fn surrounding_and_repeated_spaces_are_ignored() {
    assert_eq!(find("  report  ", "Annual report"), Some(vec![7..13]));
    assert_eq!(find("annual    report", "Annual report"), Some(vec![0..6, 7..13]));
}

#[test]
fn matching_ignores_case() {
    assert_eq!(find("HELLO", "hello world"), Some(vec![0..5]));
    assert_eq!(find("world", "HELLO WORLD"), Some(vec![6..11]));
    assert_eq!(find("привет", "ПРИВЕТ"), Some(vec![0..12]));
}

#[test]
fn every_word_must_match() {
    assert_eq!(find("quarterly report", "Quarterly sales report"), Some(vec![0..9, 16..22]));
    assert_eq!(find("report quarterly", "Quarterly sales report"), Some(vec![0..9, 16..22]));
    assert_eq!(find("quarterly invoice", "Quarterly sales report"), None);
}

#[test]
fn all_occurrences_are_highlighted() {
    assert_eq!(find("an", "banana"), Some(vec![1..5]));
    assert_eq!(find("a", "banana"), Some(vec![1..2, 3..4, 5..6]));
}

#[test]
fn overlapping_and_touching_ranges_are_merged() {
    assert_eq!(find("hel llo", "hello"), Some(vec![0..5]));
    assert_eq!(find("he llo", "hello"), Some(vec![0..5]));
}

#[test]
fn wrong_keyboard_layout_is_recognised_both_ways() {
    assert_eq!(find("ghbdtn", "Привет мир"), Some(vec![0..12]));
    assert_eq!(find("руддщ", "hello"), Some(vec![0..5]));
    assert_eq!(find("vbh", "Привет мир"), Some(vec![13..19]));
}

#[test]
fn yo_and_ye_are_equivalent() {
    assert_eq!(find("ёлка", "Елка"), Some(vec![0..8]));
    assert_eq!(find("елка", "Ёлка"), Some(vec![0..8]));
    assert_eq!(find("ЁЖ", "ёжик"), Some(vec![0..4]));
}

#[test]
fn ranges_are_byte_offsets_in_unicode_titles() {
    assert_eq!(find("мир", "Привет мир"), Some(vec![13..19]));
    assert_eq!(find("smile", "🙂 smile"), Some(vec![5..10]));
    let title = "日本語のファイル.txt";
    let ranges = find("txt", title).expect("match");
    assert_eq!(&title[ranges[0].clone()], "txt");
}

#[test]
fn special_characters_match_literally() {
    assert_eq!(find("c++", "Notes on C++"), Some(vec![9..12]));
    assert_eq!(find("a.b", "axb a.b"), Some(vec![4..7]));
    assert_eq!(find("[draft]", "[Draft] plan"), Some(vec![0..7]));
}

#[test]
fn body_match_keeps_item_without_highlight() {
    let query = Query::parse("invoice").expect("query");
    assert_eq!(query.find("Notes", Some("send the invoice tomorrow"), Grade::Fragment), Some(vec![]));
    assert_eq!(query.find("Notes", Some("nothing relevant"), Grade::Fragment), None);
    assert_eq!(query.find("Notes", None, Grade::Fragment), None);
}

#[test]
fn title_and_body_words_combine() {
    let query = Query::parse("notes invoice").expect("query");
    assert_eq!(query.find("Notes", Some("about the invoice"), Grade::Fragment), Some(vec![0..5]));
    assert_eq!(query.find("Other", Some("about the invoice"), Grade::Fragment), None);
}

#[test]
fn content_is_searched_to_the_end() {
    let content = folded(&format!("{}Needle", "x".repeat(100_000)));
    let query = Query::parse("needle").expect("query");
    assert_eq!(query.find("title", Some(&content), Grade::Fragment), Some(vec![]));
}

#[test]
fn folded_content_ignores_case_and_yo() {
    let content = folded("Счёт за ИЮНЬ");
    assert_eq!(Query::parse("счет июнь").expect("query").find("title", Some(&content), Grade::Fragment), Some(vec![]));
    assert_eq!(Query::parse("СЧЁТ").expect("query").find("title", Some(&content), Grade::Fragment), Some(vec![]));
}

#[test]
fn body_matches_layout_and_yo_variants() {
    let query = Query::parse("ghbdtn ёж").expect("query");
    assert_eq!(query.find("Note", Some("привет, ежик"), Grade::Fragment), Some(vec![]));
}

#[test]
fn typos_are_ignored_unless_enabled() {
    assert_eq!(find("helo", "hello"), None);
    let ranges = fuzzy("helo", "hello").expect("fuzzy match");
    assert!(!ranges.is_empty());
    assert_on_boundaries("hello", &ranges);
    assert_eq!(ranges[0].start, 0);
}

#[test]
fn short_words_never_match_with_typos() {
    assert_eq!(fuzzy("hlo", "hello"), None);
    assert_eq!(fuzzy("cta", "cat"), None);
}

#[test]
fn medium_words_allow_one_typo() {
    assert!(fuzzy("wrold", "hello world").is_some());
    assert!(fuzzy("hlelo", "hello").is_some());
    assert!(fuzzy("worldd", "world").is_some());
    assert_eq!(fuzzy("wrlod", "world"), None);
    assert_eq!(fuzzy("abcd", "wxyz"), None);
}

#[test]
fn long_words_allow_two_typos() {
    assert!(fuzzy("programing", "programming").is_some());
    assert!(fuzzy("porgraming", "programming").is_some());
    assert_eq!(fuzzy("porgrmaing", "programming"), None);
}

#[test]
fn fuzzy_ranges_point_at_the_misspelled_word() {
    let title = "hello world";
    let ranges = fuzzy("wrold", title).expect("match");
    assert_on_boundaries(title, &ranges);
    assert!(ranges.iter().all(|range| range.start >= 6), "{ranges:?}");
}

#[test]
fn fuzzy_ranges_stay_on_character_boundaries() {
    let title = "Документация проекта";
    let ranges = fuzzy("документацыя", title).expect("match");
    assert_on_boundaries(title, &ranges);
    assert_eq!(ranges[0].start, 0);
}

#[test]
fn exact_title_match_wins_over_body_and_typos() {
    let query = Query::parse("report").expect("query");
    assert_eq!(query.find("report", Some("report"), Grade::Typo), Some(vec![0..6]));
}

#[test]
fn query_with_mixed_terms_fails_if_one_is_missing_everywhere() {
    let query = Query::parse("report zzzz").expect("query");
    assert_eq!(query.find("report", Some("body"), Grade::Typo), None);
}

#[test]
fn locate_highlights_words_in_the_title_and_the_detail() {
    let query = Query::parse("тема светлая").expect("query");
    let found = query.locate("Тема оформления", "Светлая, тёмная или как в системе", Grade::Fragment);
    assert_eq!(found, Some((vec![0..8], vec![0..14])));
}

#[test]
fn locate_misses_when_a_word_is_in_neither_part() {
    let query = Query::parse("тема ярлык").expect("query");
    assert_eq!(query.locate("Тема", "Светлая или тёмная", Grade::Fragment), None);
    assert_eq!(query.locate("", "", Grade::Fragment), None);
}

#[test]
fn locate_ignores_typos_and_finds_layout_mistakes() {
    assert_eq!(Query::parse("ntvf").expect("query").locate("Тема", "", Grade::Fragment), Some((vec![0..8], vec![])));
    assert_eq!(Query::parse("темма").expect("query").locate("Тема", "оформление", Grade::Fragment), None);
}

#[test]
fn word_grade_matches_only_the_start_of_a_word() {
    assert_eq!(word("в", "сканов PDF"), None);
    assert_eq!(find("в", "сканов PDF"), Some(vec![10..12]));
    assert_eq!(word("в pdf", "В PDF"), Some(vec![0..2, 3..6]));
    assert_eq!(word("рх", "ZIP-архив"), None);
    assert_eq!(word("архив", "ZIP-архив"), Some(vec![4..14]));
}

#[test]
fn words_start_after_symbols_digits_and_case_changes() {
    assert_eq!(word("pdf", "отчёт.pdf"), Some(vec![11..14]));
    assert_eq!(word("2024", "IMG_20240101"), Some(vec![4..8]));
    assert_eq!(word("img", "scan2img"), Some(vec![5..8]));
    assert_eq!(word("doc", "MyDocument"), Some(vec![2..5]));
    assert_eq!(word("ument", "MyDocument"), None);
    assert_eq!(word("[draft]", "[Draft] plan"), Some(vec![0..7]));
}

#[test]
fn every_ideograph_starts_a_word() {
    let title = "日本語のファイル文件";
    let ranges = word("文件", title).expect("match");
    assert_eq!(&title[ranges[0].clone()], "文件");
}

#[test]
fn words_that_start_a_word_are_highlighted_only_there() {
    assert_eq!(find("в", "Видео в сканов"), Some(vec![0..2, 11..13]));
}

#[test]
fn wrong_layout_guess_never_matches_inside_a_word() {
    let query = Query::parse("в pdf").expect("query");
    let search = ("Поиск в интернете", "Ищет текст, название файла или картинку в браузере");
    assert_eq!(query.locate(search.0, search.1, Grade::Fragment), None);
    assert_eq!(find("зва", "название"), Some(vec![4..10]), "typed as is, the fragment is still found");
}

#[test]
fn content_grades_like_the_title() {
    let query = Query::parse("вод").expect("query");
    let content = folded("Новый заводской номер");
    assert_eq!(query.find("Note", Some(&content), Grade::Word), None);
    assert_eq!(query.find("Note", Some(&content), Grade::Fragment), Some(vec![]));
}

#[test]
fn grade_is_set_by_the_weakest_word() {
    let query = Query::parse("в pdf").expect("query");
    assert_eq!(query.grade("В PDF", "Склеивает картинки в один PDF", Grade::Fragment), Some(Grade::Word));
    let ocr = ("Распознать текст", "Копирует текст со скриншотов, фото и сканов PDF");
    assert_eq!(query.grade(ocr.0, ocr.1, Grade::Fragment), Some(Grade::Fragment));
    assert_eq!(query.grade("Тема", "Светлая", Grade::Fragment), None);
}
