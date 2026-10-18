use super::*;

#[gpui::test]
fn job_progress_redraws_only_when_the_shown_percent_changes(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    let progress = h.pending(|shelf, _, cx| {
        shelf.jobs.update(cx, |jobs, cx| jobs.start(String::new(), None, JobKind::Other, cx).1)
    });
    let jobs = h.read(|shelf| shelf.jobs.clone());
    let redraws = std::rc::Rc::new(std::cell::Cell::new(0));
    let counter = redraws.clone();
    let _watch = h.cx.update(|_, cx| cx.observe(&jobs, move |_, _| counter.set(counter.get() + 1)));
    h.elapse(Duration::from_secs(1));
    assert_eq!(redraws.get(), 0, "a job without new progress keeps the window as it is");
    progress.update(|stage| stage.fraction = Some(0.5));
    h.elapse(Duration::from_secs(1));
    assert_eq!(redraws.get(), 1);
}

#[gpui::test]
fn parallel_jobs_show_progress_only_when_every_job_knows_it(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    let mut start = || {
        h.pending(|shelf, _, cx| {
            shelf.jobs.update(cx, |jobs, cx| jobs.start(String::new(), None, JobKind::Other, cx).1)
        })
    };
    let (first, second) = (start(), start());
    first.update(|stage| stage.fraction = Some(0.25));
    assert_eq!(
        h.read_cx(|shelf, cx| shelf.jobs.read(cx).fraction()),
        None,
        "a job with unknown progress keeps the spinner"
    );
    second.update(|stage| stage.fraction = Some(0.75));
    assert_eq!(h.read_cx(|shelf, cx| shelf.jobs.read(cx).fraction()), Some(0.5));
}

#[gpui::test]
fn progress_that_would_show_100_percent_spins_instead(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    let progress = h.pending(|shelf, _, cx| {
        shelf.jobs.update(cx, |jobs, cx| jobs.start(String::new(), None, JobKind::Other, cx).1)
    });
    let shown = |h: &mut Harness| {
        h.read_cx(|shelf, cx| {
            let jobs = shelf.jobs.read(cx);
            (jobs.last().and_then(|job| job.percent()), jobs.fraction())
        })
    };
    progress.update(|stage| stage.fraction = Some(0.99));
    assert_eq!(shown(&mut h), (Some("99%".to_string()), Some(0.99)));
    for almost in [0.996, 1., 1.5] {
        progress.update(|stage| stage.fraction = Some(almost));
        assert_eq!(shown(&mut h), (None, None), "{almost} is shown as a spinner, not a frozen 100%");
    }
}

#[gpui::test]
fn video_link_offers_choices_and_downloads_onto_the_shelf(cx: &mut TestAppContext) {
    let mut h = open_with(cx, true, |_, store, _| plant_tools(store.storage()));
    let index = add_tile(&mut h, actions::Kind::Ytdlp(actions::Fetch::default()));
    assert!(h.read(|shelf| shelf.ready(shelf.tiles.get(index).expect("tile"))), "all three tools are found");
    h.fake.state().outputs.push_back(Ok(output(0, &video_info("Клип о котах"), "")));
    h.pending(|shelf, _, cx| shelf.perform(index, Payload::Text("https://youtu.be/abc".into()), cx));
    assert!(h.read(|shelf| shelf.page() == Page::Media && shelf.pending().is_some()), "the choices wait in place");
    h.settle();
    assert_eq!(h.read(|shelf| shelf.fetching().map(|f| f.media.title.clone())), Some("Клип о котах".into()));
    h.act(|shelf, _, cx| {
        shelf.set_media_mode(actions::Mode::Audio, cx);
        shelf.set_quality(actions::Quality::Height(2160), cx);
        shelf.set_quality(actions::Quality::Height(720), cx);
        shelf.toggle_extra(actions::Extra::Subtitles, cx);
        shelf.toggle_extra(actions::Extra::Playlist, cx);
    });
    let fetch = h.read(|shelf| shelf.fetching().expect("page").fetch);
    assert_eq!(fetch.quality, actions::Quality::Height(720), "a resolution the video lacks can't be picked");
    assert!(fetch.subtitles && !fetch.playlist, "the link has no playlist to switch on");
    h.fake.state().streamer = Some(Box::new(|invocation: &Invocation| {
        let args = invocation.args();
        let folder = args.iter().position(|arg| arg == "-P").and_then(|at| args.get(at + 1)).expect("folder");
        std::fs::write(Path::new(folder).join("Клип о котах.mp3"), "mp3").expect("file");
        Ok(output(0, "", ""))
    }));
    h.pending(|shelf, _, cx| shelf.download_media(cx));
    assert!(h.read(|shelf| shelf.page() == Page::Shelf), "the page closes while the download runs");
    assert!(h.read_cx(|shelf, cx| shelf.jobs.read(cx).last().is_some()));
    h.settle();
    let titles: Vec<String> = h.read(|shelf| shelf.store.items().iter().map(|item| item.title.clone()).collect());
    assert_eq!(titles, ["Клип о котах.mp3"]);
    assert_eq!(h.toast(), Some(fill(tr().converted, 1)));
    let remembered = saved_tiles(&h).into_iter().find_map(|action| match action.kind {
        actions::Kind::Ytdlp(fetch) => Some(fetch),
        _ => None,
    });
    assert_eq!(
        remembered,
        Some(actions::Fetch { playlist: false, ..fetch }),
        "the next link starts from these choices"
    );
}

#[gpui::test]
fn video_tile_needs_every_tool_and_offers_each_download(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    h.act(|shelf, window, cx| {
        shelf.toggle_action(Action::new(actions::Kind::Ytdlp(actions::Fetch::default())), None, window, cx)
    });
    assert!(h.read(|shelf| shelf.page() == Page::Edit), "a tile without its tools opens for setup");
    h.cx.update(|window, _| window.refresh());
    h.cx.run_until_parked();
    for tool in [Tool::YtDlp, Tool::Ffmpeg, Tool::Deno] {
        assert!(h.cx.debug_bounds(interned(&format!("tool:{}", tool.name()))).is_some(), "{} is offered", tool.name());
    }
}

#[gpui::test]
fn a_tool_button_stays_beside_its_status_and_moves_under_the_text_when_it_does_not_fit(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    h.act(|shelf, window, cx| {
        shelf.toggle_action(Action::new(actions::Kind::Ytdlp(actions::Fetch::default())), None, window, cx)
    });
    let row = format!("tool:{}", Tool::YtDlp.name());
    let (text, action) = (interned(&format!("{row}:text")), interned(&format!("{row}:action")));
    let laid_out = |h: &mut Harness, width: f32| {
        h.cx.simulate_resize(size(px(width), px(900.)));
        h.settle();
        h.render();
        (h.cx.debug_bounds(text).expect("status text"), h.cx.debug_bounds(action).expect("download button"))
    };
    let (status, button) = laid_out(&mut h, 1200.);
    assert!(button.left() >= status.right(), "a wide window keeps the button beside the text: {status:?} {button:?}");
    assert!(
        button.top() < status.bottom() && status.top() < button.bottom(),
        "on the same line: {status:?} {button:?}"
    );
    let (status, button) = laid_out(&mut h, 360.);
    assert!(button.top() >= status.bottom(), "a narrow window moves the button under the text: {status:?} {button:?}");
    assert!(
        (f32::from(button.left()) - f32::from(status.left())).abs() <= 0.5,
        "aligned with the text: {status:?} {button:?}"
    );
}

#[gpui::test]
fn job_tooltip_keeps_a_long_subject_inside_its_box(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    let subject = "МЕНЯЮТ МУЗЫКУ! Apple выпустила iOS 27.2 Beta 4 для iPhone! Стоит ли Ставить? Что Нового?";
    h.pending(|shelf, _, cx| {
        shelf.jobs.update(cx, |jobs, cx| {
            jobs.start("Скачивание… · Скачать видео".into(), Some(subject.into()), JobKind::Other, cx)
        })
    });
    h.cx.update(|window, _| window.refresh());
    h.cx.run_until_parked();
    let anchor = h.cx.debug_bounds("jobs").expect("the jobs indicator");
    h.cx.simulate_mouse_move(anchor.center(), None, Modifiers::default());
    h.cx.executor().advance_clock(Duration::from_millis(600));
    h.cx.run_until_parked();
    let frame = h.cx.debug_bounds("hint").expect("the jobs tooltip");
    let rows: Vec<_> = (0..).map_while(|n| h.cx.debug_bounds(interned(&format!("hint:1:{n}")))).collect();
    assert!(rows.len() >= 2, "the subject wraps: {rows:?}");
    for row in rows {
        let inside = row.left() >= frame.left() && row.right() <= frame.right() && row.bottom() <= frame.bottom();
        assert!(inside, "{row:?} leaves the tooltip {frame:?}");
    }
}
