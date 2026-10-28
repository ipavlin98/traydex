use super::*;
use sha2::{Digest as _, Sha256};

const NEWER: &str = "99.0.0";
const DAY: Duration = Duration::from_secs(24 * 60 * 60);

fn releases() -> String {
    let base = format!("https://github.com/ipavlin98/traydex/releases/download/v{NEWER}");
    let mut names = vec!["SHA256SUMS.txt".to_string()];
    for arch in ["x64", "x86", "arm64"] {
        names.push(format!("Traydex-{NEWER}-{arch}-setup.exe"));
        names.push(format!("Traydex-{NEWER}-{arch}-portable.zip"));
    }
    let assets: Vec<serde_json::Value> = names
        .iter()
        .map(|name| serde_json::json!({"name": name, "browser_download_url": format!("{base}/{name}")}))
        .collect();
    serde_json::json!([{"tag_name": format!("v{NEWER}"), "draft": false, "assets": assets}]).to_string()
}

fn answer(h: &Harness, body: &str) {
    h.fake.state().outputs.push_back(Ok(output(0, body, "")));
}

fn offered(h: &mut Harness) -> Option<String> {
    h.read(|shelf| match &shelf.confirming {
        Some(Confirm::Update(release)) => Some(release.version.to_string()),
        _ => None,
    })
}

fn serve_installer(h: &Harness, installer: &'static [u8], published: &'static [u8]) {
    let sums: String = ["x64", "x86", "arm64"]
        .iter()
        .map(|arch| format!("{:x}  Traydex-{NEWER}-{arch}-setup.exe\n", Sha256::digest(published)))
        .collect();
    h.fake.state().downloader = Some(Box::new(move |url, target, _| {
        let body = if url.ends_with("SHA256SUMS.txt") { sums.clone().into_bytes() } else { installer.to_vec() };
        Ok(std::fs::write(target, body)?)
    }));
}

fn launched(h: &Harness) -> Vec<Invocation> {
    h.fake
        .calls()
        .into_iter()
        .filter_map(|call| if let Call::Launch(invocation) = call { Some(invocation) } else { None })
        .collect()
}

#[gpui::test]
fn manual_check_says_when_nothing_is_newer(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    answer(&h, "[]");
    h.act(|shelf, _, cx| shelf.check_updates(cx));
    assert_eq!(h.toast().as_deref(), Some(tr().update_none));
    assert_eq!(offered(&mut h), None);
    h.fake.state().outputs.push_back(Ok(output(6, "", "curl: (6) Could not resolve host")));
    h.act(|shelf, _, cx| shelf.check_updates(cx));
    assert_eq!(h.alert(), Some(fill(tr().update_check_failed, tr().no_connection)));
}

#[gpui::test]
fn accepted_update_starts_the_verified_installer(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    answer(&h, &releases());
    h.act(|shelf, _, cx| shelf.check_updates(cx));
    assert_eq!(offered(&mut h).as_deref(), Some(NEWER));
    serve_installer(&h, b"installer", b"installer");
    h.act(|shelf, window, cx| shelf.confirm(window, cx));
    let [setup] = launched(&h).try_into().expect("the installer is started once");
    assert_eq!(std::fs::read(&setup.program).expect("downloaded installer"), b"installer");
    assert!(setup.args().contains(&"/SILENT".to_string()), "{:?}", setup.args());
    assert_eq!(h.alert(), None);
}

#[gpui::test]
fn damaged_download_is_never_started(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    answer(&h, &releases());
    h.act(|shelf, _, cx| shelf.check_updates(cx));
    serve_installer(&h, b"tampered", b"installer");
    h.act(|shelf, window, cx| shelf.confirm(window, cx));
    assert!(launched(&h).is_empty());
    assert_eq!(h.alert(), Some(fill(tr().update_failed, tr().update_corrupted)));
}

#[gpui::test]
fn declined_update_is_not_offered_again_automatically(cx: &mut TestAppContext) {
    let mut h = open(cx, false);
    answer(&h, &releases());
    h.elapse(Duration::from_secs(60));
    assert_eq!(offered(&mut h).as_deref(), Some(NEWER));
    assert!(h.fake.calls().contains(&Call::Notify(fill(tr().update_notify, NEWER))));
    h.act(|shelf, _, cx| shelf.cancel_confirm(cx));
    answer(&h, &releases());
    h.elapse(DAY);
    assert_eq!(offered(&mut h), None);
    answer(&h, &releases());
    h.act(|shelf, _, cx| shelf.check_updates(cx));
    assert_eq!(offered(&mut h).as_deref(), Some(NEWER), "asking by hand always shows it");
}

#[gpui::test]
fn automatic_checks_stop_when_turned_off(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    h.act(|shelf, _, cx| shelf.toggle_check_updates(cx));
    assert!(!h.saved().settings().check_updates);
    answer(&h, &releases());
    h.elapse(DAY * 2);
    assert!(!h.fake.calls().iter().any(|call| matches!(call, Call::Output(_))));
    assert_eq!(offered(&mut h), None);
}

#[gpui::test]
fn the_version_in_settings_opens_the_releases_page(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    h.act(|shelf, _, cx| shelf.toggle_page(Page::Settings, cx));
    h.settle();
    h.act(|shelf, window, _| {
        let scroll = shelf.scroll(Page::Settings);
        scroll.set_offset(point(px(0.), -scroll.max_offset().height));
        window.refresh();
    });
    h.settle();
    click(&mut h, "releases");
    assert!(h.fake.calls().contains(&Call::ShellOpen("https://github.com/ipavlin98/traydex/releases".into())));
}
