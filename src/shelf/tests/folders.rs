use super::*;

const SCAN: Duration = Duration::from_secs(2);

fn watch(h: &mut Harness, folder: &Path) {
    h.fake.state().picked.push_back(Ok(Some(folder.to_path_buf())));
    h.act(|shelf, _, cx| shelf.watch_folder(cx));
}

fn sorted_titles(h: &mut Harness) -> Vec<String> {
    let mut titles = h.titles();
    titles.sort();
    titles
}

fn key_of(h: &mut Harness, title: &str) -> ItemId {
    h.read(|shelf| shelf.store.items().iter().find(|item| item.title == title).expect("item on the shelf").id.clone())
}

#[gpui::test]
fn watched_folder_puts_its_files_on_the_shelf_and_follows_the_disk(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    h.sandbox.file("Мои загрузки/отчёт 1.pdf", "pdf");
    h.sandbox.file("Мои загрузки/Фото/кот.png", "png");
    let folder = h.sandbox.join("Мои загрузки");
    watch(&mut h, &folder);
    assert_eq!(sorted_titles(&mut h), ["Фото", "отчёт 1.pdf"]);
    assert_eq!(h.saved().watched().iter().map(|watched| watched.path().to_path_buf()).collect::<Vec<_>>(), [folder]);
    h.sandbox.file("Мои загрузки/новый.txt", "new");
    std::fs::remove_file(h.sandbox.join("Мои загрузки/отчёт 1.pdf")).expect("delete");
    h.elapse(SCAN);
    assert_eq!(sorted_titles(&mut h), ["Фото", "новый.txt"]);
    assert_eq!(h.saved().items().len(), 2);
}

#[gpui::test]
fn file_removed_from_the_shelf_stays_off_while_it_remains_in_the_folder(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    h.sandbox.file("watched/a.txt", "a");
    h.sandbox.file("watched/b.txt", "b");
    let folder = h.sandbox.join("watched");
    watch(&mut h, &folder);
    let key = key_of(&mut h, "b.txt");
    h.act(|shelf, _, cx| shelf.remove(&key, cx));
    h.elapse(SCAN);
    assert_eq!(h.titles(), ["a.txt"]);
    let mut reopened = h.saved();
    let listed = store::list_folder(&folder).expect("listing");
    reopened.sync_folder(&folder, listed);
    assert_eq!(reopened.items().iter().map(|item| item.title.as_str()).collect::<Vec<_>>(), ["a.txt"]);
}

#[gpui::test]
fn unreadable_watched_folder_keeps_its_items(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    h.sandbox.file("сеть/a.txt", "a");
    let folder = h.sandbox.join("сеть");
    watch(&mut h, &folder);
    std::fs::rename(h.sandbox.join("сеть"), h.sandbox.join("offline")).expect("folder goes away");
    h.elapse(SCAN);
    assert_eq!(h.titles(), ["a.txt"]);
}

#[gpui::test]
fn unwatching_removes_the_folder_items_except_pinned_and_stops_adding(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    h.sandbox.file("watched/a.txt", "a");
    h.sandbox.file("watched/b.txt", "b");
    let elsewhere = h.sandbox.file("elsewhere/c.txt", "c");
    h.act(|shelf, _, cx| shelf.ingest(Payload::Paths(vec![elsewhere]), cx));
    let folder = h.sandbox.join("watched");
    watch(&mut h, &folder);
    let pinned = key_of(&mut h, "a.txt");
    h.act(|shelf, _, cx| shelf.toggle_stuck(&pinned, cx));
    h.act(|shelf, _, cx| shelf.unwatch_folder(&folder, cx));
    assert_eq!(sorted_titles(&mut h), ["a.txt", "c.txt"]);
    assert!(h.saved().watched().is_empty());
    h.sandbox.file("watched/d.txt", "d");
    h.elapse(SCAN);
    assert_eq!(sorted_titles(&mut h), ["a.txt", "c.txt"]);
}
