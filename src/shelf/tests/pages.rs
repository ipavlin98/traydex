use super::*;

#[gpui::test]
fn every_page_renders_with_rich_state(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, |sandbox, store, _| {
        store.add_paths(vec![sandbox.file("a.txt", "x"), sandbox.file("b.png", png(3, 3)), sandbox.folder("c")]);
        store.add_text("https://example.com/page".into()).expect("link");
        store.add_text("multi\nline note".into()).expect("note");
    });
    let ids = h.ids();
    h.render();
    h.act(|shelf, _, cx| {
        shelf.toggle(&ids[0], cx);
        shelf.fail("visible alert", cx);
        shelf.ask_clear(cx);
    });
    h.render();
    h.act(|shelf, _, cx| {
        shelf.cancel_confirm(cx);
        shelf.toggle_layout(cx);
    });
    h.render();
    h.send(Event::Hover(true));
    h.render();
    h.pending(|shelf, _, cx| shelf.install(Tool::Ffmpeg, Slot::Single(Single::Gif), cx));
    h.render();
    h.fake.drop_jobs();
    h.send(Event::Hover(false));
    for id in &ids {
        h.act(|shelf, window, cx| shelf.show_preview(id.clone(), window, cx));
        h.render();
    }
    h.act(|shelf, _, cx| shelf.toggle_page(Page::Actions, cx));
    h.render();
    let tiles: Vec<TileId> = h.read(|shelf| shelf.tiles.iter().map(|tile| tile.id).collect());
    for tile in tiles {
        h.act(|shelf, window, cx| shelf.open_editor(tile, Some(Field::Name), window, cx));
        h.render();
    }
    h.act(|shelf, _, cx| {
        shelf.toggle_page(Page::Settings, cx);
        shelf.toggle_menu(Dropdown::Language, cx);
        shelf.record(Command::Toggle, cx);
    });
    h.render();
    h.act(|shelf, _, cx| shelf.ask_reset(cx));
    h.render();
}

fn clipboard(h: &mut Harness) -> Option<String> {
    h.cx.read_from_clipboard().and_then(|item| item.text())
}

#[gpui::test]
fn visible_success_shows_a_toast_that_fades(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    let file = h.sandbox.file("a.txt", "a");
    let zip = h.tile(0);
    h.act(|shelf, _, cx| shelf.perform(zip, Payload::Paths(vec![file]), cx));
    assert_eq!(h.toast(), Some(tr().zipped.to_string()));
    assert!(!h.fake.calls().iter().any(|call| matches!(call, Call::Notify(_))));
    h.elapse(Duration::from_millis(9900));
    assert!(h.toast().is_some(), "a success stays as long as an error");
    h.elapse(Duration::from_millis(200));
    assert!(h.toast().is_none());
}

#[gpui::test]
fn hiding_the_window_takes_success_notices_away_at_once(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    h.act(|shelf, _, cx| {
        shelf.show_toast("SHA-256 скопирован".into(), cx);
        shelf.flash_copied("copy-sha256:0".into(), cx);
        shelf.hide("test", cx);
    });
    assert_eq!(h.toast(), None, "a toast left for its timer reappears on the next show");
    assert!(h.read_cx(|shelf, cx| shelf.notices.read(cx).copied().is_none()));
}

#[gpui::test]
fn hiding_the_window_takes_an_error_away_at_once(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    h.act(|shelf, _, cx| {
        shelf.fail("boom", cx);
        shelf.hide("test", cx);
    });
    assert_eq!(h.alert(), None);
}

#[gpui::test]
fn notices_stay_with_the_page_they_were_shown_on(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    h.act(|shelf, _, cx| {
        shelf.toggle_page(Page::Settings, cx);
        shelf.show_toast("SHA-256 скопирован".into(), cx);
        shelf.fail("boom", cx);
        shelf.toggle_page(Page::Shelf, cx);
    });
    assert_eq!((h.toast(), h.alert()), (None, None));
}

#[gpui::test]
fn hash_page_opens_at_once_and_fills_in_when_hashes_are_ready(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    let index =
        add_tile(&mut h, actions::Kind::Hash { algorithm: actions::Algorithm::Sha256, virustotal: String::new() });
    let file = h.sandbox.file("большой образ.iso", "abc");
    h.pending(|shelf, _, cx| shelf.perform(index, Payload::Paths(vec![file.clone()]), cx));
    let computing = h.read(|shelf| match shelf.pending() {
        Some(actions::Pending::Hashes { files: Some(files), checking: false }) => files.clone(),
        _ => Vec::new(),
    });
    assert_eq!(computing, [file], "the page shows the file before its hashes are counted");
    assert!(h.read(|shelf| shelf.page() == Page::Hashes));
    h.settle();
    assert_eq!(h.read(|shelf| shelf.hashes().map(|digests| digests.len())), Some(1));
}

#[gpui::test]
fn shelf_notices_live_in_the_title_and_page_notices_at_the_bottom_for_less(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    assert!(h.read(|shelf| shelf.visible_items().is_empty()));
    assert_eq!(h.read(|shelf| shelf.notice_place()), crate::shelf::Place::Title, "even an empty shelf");
    h.act(|shelf, _, cx| shelf.show_toast("done".into(), cx));
    h.elapse(Duration::from_millis(9900));
    assert!(h.toast().is_some());
    h.elapse(Duration::from_millis(200));
    assert!(h.toast().is_none());
    h.act(|shelf, _, cx| {
        shelf.toggle_page(Page::Settings, cx);
        shelf.fail("boom", cx);
    });
    assert_eq!(h.read(|shelf| shelf.notice_place()), crate::shelf::Place::Bottom);
    h.act(|shelf, _, cx| shelf.ask_reset(cx));
    assert_eq!(h.read(|shelf| shelf.notice_place()), crate::shelf::Place::Bottom, "the bottom bar doesn't move it");
    h.elapse(Duration::from_millis(4900));
    assert!(h.alert().is_some());
    h.elapse(Duration::from_millis(200));
    assert!(h.alert().is_none());
}

#[gpui::test]
fn hovering_a_notice_pauses_its_timer(cx: &mut TestAppContext) {
    use crate::shelf::Notice;
    let mut h = open(cx, true);
    h.act(|shelf, _, cx| {
        shelf.toggle_page(Page::Settings, cx);
        shelf.show_toast("done".into(), cx);
    });
    h.elapse(Duration::from_secs(3));
    h.act(|shelf, _, cx| shelf.hold_notice(Notice::Toast, true, cx));
    h.elapse(Duration::from_secs(30));
    assert!(h.toast().is_some(), "stays while the pointer is on it");
    h.act(|shelf, _, cx| shelf.hold_notice(Notice::Toast, false, cx));
    h.elapse(Duration::from_millis(1900));
    assert!(h.toast().is_some(), "the remaining two seconds continue, not a fresh timer and not zero");
    h.elapse(Duration::from_millis(200));
    assert!(h.toast().is_none());
    h.act(|shelf, _, cx| {
        shelf.hold_notice(Notice::Alert, true, cx);
        shelf.dismiss_notice(Notice::Alert, cx);
        shelf.fail("boom", cx);
    });
    h.elapse(Duration::from_millis(5100));
    assert!(h.alert().is_none(), "a dismissed notice does not leave the next one paused");
}

#[gpui::test]
fn every_page_tile_opens_its_page_before_the_work_finishes(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, |_, store, _| plant_tools(store.storage()));
    let file = h.sandbox.file("отчёт.txt", "https://example.com");
    h.fake.state().outputs.push_back(Ok(output(0, &video_info("Клип"), "")));
    let cases = [
        (actions::Kind::Phone, Page::Share),
        (actions::Kind::Qr, Page::Qr),
        (actions::Kind::Rename, Page::Rename),
        (actions::Kind::Hash { algorithm: actions::Algorithm::Sha256, virustotal: String::new() }, Page::Hashes),
        (actions::Kind::Ytdlp(actions::Fetch::default()), Page::Media),
    ];
    for (kind, page) in cases {
        let index = add_tile(&mut h, kind);
        h.pending(|shelf, _, cx| shelf.perform(index, Payload::Paths(vec![file.clone()]), cx));
        assert!(h.read(|shelf| shelf.page() == page && shelf.pending().is_some()), "{page:?} waits on its own page");
        h.settle();
        let ready = h.read(|shelf| {
            shelf.share().is_some()
                || shelf.qr().is_some()
                || shelf.renaming().is_some()
                || shelf.hashes().is_some()
                || shelf.fetching().is_some()
        });
        assert!(ready && h.read(|shelf| shelf.page() == page), "{page:?} fills in place");
        h.act(|shelf, _, cx| shelf.toggle_page(Page::Shelf, cx));
    }
}

#[gpui::test]
fn stop_drops_running_work_its_page_and_its_result(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    let index =
        add_tile(&mut h, actions::Kind::Hash { algorithm: actions::Algorithm::Sha256, virustotal: String::new() });
    let file = h.sandbox.file("a.txt", "abc");
    h.pending(|shelf, _, cx| shelf.perform(index, Payload::Paths(vec![file]), cx));
    assert!(h.read(|shelf| shelf.page() == Page::Hashes));
    h.act(|shelf, _, cx| shelf.stop_jobs(cx));
    assert!(h.read_cx(|shelf, cx| shelf.jobs.read(cx).last().is_none()), "the indicator goes away at once");
    assert!(h.read(|shelf| shelf.page() == Page::Shelf));
    h.settle();
    assert!(h.read(|shelf| shelf.page() == Page::Shelf && shelf.hashes().is_none()));
    assert_eq!((h.toast(), h.alert(), clipboard(&mut h)), (None, None, None), "a stopped action leaves no trace");
}

#[gpui::test]
fn result_does_not_reopen_a_page_the_user_left(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    let file = h.sandbox.file("a.txt", "x");
    let index = add_tile(&mut h, actions::Kind::Phone);
    h.pending(|shelf, _, cx| shelf.perform(index, Payload::Paths(vec![file]), cx));
    h.act(|shelf, _, cx| shelf.toggle_page(Page::Settings, cx));
    h.settle();
    assert!(h.read(|shelf| shelf.page() == Page::Settings && shelf.share().is_none()));
}

#[gpui::test]
fn hashing_failure_leaves_the_placeholder_page(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    let index =
        add_tile(&mut h, actions::Kind::Hash { algorithm: actions::Algorithm::Sha256, virustotal: String::new() });
    let file = h.sandbox.file("a.txt", "abc");
    h.pending(|shelf, _, cx| shelf.perform(index, Payload::Paths(vec![file.clone()]), cx));
    std::fs::remove_file(&file).expect("remove");
    h.settle();
    assert!(h.read(|shelf| shelf.page() == Page::Shelf));
    assert!(h.alert().is_some());
}

#[gpui::test]
fn hash_page_opens_before_virustotal_answers(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    let index =
        add_tile(&mut h, actions::Kind::Hash { algorithm: actions::Algorithm::Sha256, virustotal: "key".into() });
    let file = h.sandbox.file("a.txt", "abc");
    let stats = r#"{"data":{"attributes":{"last_analysis_stats":{"malicious":0,"undetected":60}}}}"#;
    h.fake.state().outputs.push_back(Ok(crate::testing::output(0, &format!("{stats}\n200"), "")));
    h.pending(|shelf, _, cx| shelf.perform(index, Payload::Paths(vec![file]), cx));
    h.fake.run_jobs();
    h.cx.run_until_parked();
    let verdict = |h: &mut Harness| h.read(|shelf| shelf.hashes().and_then(|digests| digests[0].verdict.clone()));
    assert!(h.read(|shelf| shelf.page() == Page::Hashes));
    assert_eq!(verdict(&mut h), Some(actions::Verdict::Checking));
    assert_eq!(h.toast(), Some(fill(tr().hash_copied, "SHA-256")));
    h.settle();
    assert_eq!(verdict(&mut h), Some(actions::Verdict::Clean { total: 60 }));
    assert!(h.toast().is_some(), "the verdict lands on the page, the toast stays");
}

#[gpui::test]
fn late_virustotal_answer_does_not_land_on_another_files_hashes(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    let index =
        add_tile(&mut h, actions::Kind::Hash { algorithm: actions::Algorithm::Sha256, virustotal: "key".into() });
    let file = h.sandbox.file("a.txt", "abc");
    h.fake.state().outputs.push_back(Ok(crate::testing::output(0, "{}\n404", "")));
    h.pending(|shelf, _, cx| shelf.perform(index, Payload::Paths(vec![file]), cx));
    h.fake.run_jobs();
    h.cx.run_until_parked();
    let other = actions::Digest { path: h.sandbox.join("b.txt"), sha256: "b".into(), md5: "b".into(), verdict: None };
    h.pending(|shelf, _, cx| shelf.open_hashes(vec![other], None, cx));
    h.settle();
    assert_eq!(h.read(|shelf| shelf.hashes().map(|digests| digests[0].verdict.clone())), Some(None));
}

#[gpui::test]
fn stopping_the_virustotal_check_marks_the_files_as_stopped(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    let index =
        add_tile(&mut h, actions::Kind::Hash { algorithm: actions::Algorithm::Sha256, virustotal: "key".into() });
    let file = h.sandbox.file("a.txt", "abc");
    h.pending(|shelf, _, cx| shelf.perform(index, Payload::Paths(vec![file]), cx));
    h.fake.run_jobs();
    h.cx.run_until_parked();
    assert_eq!(h.running(), 1, "the VirusTotal check runs");
    h.act(|shelf, _, cx| shelf.stop_jobs(cx));
    let verdict = h.read(|shelf| shelf.hashes().and_then(|digests| digests[0].verdict.clone()));
    assert_eq!(verdict, Some(actions::Verdict::Failed(tr().stopped.into())), "no file stays in the checking state");
    assert!(h.alert().is_none());
}

#[gpui::test]
fn hash_tile_shows_both_hashes_and_leads_to_the_key(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    let index = add_tile(&mut h, actions::Kind::Hash { algorithm: actions::Algorithm::Md5, virustotal: String::new() });
    let file = h.sandbox.file("setup.exe", "abc");
    h.act(|shelf, _, cx| shelf.perform(index, Payload::Paths(vec![file.clone()]), cx));
    h.render();
    assert!(h.read(|shelf| shelf.page() == Page::Hashes));
    assert_eq!(clipboard(&mut h).as_deref(), Some("900150983cd24fb0d6963f7d28e17f72"));
    let shown = h.read(|shelf| shelf.hashes().map(<[_]>::to_vec)).expect("hashes");
    assert_eq!(shown[0].path, file);
    assert_eq!(shown[0].sha256, "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    h.act(|shelf, _, cx| shelf.copy_text(shown[0].sha256.clone(), "copy-sha256:0".into(), cx));
    assert_eq!(clipboard(&mut h).as_deref(), Some(shown[0].sha256.as_str()));
    h.act(|shelf, window, cx| shelf.hash_settings(window, cx));
    h.render();
    assert!(h.read(|shelf| shelf.page() == Page::Edit) && h.editing() == Some(index));
    assert!(h.read(|shelf| shelf.hashes().is_none()));
}

#[gpui::test]
fn qr_tile_shows_the_code_and_keeps_a_picture(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    let index = add_tile(&mut h, actions::Kind::Qr);
    h.act(|shelf, _, cx| shelf.perform(index, Payload::Text("https://example.com/page".into()), cx));
    h.render();
    assert!(h.read(|shelf| shelf.page() == Page::Qr));
    assert_eq!(h.read(|shelf| shelf.qr().map(|qr| qr.text.clone())).as_deref(), Some("https://example.com/page"));
    h.fake.take_calls();
    h.act(|shelf, _, cx| shelf.copy_qr(cx));
    assert!(h.fake.calls().iter().any(|call| matches!(call, Call::Copy(ids) if ids.len() == 1)));
    h.act(|shelf, _, cx| shelf.keep_qr(cx));
    h.render();
    assert!(h.read(|shelf| shelf.page() == Page::Shelf));
    let item = h.read(|shelf| shelf.store.items()[0].clone());
    assert_eq!(item.title, "QR · https://example.com/page");
    assert!(item.kind == Kind::Image && item.managed);
    let picture = store::open_image(&item.path).expect("png");
    assert_eq!(picture.width(), picture.height());
    assert!(h.saved().items().iter().any(|saved| saved.path == item.path));
}

#[gpui::test]
fn rename_tile_renames_files_and_updates_the_shelf(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    let index = add_tile(&mut h, actions::Kind::Rename);
    let kept = h.sandbox.file("Привет.txt", "1");
    let loose = h.sandbox.file("Мир.txt", "2");
    h.act(|shelf, _, _| shelf.store.add_paths(vec![kept.clone()]));
    h.act(|shelf, _, cx| shelf.perform(index, Payload::Paths(vec![kept.clone(), loose.clone()]), cx));
    h.render();
    assert!(h.read(|shelf| shelf.page() == Page::Rename));
    h.act(|shelf, window, cx| shelf.set_rule(actions::Rule::Latin, window, cx));
    h.act(|shelf, _, cx| shelf.rename_files(cx));
    h.render();
    assert!(!kept.exists() && !loose.exists());
    assert_eq!(std::fs::read_to_string(h.sandbox.join("Privet.txt")).expect("renamed"), "1");
    assert_eq!(std::fs::read_to_string(h.sandbox.join("Mir.txt")).expect("renamed"), "2");
    let item = h.read(|shelf| shelf.store.items()[0].clone());
    assert_eq!((item.title.as_str(), item.path.clone()), ("Privet.txt", h.sandbox.join("Privet.txt")));
    assert_eq!(h.saved().items()[0].path, h.sandbox.join("Privet.txt"));
    assert!(h.read(|shelf| shelf.page() == Page::Shelf && shelf.renaming().is_none()));
    assert_eq!(h.toast(), Some(fill(tr().renamed, 2)));
}

#[gpui::test]
fn custom_name_renames_on_enter_only_when_given(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    let index = add_tile(&mut h, actions::Kind::Rename);
    let first = h.sandbox.file("b.jpg", "1");
    let second = h.sandbox.file("a.jpg", "2");
    h.act(|shelf, _, cx| shelf.perform(index, Payload::Paths(vec![first, second]), cx));
    h.render();
    h.act(|shelf, window, cx| shelf.set_rule(actions::Rule::Name, window, cx));
    h.render();
    h.keys("enter");
    assert!(h.read(|shelf| shelf.page() == Page::Rename));
    assert!(h.sandbox.join("a.jpg").exists());
    h.cx.simulate_input("Отпуск");
    h.keys("enter");
    h.render();
    assert_eq!(std::fs::read_to_string(h.sandbox.join("Отпуск_01.jpg")).expect("first"), "2");
    assert_eq!(std::fs::read_to_string(h.sandbox.join("Отпуск_02.jpg")).expect("second"), "1");
    assert!(h.read(|shelf| shelf.page() == Page::Shelf));
}

#[gpui::test]
fn failed_rename_stays_on_the_page_and_alerts(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    let index = add_tile(&mut h, actions::Kind::Rename);
    let file = h.sandbox.file("Ёлка.txt", "1");
    h.act(|shelf, _, cx| shelf.perform(index, Payload::Paths(vec![file.clone()]), cx));
    h.act(|shelf, window, cx| shelf.set_rule(actions::Rule::Latin, window, cx));
    std::fs::remove_file(&file).expect("gone");
    h.act(|shelf, _, cx| shelf.rename_files(cx));
    assert!(h.alert().is_some());
    assert!(h.read(|shelf| shelf.page() == Page::Rename));
}

#[gpui::test]
fn page_search_keeps_the_shelf_search_and_resets_when_leaving_the_page(cx: &mut TestAppContext) {
    let page_search = |h: &mut Harness| h.read_cx(|shelf, cx| shelf.find.input().read(cx).text().to_string());
    let mut h = open_with(cx, true, notes(&["alpha", "beta"]));
    h.keys("/");
    h.cx.simulate_input("alpha");
    h.settle();
    h.act(|shelf, window, cx| {
        window.focus(&shelf.focus);
        shelf.toggle_page(Page::Settings, cx);
    });
    h.keys("/");
    h.cx.simulate_input("тема");
    assert_eq!(page_search(&mut h), "тема");
    h.act(|shelf, _, cx| shelf.toggle_page(Page::Settings, cx));
    assert_eq!(h.titles(), ["alpha"], "the shelf search survives a visit to the settings");
    assert_eq!(page_search(&mut h), "");
    let tile = h.tile(0);
    h.act(|shelf, window, cx| {
        window.focus(&shelf.focus);
        shelf.toggle_page(Page::Actions, cx);
    });
    h.keys("/");
    h.cx.simulate_input("zip");
    h.act(|shelf, window, cx| shelf.open_editor(tile, None, window, cx));
    h.act(|shelf, window, cx| {
        window.focus(&shelf.focus);
        shelf.escape(cx);
    });
    assert!(h.read(|shelf| shelf.page() == Page::Actions));
    assert_eq!(page_search(&mut h), "zip", "a tile editor is part of the tiles page");
    h.act(|shelf, _, cx| shelf.toggle_page(Page::Settings, cx));
    assert_eq!(page_search(&mut h), "");
}

#[gpui::test]
fn page_search_scrolls_the_first_match_into_view(cx: &mut TestAppContext) {
    let in_view = |h: &mut Harness, mark: &str| {
        h.cx.update(|window, _| window.refresh());
        h.cx.run_until_parked();
        let view = h.read(|shelf| shelf.scroll(Page::Settings).bounds());
        let bounds = h.cx.debug_bounds(interned(mark)).expect("the setting is rendered");
        bounds.top() >= view.top() && bounds.bottom() <= view.bottom()
    };
    let mut h = open(cx, true);
    h.cx.simulate_resize(size(px(NARROW_SIZE.0), px(NARROW_SIZE.1)));
    h.act(|shelf, window, cx| {
        window.focus(&shelf.focus);
        shelf.toggle_page(Page::Settings, cx);
    });
    let last = "switch:separate-notes";
    assert!(!in_view(&mut h, last), "the setting starts below the fold");
    h.keys("/");
    h.cx.simulate_input(tr().separate_notes);
    assert!(in_view(&mut h, last), "the only match is scrolled into view");
    h.keys("escape");
    h.cx.simulate_input(tr().theme);
    assert!(in_view(&mut h, "segmented:theme"), "a new query brings its first match back up");
}
