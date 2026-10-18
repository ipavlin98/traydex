use super::*;
use crate::testing::Sandbox;
use std::time::{Duration, SystemTime};

const NOON_2024_03_15: u64 = 1_710_504_000;

fn batch(files: &[PathBuf]) -> Batch {
    match prepare(files).expect("prepare").show {
        Some(Show::Rename(batch)) => batch,
        _ => panic!("expected a rename batch"),
    }
}

fn names(plan: &[(PathBuf, PathBuf)]) -> Vec<String> {
    plan.iter().map(|(_, to)| base(to)).collect()
}

fn stamped(sandbox: &Sandbox, name: &str) -> PathBuf {
    let path = sandbox.file(name, "x");
    let time = SystemTime::UNIX_EPOCH + Duration::from_secs(NOON_2024_03_15);
    fs::File::options().write(true).open(&path).expect("open").set_modified(time).expect("time");
    path
}

#[test]
fn dates_come_from_names_before_file_times() {
    let sandbox = Sandbox::new();
    let cases = [
        ("IMG_20261011_WA0023.jpg", "2026-10-11"),
        ("IMG-20261011-WA0023.jpg", "2026-10-11"),
        ("Screenshot 2026-10-11 at 10.15.00.png", "2026-10-11"),
        ("scan_2025.01.31.pdf", "2025-01-31"),
        ("Фото 2026_02_03.jpg", "2026-02-03"),
        ("Screenshot_92819.png", "2024-03-15"),
        ("IMG_20261311.jpg", "2024-03-15"),
        ("120261011.jpg", "2024-03-15"),
        ("202610115.jpg", "2024-03-15"),
        ("2026-10_11.jpg", "2024-03-15"),
        ("19891231.jpg", "2024-03-15"),
    ];
    for (name, expected) in cases {
        assert_eq!(date(&stamped(&sandbox, name)), expected, "{name}");
    }
}

#[test]
fn date_rule_numbers_each_day_separately_in_name_order() {
    let sandbox = Sandbox::new();
    let files = [
        sandbox.file("IMG_20261011_10.jpg", "x"),
        sandbox.file("IMG_20261011_9.jpg", "x"),
        sandbox.file("IMG_20261012_1.PNG", "x"),
        sandbox.file("IMG_20261011_100.jpg", "x"),
    ];
    let plan = batch(&files).plan(Rule::Date, "");
    assert_eq!(names(&plan), ["2026-10-11_01.jpg", "2026-10-11_02.jpg", "2026-10-11_03.jpg", "2026-10-12_01.PNG"]);
    assert_eq!(base(&plan[0].0), "IMG_20261011_9.jpg");
    assert!(plan.iter().all(|(from, to)| from.parent() == to.parent()));
}

#[test]
fn numbers_widen_for_large_batches() {
    let sandbox = Sandbox::new();
    let files: Vec<PathBuf> = (0..120).map(|n| sandbox.file(&format!("f{n}.txt"), "x")).collect();
    let plan = batch(&files).plan(Rule::Name, "Отпуск");
    assert_eq!(base(&plan[0].1), "Отпуск_001.txt");
    assert_eq!(base(&plan[119].1), "Отпуск_120.txt");
}

#[test]
fn name_rule_cleans_the_name_and_needs_one() {
    let sandbox = Sandbox::new();
    let files = [sandbox.file("a.jpg", "x"), sandbox.file("b.jpeg", "x"), sandbox.folder("Папка")];
    let batch = batch(&files);
    assert_eq!(
        names(&batch.plan(Rule::Name, "  Мой: отпуск?/2026.  ")),
        ["Мой отпуск2026_01.jpg", "Мой отпуск2026_02.jpeg", "Мой отпуск2026_03"]
    );
    for empty in ["", "   ", "???", "..."] {
        assert!(!batch.ready(Rule::Name, empty), "{empty:?}");
        assert!(batch.plan(Rule::Name, empty).iter().all(|(from, to)| from == to));
    }
    assert!(batch.ready(Rule::Date, ""));
}

#[test]
fn transliteration_keeps_case_and_extension() {
    let sandbox = Sandbox::new();
    let files = [
        sandbox.file("Привет Мир.TXT", "x"),
        sandbox.file("ЖУК и Щука.txt", "x"),
        sandbox.file("Ёжик, Юля, Ярослав — Ъ.md", "x"),
        sandbox.file("ready.txt", "x"),
        sandbox.folder("Папка.v2"),
    ];
    let plan = batch(&files).plan(Rule::Latin, "");
    assert_eq!(
        names(&plan),
        ["ready.txt", "ZHUK i Shchuka.txt", "Papka.v2", "Privet Mir.TXT", "Yozhik, Yulya, Yaroslav — .md"]
    );
    assert_eq!(latin("Щ"), "Shch");
    assert_eq!(latin("ЩИ"), "SHCHI");
    assert_eq!(latin("Їжак є"), "Yizhak ye");
}

#[test]
fn web_rule_makes_safe_lowercase_names() {
    let sandbox = Sandbox::new();
    let files = [
        sandbox.file("Мой Файл (копия).JPG", "x"),
        sandbox.file("!!!.txt", "x"),
        sandbox.file("--Report__2026--.PDF", "x"),
        sandbox.file("README", "x"),
    ];
    let plan = batch(&files).plan(Rule::Web, "");
    assert_eq!(names(&plan), ["file.txt", "report-2026.pdf", "readme", "moy-fayl-kopiya.jpg"]);
}

#[test]
fn renaming_swaps_names_and_avoids_existing_files() {
    let sandbox = Sandbox::new();
    let a = sandbox.file("a.txt", "A");
    let b = sandbox.file("b.txt", "B");
    let c = sandbox.file("c.txt", "C");
    sandbox.file("taken.txt", "keep");
    let plan = vec![(a.clone(), b.clone()), (b.clone(), a.clone()), (c.clone(), sandbox.join("taken.txt"))];
    let done = apply(plan).expect("rename");
    assert_eq!(fs::read_to_string(&a).expect("a"), "B");
    assert_eq!(fs::read_to_string(&b).expect("b"), "A");
    assert_eq!(fs::read_to_string(sandbox.join("taken.txt")).expect("taken"), "keep");
    assert_eq!(fs::read_to_string(sandbox.join("taken (2).txt")).expect("moved"), "C");
    assert_eq!(done[2], (c, sandbox.join("taken (2).txt")));
    let leftovers = fs::read_dir(sandbox.path()).expect("dir").filter_map(Result::ok);
    assert!(leftovers.into_iter().all(|entry| !entry.file_name().to_string_lossy().starts_with(".traydex")));
}

#[test]
fn equal_targets_get_distinct_names_and_case_changes_apply() {
    let sandbox = Sandbox::new();
    let photo = sandbox.file("Photo.JPG", "1");
    let other = sandbox.file("other.jpg", "2");
    let plan = vec![(photo, sandbox.join("photo.jpg")), (other, sandbox.join("photo.jpg"))];
    apply(plan).expect("rename");
    let mut names: Vec<String> = fs::read_dir(sandbox.path())
        .expect("dir")
        .filter_map(|entry| Some(entry.ok()?.file_name().to_string_lossy().into_owned()))
        .filter(|name| name.ends_with(".jpg"))
        .collect();
    names.sort();
    assert_eq!(names, ["photo (2).jpg", "photo.jpg"]);
    assert_eq!(fs::read_to_string(sandbox.join("photo.jpg")).expect("first"), "1");
}

#[test]
fn failed_rename_restores_every_name() {
    let sandbox = Sandbox::new();
    let first = sandbox.file("first.txt", "1");
    let missing = sandbox.join("missing.txt");
    let plan = vec![(first.clone(), sandbox.join("x.txt")), (missing, sandbox.join("y.txt"))];
    let failure = format!("{:#}", apply(plan).expect_err("missing source"));
    assert!(failure.starts_with("missing.txt"), "{failure}");
    assert_eq!(fs::read_to_string(&first).expect("restored"), "1");
    let names: Vec<String> = fs::read_dir(sandbox.path())
        .expect("dir")
        .filter_map(|entry| Some(entry.ok()?.file_name().to_string_lossy().into_owned()))
        .collect();
    assert_eq!(names, ["first.txt"]);
}

#[test]
fn unchanged_names_are_reported() {
    let sandbox = Sandbox::new();
    let file = sandbox.file("ready.txt", "x");
    let plan = batch(&[file]).plan(Rule::Latin, "");
    assert_eq!(apply(plan).expect_err("nothing to do").to_string(), tr().rename_unchanged);
}
