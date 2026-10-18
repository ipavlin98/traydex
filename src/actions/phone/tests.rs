use super::*;
use crate::testing::Sandbox;
use std::io::Cursor;

struct Exchange {
    request: Cursor<Vec<u8>>,
    response: Vec<u8>,
}

impl Read for Exchange {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.request.read(buf)
    }
}

impl Write for Exchange {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.response.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct Response {
    status: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Response {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(key, _)| key.eq_ignore_ascii_case(name)).map(|(_, value)| value.as_str())
    }

    fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

fn exchange(shared: &Shared, request: &str) -> Response {
    let mut stream = Exchange { request: Cursor::new(request.as_bytes().to_vec()), response: Vec::new() };
    shared.respond(&mut stream).expect("respond");
    let split = stream.response.windows(4).position(|w| w == b"\r\n\r\n").expect("head");
    let head = String::from_utf8(stream.response[..split].to_vec()).expect("utf8 head");
    let mut lines = head.split("\r\n");
    let status = lines.next().expect("status").trim_start_matches("HTTP/1.1 ").to_string();
    let headers = lines
        .map(|line| {
            let (name, value) = line.split_once(": ").expect("header");
            (name.to_string(), value.to_string())
        })
        .collect();
    let body = stream.response[split + 4..].to_vec();
    Response { status, headers, body }
}

fn get(shared: &Shared, path: &str, extra: &str) -> Response {
    exchange(shared, &format!("GET {path} HTTP/1.1\r\nHost: phone\r\n{extra}\r\n"))
}

#[test]
fn single_file_gets_the_page_with_the_download_button() {
    let sandbox = Sandbox::new();
    let shared = Shared::new(vec![sandbox.file("photo.jpg", "jpeg")]);
    for path in [shared.root(), format!("{}/?from=qr", shared.root())] {
        let page = get(&shared, &path, "");
        assert_eq!(page.header("Content-Type"), Some("text/html; charset=utf-8"), "{path}");
        assert!(page.text().contains(&format!(">{}</button>", tr().phone_download_all)), "{path}");
    }
}

#[test]
fn file_opens_at_its_link_with_its_real_name() {
    let sandbox = Sandbox::new();
    let file = sandbox.file("Отчёт за май.pdf", "%PDF-data");
    let shared = Shared::new(vec![file]);
    let response = get(&shared, &format!("{}/0", shared.root()), "");
    assert_eq!(response.status, "200 OK");
    assert_eq!(response.body, b"%PDF-data");
    assert_eq!(response.header("Content-Type"), Some("application/pdf"));
    assert_eq!(response.header("Content-Length"), Some("9"));
    assert_eq!(
        response.header("Content-Disposition"),
        Some(
            "inline; filename=\"____________.pdf\"; filename*=UTF-8''%D0%9E%D1%82%D1%87%D1%91%D1%82%20%D0%B7%D0%B0%20%D0%BC%D0%B0%D0%B9.pdf"
        )
    );
    assert_eq!(get(&shared, &format!("{}/0/x?from=qr", shared.root()), "").body, b"%PDF-data");
}

#[test]
fn files_phones_cannot_show_are_downloaded() {
    let sandbox = Sandbox::new();
    let shared = Shared::new(vec![sandbox.file("backup.zip", "PK")]);
    let response = get(&shared, &format!("{}/0", shared.root()), "");
    assert_eq!(response.header("Content-Type"), Some("application/octet-stream"));
    assert!(response.header("Content-Disposition").is_some_and(|value| value.starts_with("attachment;")));
}

#[test]
fn foreign_paths_are_not_served() {
    let sandbox = Sandbox::new();
    let shared = Shared::new(vec![sandbox.file("secret.txt", "secret")]);
    let root = shared.root();
    for path in [
        "/",
        "/favicon.ico",
        &format!("{root}x"),
        &format!("{root}/1"),
        &format!("{root}/../secret.txt"),
        &root[..root.len() - 1],
    ] {
        let response = get(&shared, path, "");
        assert_eq!(response.status, NOT_FOUND, "{path}");
        assert!(!response.text().contains("secret.txt") && response.text() != "secret", "{path}");
    }
}

#[test]
fn every_share_gets_its_own_unguessable_link() {
    let sandbox = Sandbox::new();
    let file = sandbox.file("a.txt", "a");
    let first = Shared::new(vec![file.clone()]);
    let second = Shared::new(vec![file]);
    assert_ne!(first.root(), second.root());
    assert_eq!(first.root().len(), 25);
    assert_eq!(get(&second, &first.root(), "").status, NOT_FOUND);
}

#[test]
fn several_files_get_an_index_with_escaped_names() {
    let sandbox = Sandbox::new();
    let first = sandbox.file("Tom &amp; Jerry.txt", "first");
    let second = sandbox.file("фото 1.png", "second");
    let shared = Shared::new(vec![first, second]);
    let root = shared.root();
    let index = get(&shared, &format!("{root}/"), "");
    assert_eq!(index.header("Content-Type"), Some("text/html; charset=utf-8"));
    let page = index.text();
    assert!(page.contains(">Tom &amp;amp; Jerry.txt<"));
    assert!(page.contains(&format!("href=\"{root}/0/Tom%20%26amp%3B%20Jerry.txt\"")));
    assert!(page.contains(&format!("href=\"{root}/1/%D1%84%D0%BE%D1%82%D0%BE%201.png\"")));
    assert_eq!(get(&shared, &format!("{root}/1/%D1%84%D0%BE%D1%82%D0%BE%201.png"), "").body, b"second");
    assert_eq!(get(&shared, &format!("{root}/0"), "").body, b"first");
    assert_eq!(get(&shared, &format!("{root}/2/x"), "").status, NOT_FOUND);
}

#[test]
fn photo_previews_sit_outside_links_so_holding_offers_add_to_photos() {
    let sandbox = Sandbox::new();
    let shared = Shared::new(vec![sandbox.file("a.jpg", "a"), sandbox.file("b.png", "b")]);
    let page = get(&shared, &shared.root(), "").text();
    let previews: Vec<usize> = page.match_indices("<img").map(|(at, _)| at).collect();
    assert_eq!(previews.len(), 2);
    for at in previews {
        let before = &page[..at];
        if let Some(open) = before.rfind("<a ") {
            assert!(before[open..].contains("</a>"), "preview at {at} is inside a link");
        }
    }
}

#[test]
fn download_all_saves_every_file_even_the_ones_phones_can_show() {
    let sandbox = Sandbox::new();
    let shared = Shared::new(vec![sandbox.file("фото.jpg", "jpeg"), sandbox.file("notes.txt", "text")]);
    let root = shared.root();
    let page = get(&shared, &root, "").text();
    assert!(page.contains(&format!(">{}</button>", tr().phone_download_all)));
    assert!(page.contains(&format!("<img src=\"{root}/0/%D1%84%D0%BE%D1%82%D0%BE.jpg\"")), "photo preview to hold");
    assert!(!page.contains(&format!("<img src=\"{root}/1/")));
    assert!(page.contains(tr().phone_photo_hint));
    for path in [format!("{root}/0/x?download"), format!("{root}/1/x?from=qr&download")] {
        let saved = get(&shared, &path, "");
        assert!(saved.header("Content-Disposition").is_some_and(|value| value.starts_with("attachment;")), "{path}");
    }
    let shown = get(&shared, &format!("{root}/0/x?downloaded"), "");
    assert!(shown.header("Content-Disposition").is_some_and(|value| value.starts_with("inline;")));
}

#[test]
fn photo_hint_appears_only_when_there_are_photos() {
    let sandbox = Sandbox::new();
    let shared = Shared::new(vec![sandbox.file("a.txt", "a"), sandbox.file("b.zip", "PK")]);
    let page = get(&shared, &shared.root(), "").text();
    assert!(!page.contains(tr().phone_photo_hint));
    assert!(!page.contains("<img"));
}

#[test]
fn ranges_serve_parts_for_video_players() {
    let sandbox = Sandbox::new();
    let shared = Shared::new(vec![sandbox.file("clip.mp4", "0123456789")]);
    let root = shared.root();
    let part = |range: &str| get(&shared, &format!("{root}/0"), &format!("Range: {range}\r\n"));
    let middle = part("bytes=2-4");
    assert_eq!(middle.status, "206 Partial Content");
    assert_eq!(middle.body, b"234");
    assert_eq!(middle.header("Content-Range"), Some("bytes 2-4/10"));
    assert_eq!(middle.header("Content-Length"), Some("3"));
    assert_eq!(part("bytes=7-").body, b"789");
    assert_eq!(part("bytes=-3").body, b"789");
    assert_eq!(part("bytes=-30").body, b"0123456789");
    assert_eq!(part("bytes=8-100").header("Content-Range"), Some("bytes 8-9/10"));
    let past = part("bytes=10-");
    assert_eq!(past.status, "416 Range Not Satisfiable");
    assert_eq!(past.header("Content-Range"), Some("bytes */10"));
    assert!(past.body.is_empty());
    for ignored in ["bytes=5-3", "bytes=0-1,4-5", "lines=1-2", "bytes=x-"] {
        let whole = part(ignored);
        assert_eq!((whole.status.as_str(), whole.body.as_slice()), ("200 OK", b"0123456789".as_slice()), "{ignored}");
    }
}

#[test]
fn head_requests_get_headers_without_the_file() {
    let sandbox = Sandbox::new();
    let shared = Shared::new(vec![sandbox.file("clip.mp4", "0123456789")]);
    let response = exchange(&shared, &format!("HEAD {}/0 HTTP/1.1\r\n\r\n", shared.root()));
    assert_eq!(response.status, "200 OK");
    assert_eq!(response.header("Content-Length"), Some("10"));
    assert!(response.body.is_empty());
}

#[test]
fn file_removed_after_sharing_is_reported_missing() {
    let sandbox = Sandbox::new();
    let file = sandbox.file("gone.txt", "x");
    let shared = Shared::new(vec![file.clone()]);
    fs::remove_file(&file).expect("remove");
    assert_eq!(get(&shared, &format!("{}/0", shared.root()), "").status, NOT_FOUND);
}

#[test]
fn malformed_requests_are_refused() {
    let sandbox = Sandbox::new();
    let shared = Shared::new(vec![sandbox.file("a.txt", "a")]);
    let root = shared.root();
    assert_eq!(exchange(&shared, &format!("POST {root} HTTP/1.1\r\n\r\n")).status, "405 Method Not Allowed");
    assert_eq!(exchange(&shared, "GET").status, "400 Bad Request");
    let huge = format!("GET {root} HTTP/1.1\r\nX: {}\r\n\r\n", "a".repeat(HEAD_LIMIT * 2));
    assert_eq!(exchange(&shared, &huge).status, "400 Bad Request");
}
