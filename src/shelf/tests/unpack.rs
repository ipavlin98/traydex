use super::*;
use crate::shelf::Attempt;
use std::io::Write;
use zip::write::SimpleFileOptions;
use zip::{AesMode, ZipWriter};

const PASSWORD: &str = "пароль";

pub(super) fn locked_zip(sandbox: &Sandbox, name: &str) -> PathBuf {
    let path = sandbox.join(name);
    let mut writer = ZipWriter::new(std::fs::File::create(&path).expect("zip"));
    let options = SimpleFileOptions::default().with_aes_encryption(AesMode::Aes256, PASSWORD);
    for entry in ["a.txt", "b.txt"] {
        writer.start_file(entry, options).expect("entry");
        writer.write_all(entry.as_bytes()).expect("write");
    }
    writer.finish().expect("finish");
    path
}

fn attempt(h: &mut Harness) -> Option<Attempt> {
    h.read(|shelf| shelf.unlocking().map(|unlocking| unlocking.attempt))
}

fn enter_password(h: &mut Harness, password: &str) {
    h.act(|shelf, window, cx| shelf.focus_password(window, cx));
    h.keys("ctrl-a backspace");
    h.cx.simulate_input(password);
    h.keys("enter");
}

#[gpui::test]
fn locked_archive_asks_for_the_password_in_the_window(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    let index = add_tile(&mut h, actions::Kind::Unpack);
    let archive = locked_zip(&h.sandbox, "Секрет.zip");
    h.act(|shelf, _, cx| shelf.perform(index, Payload::Paths(vec![archive.clone()]), cx));
    assert!(h.read(|shelf| shelf.page() == Page::Password));
    assert_eq!(h.read(|shelf| shelf.unlocking().map(|unlocking| unlocking.archives.clone())), Some(vec![archive]));
    assert_eq!(attempt(&mut h), Some(Attempt::Ready));
    enter_password(&mut h, "неверный");
    assert_eq!(attempt(&mut h), Some(Attempt::Wrong), "a wrong password keeps the page");
    assert!(h.read(|shelf| shelf.page() == Page::Password && shelf.store.items().is_empty()));
    assert!(!h.sandbox.join("Секрет").exists());
    h.cx.simulate_input("x");
    assert_eq!(attempt(&mut h), Some(Attempt::Ready), "typing clears the error");
    enter_password(&mut h, PASSWORD);
    let folder = h.sandbox.join("Секрет");
    assert!(h.read(|shelf| shelf.page() == Page::Shelf));
    assert_eq!(std::fs::read_to_string(folder.join("b.txt")).expect("unpacked"), "b.txt");
    assert_eq!(h.read(|shelf| shelf.store.items().iter().map(|item| item.path.clone()).collect::<Vec<_>>()), [folder]);
    assert_eq!(h.saved().items().len(), 1);
    assert_eq!(h.toast(), Some(fill(tr().unpacked, 1)));
}

#[gpui::test]
fn password_result_after_leaving_the_page_does_not_reopen_it(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    let index = add_tile(&mut h, actions::Kind::Unpack);
    let archive = locked_zip(&h.sandbox, "a.zip");
    h.act(|shelf, _, cx| shelf.perform(index, Payload::Paths(vec![archive]), cx));
    h.act(|shelf, window, cx| shelf.focus_password(window, cx));
    h.cx.simulate_input("неверный");
    h.pending(|shelf, _, cx| {
        shelf.unlock(cx);
        shelf.toggle_page(Page::Shelf, cx);
    });
    h.settle();
    assert!(h.read(|shelf| shelf.page() == Page::Shelf));
    assert!(h.alert().is_some_and(|alert| alert.contains(tr().wrong_password)));
}
