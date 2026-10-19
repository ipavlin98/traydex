use super::*;
use crate::testing::{Fake, Sandbox, archive};
use serde_json::{Value, json};

const X64_SETUP: Install = Install { arch: Arch::X64, form: Form::Setup, channel: Channel::Preview };

fn version(text: &str) -> Version {
    Version::parse(text).expect("valid version")
}

fn url(tag: &str, name: &str) -> String {
    format!("https://github.com/ipavlin98/traydex/releases/download/{tag}/{name}")
}

fn listed(tag: &str, draft: bool, names: &[String]) -> Value {
    let assets: Vec<Value> =
        names.iter().map(|name| json!({"name": name, "browser_download_url": url(tag, name)})).collect();
    json!({"tag_name": tag, "draft": draft, "prerelease": tag.contains('-'), "assets": assets})
}

fn full(tag: &str) -> Value {
    let version = tag.trim_start_matches('v');
    let mut names = vec![SUMS.to_string()];
    for arch in ["x64", "x86", "arm64"] {
        names.push(format!("Traydex-{version}-{arch}-setup.exe"));
        names.push(format!("Traydex-{version}-{arch}-portable.zip"));
    }
    listed(tag, false, &names)
}

fn found(releases: &[Value], install: Install, current: &str) -> Option<Release> {
    newest(&Value::Array(releases.to_vec()).to_string(), install, &version(current)).expect("release list")
}

#[test]
fn versions_follow_semver_precedence() {
    let ascending = [
        "0.1.0-alpha",
        "0.1.0-alpha.2",
        "0.1.0-alpha.10",
        "0.1.0-alpha.beta",
        "0.1.0-beta",
        "0.1.0-rc.1",
        "0.1.0",
        "0.1.1",
        "0.2.0-alpha",
        "0.10.0",
        "1.0.0",
    ];
    for pair in ascending.windows(2) {
        assert!(version(pair[0]) < version(pair[1]), "{} < {}", pair[0], pair[1]);
    }
    assert_eq!(version("v0.2.0"), version("0.2.0"));
    assert_eq!(version("0.2.0+build.5"), version("0.2.0"));
    assert_eq!(version("0.2.0-alpha.2").to_string(), "0.2.0-alpha.2");
    for broken in ["", "1.2", "1.2.3.4", "1.x.3", "1.2.3-", "1.2.3-a..b", "1.2.3-a/b", "-1.2.3"] {
        assert_eq!(Version::parse(broken), None, "{broken:?}");
    }
}

#[test]
fn newest_matching_release_is_offered() {
    let releases = [
        full("v0.1.0-alpha"),
        full("v0.2.0-alpha"),
        listed("v0.3.0-alpha", true, &[SUMS.into(), "Traydex-0.3.0-alpha-x64-setup.exe".into()]),
        listed("v0.4.0-alpha", false, &[SUMS.into(), "Traydex-0.4.0-alpha-arm64-setup.exe".into()]),
        listed("v0.5.0-alpha", false, &["Traydex-0.5.0-alpha-x64-setup.exe".into()]),
    ];
    let release = found(&releases, X64_SETUP, "0.1.0-alpha").expect("an update");
    assert_eq!(release.version, version("0.2.0-alpha"));
    assert_eq!(release.file.name, "Traydex-0.2.0-alpha-x64-setup.exe");
    assert_eq!(release.file.browser_download_url, url("v0.2.0-alpha", "Traydex-0.2.0-alpha-x64-setup.exe"));
    assert_eq!(release.sums, url("v0.2.0-alpha", SUMS));
}

#[test]
fn portable_install_gets_the_zip_of_its_architecture() {
    let install = Install { arch: Arch::Arm64, form: Form::Portable, channel: Channel::Preview };
    let release = found(&[full("v0.2.0-alpha")], install, "0.1.0-alpha").expect("an update");
    assert_eq!(release.form, Form::Portable);
    assert_eq!(release.file.browser_download_url, url("v0.2.0-alpha", "Traydex-0.2.0-alpha-arm64-portable.zip"));
    let x86 = Install { arch: Arch::X86, form: Form::Setup, channel: Channel::Preview };
    let release = found(&[full("v0.2.0-alpha")], x86, "0.1.0-alpha").expect("an update");
    assert_eq!(release.file.browser_download_url, url("v0.2.0-alpha", "Traydex-0.2.0-alpha-x86-setup.exe"));
}

#[test]
fn stable_install_ignores_previews() {
    let stable = Install { channel: Channel::Stable, ..X64_SETUP };
    let releases = [full("v1.1.0-beta"), full("v1.0.1"), full("v1.0.0")];
    assert_eq!(found(&releases, stable, "1.0.0").map(|release| release.version), Some(version("1.0.1")));
    assert_eq!(found(&[full("v1.1.0-beta")], stable, "1.0.0"), None);
}

#[test]
fn preview_install_moves_to_a_newer_stable_release() {
    let releases = [full("v0.9.1-alpha"), full("v1.0.0"), full("v1.0.0-rc.2")];
    assert_eq!(found(&releases, X64_SETUP, "0.9.1-alpha").map(|release| release.version), Some(version("1.0.0")));
}

#[test]
fn nothing_is_offered_without_a_newer_release() {
    let releases = [full("v0.2.0-alpha"), full("v0.1.0")];
    assert_eq!(found(&releases, X64_SETUP, "0.2.0-alpha"), None);
    assert_eq!(found(&[], X64_SETUP, "0.2.0-alpha"), None);
    assert!(newest("{\"message\": \"API rate limit exceeded\"}", X64_SETUP, &version("0.1.0")).is_err());
}

fn sandbox_release(form: Form, name: &str) -> Release {
    Release {
        version: version("9.0.0"),
        form,
        file: Asset { name: name.into(), browser_download_url: "https://example.test/file".into() },
        sums: "https://example.test/sums".into(),
    }
}

fn serve(fake: &Fake, file: Vec<u8>, sums: String) {
    fake.state().downloader = Some(Box::new(move |url, target, _| {
        let body = if url.ends_with("sums") { sums.clone().into_bytes() } else { file.clone() };
        Ok(fs::write(target, body)?)
    }));
}

fn hash(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}

#[test]
fn verified_installer_is_started_silently() {
    let sandbox = Sandbox::new();
    let fake = Fake::new(&sandbox);
    let name = "Traydex-9.0.0-x64-setup.exe";
    serve(
        &fake,
        b"setup".to_vec(),
        format!("{}  other.zip\n{}  {name}\n", hash(b"zip"), hash(b"setup").to_uppercase()),
    );
    let command = prepare(&sandbox_release(Form::Setup, name), &fake, &Progress::default()).expect("prepared");
    let program = PathBuf::from(command.get_program());
    assert_eq!(fs::read(&program).expect("installer"), b"setup");
    let args: Vec<String> = command.get_args().map(|arg| arg.to_string_lossy().into_owned()).collect();
    for flag in ["/SILENT", "/SUPPRESSMSGBOXES", "/relaunch=1"] {
        assert!(args.iter().any(|arg| arg == flag), "{flag} in {args:?}");
    }
}

#[test]
fn download_with_a_wrong_checksum_is_refused() {
    let sandbox = Sandbox::new();
    let fake = Fake::new(&sandbox);
    let name = "Traydex-9.0.0-x64-setup.exe";
    serve(&fake, b"tampered".to_vec(), format!("{}  {name}\n", hash(b"setup")));
    let error = prepare(&sandbox_release(Form::Setup, name), &fake, &Progress::default()).expect_err("refused");
    assert_eq!(error.to_string(), tr().update_corrupted);
    serve(&fake, b"setup".to_vec(), format!("{}  another-file.exe\n", hash(b"setup")));
    let error = prepare(&sandbox_release(Form::Setup, name), &fake, &Progress::default()).expect_err("refused");
    assert_eq!(error.to_string(), tr().update_corrupted);
}

#[test]
fn portable_update_replaces_the_app_and_keeps_the_marker() {
    let sandbox = Sandbox::new();
    let exe = sandbox.file("Мой Traydex/Traydex (2).exe", "old app");
    sandbox.file("Мой Traydex/portable.txt", "mine");
    sandbox.file("Мой Traydex/LICENSE.txt", "old license");
    sandbox.file("Мой Traydex/data/shelf.json", "{}");
    let zip = sandbox.join("update.zip");
    archive(
        &zip,
        &[
            ("Traydex/Traydex.exe", b"new app"),
            ("Traydex/portable.txt", b"theirs"),
            ("Traydex/LICENSE.txt", b"new license"),
            ("Traydex/THIRD-PARTY-NOTICES.txt", b"notices"),
        ],
    );
    replace(&zip, &exe).expect("replaced");
    let folder = sandbox.join("Мой Traydex");
    let read = |name: &str| fs::read_to_string(folder.join(name)).expect(name);
    assert_eq!(read("Traydex (2).exe"), "new app");
    assert_eq!(read("portable.txt"), "mine");
    assert_eq!(read("LICENSE.txt"), "new license");
    assert_eq!(read("THIRD-PARTY-NOTICES.txt"), "notices");
    assert_eq!(read("data/shelf.json"), "{}");
    assert!(!folder.join("Traydex.exe").exists(), "the app keeps its own file name");

    sweep(&sandbox.join("temp"), &exe);
    let mut left: Vec<String> = fs::read_dir(&folder)
        .expect("folder")
        .map(|entry| entry.expect("entry").file_name().to_string_lossy().into_owned())
        .collect();
    left.sort();
    assert_eq!(left, ["LICENSE.txt", "THIRD-PARTY-NOTICES.txt", "Traydex (2).exe", "data", "portable.txt"]);
}

#[test]
fn portable_update_without_the_app_changes_nothing() {
    let sandbox = Sandbox::new();
    let exe = sandbox.file("app/Traydex.exe", "old app");
    sandbox.file("app/LICENSE.txt", "old license");
    let zip = sandbox.join("update.zip");
    archive(&zip, &[("Traydex/LICENSE.txt", b"new license")]);
    let error = replace(&zip, &exe).expect_err("refused");
    assert_eq!(error.to_string(), tr().update_corrupted);
    let folder = sandbox.join("app");
    assert_eq!(fs::read_to_string(&exe).expect("exe"), "old app");
    assert_eq!(fs::read_to_string(folder.join("LICENSE.txt")).expect("license"), "old license");
    assert_eq!(fs::read_dir(&folder).expect("folder").count(), 2, "no leftovers");
}
