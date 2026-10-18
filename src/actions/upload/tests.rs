use super::*;
use crate::testing::Sandbox;

#[test]
fn base64_matches_rfc_vectors() {
    let vectors = [
        ("", ""),
        ("f", "Zg=="),
        ("fo", "Zm8="),
        ("foo", "Zm9v"),
        ("foob", "Zm9vYg=="),
        ("fooba", "Zm9vYmE="),
        ("foobar", "Zm9vYmFy"),
    ];
    for (plain, encoded) in vectors {
        assert_eq!(base64(plain.as_bytes()), encoded, "{plain}");
    }
    assert_eq!(base64(&[0xff, 0xfe, 0xfd]), "//79");
    assert_eq!(base64(&[0, 0, 0, 0]), "AAAAAA==");
}

#[test]
fn ascii_name_replaces_unsafe_characters() {
    assert_eq!(ascii_name(Path::new(r"C:\docs\My Photo (1).png")), "My_Photo__1_.png");
    assert_eq!(ascii_name(Path::new(r"C:\docs\отчёт.pdf")), "_____.pdf");
    assert_eq!(ascii_name(Path::new("a;b\"c.txt")), "a_b_c.txt");
    assert_eq!(ascii_name(Path::new("safe-name_1.tar.gz")), "safe-name_1.tar.gz");
}

#[test]
fn mime_types_follow_extension() {
    let cases = [
        ("a.PNG", "image/png"),
        ("a.jpg", "image/jpeg"),
        ("a.jpeg", "image/jpeg"),
        ("a.gif", "image/gif"),
        ("a.webp", "image/webp"),
        ("a.bmp", "image/bmp"),
        ("a.mp4", "video/mp4"),
        ("a.pdf", "application/pdf"),
        ("a.txt", "text/plain; charset=utf-8"),
        ("a.bin", "application/octet-stream"),
        ("README", "application/octet-stream"),
    ];
    for (name, mime_type) in cases {
        assert_eq!(mime(Path::new(name)), mime_type, "{name}");
    }
}

#[test]
fn http_status_check() {
    assert!(check("200", "").is_ok());
    assert!(check("204", "").is_ok());
    assert_eq!(check("401", "x").expect_err("401").to_string(), tr().key_rejected);
    assert_eq!(check("403", "x").expect_err("403").to_string(), tr().key_rejected);
    assert_eq!(check("404", "missing").expect_err("404").to_string(), "HTTP 404: missing");
    assert_eq!(check("", "").expect_err("empty").to_string(), "HTTP : ");
}

#[test]
fn curl_failures_are_explained() {
    assert_eq!(curl_failure(Some(6), b""), tr().no_connection);
    assert_eq!(curl_failure(Some(7), b""), tr().no_connection);
    assert_eq!(curl_failure(Some(28), b""), tr().timed_out);
    assert_eq!(
        curl_failure(Some(22), b"curl: (22) The requested URL returned error: 404\n"),
        "curl: (22) The requested URL returned error: 404"
    );
    assert_eq!(curl_failure(None, b"killed"), "killed");
}

#[test]
fn curl_comes_from_windows_system_folder() {
    let path = curl_exe();
    assert!(path.ends_with(r"System32\curl.exe"), "{}", path.display());
}

#[test]
fn publish_skips_unsuitable_files_and_collects_links() {
    let sandbox = Sandbox::new();
    let good = sandbox.file("a.png", "x");
    let other = sandbox.file("b.txt", "x");
    let folder = sandbox.folder("c");
    let accepts = Accepts { kind: crate::store::is_image, limit: 10, wrong: "images only" };
    let outcome = publish(&[good.clone(), other, folder], &Progress::default(), accepts, |file| {
        Ok(format!("https://x/{}", base(file)))
    })
    .expect("publish");
    assert_eq!(outcome.clipboard.as_deref(), Some("https://x/a.png"));
    assert_eq!(outcome.message, format!("{} · {}", tr().link_copied, fill(tr().skipped, 2)));
    let none = Accepts { kind: crate::store::is_image, limit: 10, wrong: "images only" };
    let failure =
        publish(&[sandbox.join("missing.png")], &Progress::default(), none, |_| panic!("nothing must be uploaded"));
    assert_eq!(failure.err().expect("error").to_string(), "images only");
}

#[test]
fn publish_returns_first_failure_when_nothing_was_sent() {
    let sandbox = Sandbox::new();
    let files = [sandbox.file("a.txt", "x"), sandbox.file("b.txt", "x")];
    let failure = publish(&files, &Progress::default(), Accepts::files(), |file| Err(anyhow!("failed {}", base(file))));
    assert_eq!(failure.err().expect("error").to_string(), "failed a.txt");
}

#[test]
fn publish_reports_progress_and_joins_links() {
    let sandbox = Sandbox::new();
    let files: Vec<PathBuf> = (0..3).map(|n| sandbox.file(&format!("{n}.txt"), "x")).collect();
    let progress = Progress::default();
    let outcome = publish(&files, &progress, Accepts::files(), |file| Ok(base(file))).expect("publish");
    assert_eq!(outcome.clipboard.as_deref(), Some("0.txt\r\n1.txt\r\n2.txt"));
    assert!((progress.get().fraction.expect("fraction") - 2. / 3.).abs() < 1e-6);
}
