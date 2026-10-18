use super::*;
use crate::actions::{Action, Kind, base, run};
use crate::i18n::Language;
use crate::store::Payload;
use crate::testing::{Call, Fake, Invocation, Sandbox, language, output, plant_tools, video_info};
use std::io;

const VIDEO: &str = "https://www.youtube.com/watch?v=abc&list=PL1";
const NOT_A_BOT: &str = "ERROR: [youtube] abc: Sign in to confirm you're not a bot. \
    Use --cookies-from-browser or --cookies for the authentication.";
const MEMBERS_ONLY: &str = "ERROR: [youtube] abc: Join this channel to get access to members-only content. \
    Use --cookies-from-browser or --cookies for the authentication.";
const DPAPI: &str = "ERROR: Failed to decrypt with DPAPI. See https://github.com/yt-dlp/yt-dlp/issues/10927";

struct Bench {
    sandbox: Sandbox,
    fake: Fake,
    storage: Storage,
}

impl Bench {
    fn new() -> Self {
        let sandbox = Sandbox::new();
        let fake = Fake::new(&sandbox);
        let storage = sandbox.storage();
        plant_tools(&storage);
        Self { sandbox, fake, storage }
    }

    fn respond(&self, code: u32, stdout: &str, stderr: &str) {
        self.fake.state().outputs.push_back(Ok(output(code, stdout, stderr)));
    }

    fn probe(&self, payload: Payload) -> Result<Media> {
        let action = Action::new(Kind::Ytdlp(Fetch::default()));
        let outcome = run(&action, payload, &Progress::default(), &self.storage, &self.fake)?;
        match outcome.show {
            Some(Show::Media(media)) => Ok(media),
            _ => panic!("expected the choices page"),
        }
    }

    fn calls(&self) -> Vec<Vec<String>> {
        self.fake
            .calls()
            .into_iter()
            .filter_map(|call| match call {
                Call::Output(invocation) => Some(invocation.args()),
                _ => None,
            })
            .collect()
    }

    fn browsers(&self, roaming: &[&str], local: &[&str]) {
        let mut state = self.fake.state();
        state.roaming = Some(self.sandbox.folder("roaming"));
        state.local = Some(self.sandbox.folder("local"));
        drop(state);
        for folder in roaming {
            self.sandbox.folder(&format!("roaming/{folder}"));
        }
        for folder in local {
            self.sandbox.folder(&format!("local/{folder}"));
        }
    }

    fn produce(&self, names: &'static [&'static str], code: u32, stdout: &'static str, stderr: &'static str) {
        self.fake.state().streamer = Some(Box::new(move |invocation: &Invocation| {
            let args = invocation.args();
            let folder = args.iter().position(|arg| arg == "-P").and_then(|at| args.get(at + 1)).expect("-P folder");
            for name in names {
                fs::write(Path::new(folder).join(name), "data").expect("file");
            }
            Ok(output(code, stdout, stderr))
        }));
    }
}

fn video(fetch_url: &str) -> Media {
    parse_info(video_info("Клип").as_bytes(), fetch_url, None).expect("media")
}

fn has(args: &[String], pair: [&str; 2]) -> bool {
    args.windows(2).any(|window| window == pair)
}

fn error(result: Result<impl Sized>) -> String {
    match result {
        Ok(_) => panic!("expected an error"),
        Err(error) => error.to_string(),
    }
}

#[test]
fn link_comes_from_dropped_text_shortcuts_and_notes() {
    let sandbox = Sandbox::new();
    let note = sandbox.file("note.txt", "смотри: https://youtu.be/abc?si=1 — классно");
    let shortcut = sandbox.file("clip.url", "[InternetShortcut]\r\nURL=https://vk.com/video1\r\n");
    let picture = sandbox.file("photo.png", "https://example.com/not-read");
    let empty = sandbox.file("empty.txt", "просто текст");
    assert_eq!(link(std::slice::from_ref(&note)).expect("note"), "https://youtu.be/abc?si=1");
    assert_eq!(link(&[picture.clone(), shortcut]).expect("shortcut"), "https://vk.com/video1");
    assert_eq!(error(link(&[picture, empty])), tr().ytdlp_needs_link);
}

#[test]
fn probe_offers_what_the_video_really_has() {
    let _ru = language(Language::Russian);
    let bench = Bench::new();
    bench.respond(0, &video_info("Клип о котах"), "");
    let media = bench.probe(Payload::Text(format!("  {VIDEO}\n"))).expect("media");
    assert_eq!((media.title.as_str(), media.url.as_str()), ("Клип о котах", VIDEO));
    assert!(media.allows(1080) && !media.allows(1440), "only up to the best video stream");
    assert_eq!(media.quality(&Fetch { quality: Quality::Height(2160), ..Fetch::default() }), Quality::Best);
    assert_eq!(media.hint(Extra::Subtitles), "ru, en-US", "Russian and English, manual subtitles first");
    assert!(media.offers(Extra::Playlist) && media.offers(Extra::Sponsors));
    assert!(!media.applies(&Fetch::default(), Extra::Playlist), "only this video unless asked");
    let args = bench.calls().remove(0);
    assert!(args.ends_with(&["-J".into(), "--flat-playlist".into(), "--no-playlist".into(), VIDEO.into()]));
    let tools = bench.storage.root().join("tools");
    let ffmpeg = tools.join("ffmpeg.exe").display().to_string();
    let deno = format!("deno:{}", tools.join("deno.exe").display());
    assert!(has(&args, ["--ffmpeg-location", &ffmpeg]) && has(&args, ["--js-runtimes", &deno]));
    assert!(!args.iter().any(|arg| arg == "--cookies-from-browser"), "no cookies when the site doesn't ask");
}

#[test]
fn playlist_link_downloads_the_whole_list() {
    let _ru = language(Language::Russian);
    let bench = Bench::new();
    let list = serde_json::json!({"_type": "playlist", "title": "Сборник", "playlist_count": 25, "entries": []});
    bench.respond(0, &list.to_string(), "");
    let media = bench.probe(Payload::Text("https://www.youtube.com/playlist?list=PL1".into())).expect("playlist");
    assert!(media.applies(&Fetch::default(), Extra::Playlist));
    assert!(!media.offers(Extra::Playlist), "nothing to switch off");
    assert_eq!(media.hint(Extra::Playlist), fill(tr().media_entries, 25));
    assert!(media.allows(2160), "qualities stay open when the list isn't inspected");
    assert!(arguments(&media, &Fetch::default()).contains(&"--yes-playlist".to_string()));
}

#[test]
fn sign_in_is_borrowed_from_the_first_browser_that_has_it() {
    let bench = Bench::new();
    bench.browsers(&[r"Mozilla\Firefox\Profiles"], &[r"Google\Chrome\User Data"]);
    bench.respond(1, "", NOT_A_BOT);
    bench.respond(1, "", "ERROR: could not find firefox cookies database");
    bench.respond(0, &video_info("Клип"), "");
    let media = bench.probe(Payload::Text(VIDEO.into())).expect("signed in");
    let calls = bench.calls();
    assert_eq!(calls.len(), 3);
    assert!(has(&calls[1], ["--cookies-from-browser", "firefox"]));
    assert!(has(&calls[2], ["--cookies-from-browser", "chrome"]));
    bench.produce(&["Клип.mp4"], 0, "", "");
    let fetched = download_media(&media, &Fetch::default(), &Progress::default(), &bench.storage, &bench.fake);
    assert_eq!(fetched.expect("download").created.len(), 1);
    let download = bench.calls().pop().expect("download");
    assert!(has(&download, ["--cookies-from-browser", "chrome"]), "the download reuses the browser that worked");
}

#[test]
fn missing_sign_in_is_explained() {
    let _ru = language(Language::Russian);
    let bench = Bench::new();
    bench.respond(1, "", MEMBERS_ONLY);
    let unreadable = error(bench.probe(Payload::Text(VIDEO.into())));
    assert_eq!(unreadable, fill(tr().ytdlp_no_cookies, "youtube.com"), "no browser could lend a sign-in");

    let bench = Bench::new();
    bench.browsers(&[r"Mozilla\Firefox\Profiles"], &[]);
    bench.respond(1, "", MEMBERS_ONLY);
    bench.respond(1, "", MEMBERS_ONLY);
    let signed_out = error(bench.probe(Payload::Text(VIDEO.into())));
    assert_eq!(signed_out, fill(tr().ytdlp_sign_in, "youtube.com"), "the browser was read but isn't signed in");
}

#[test]
fn bot_check_is_explained_apart_from_a_closed_video() {
    let _ru = language(Language::Russian);
    let bench = Bench::new();
    bench.browsers(&[r"Mozilla\Firefox\Profiles"], &[r"Microsoft\Edge\User Data", r"Google\Chrome\User Data"]);
    for stderr in [NOT_A_BOT, NOT_A_BOT, DPAPI, DPAPI] {
        bench.respond(1, "", stderr);
    }
    let refused = error(bench.probe(Payload::Text(VIDEO.into())));
    assert_eq!(refused, fill(tr().ytdlp_bot, "youtube.com"));
    assert_eq!(bench.calls().len(), 4, "every browser is still tried");
}

#[test]
fn other_failures_show_the_ytdlp_reason_without_retrying() {
    let bench = Bench::new();
    bench.browsers(&[r"Mozilla\Firefox\Profiles"], &[]);
    bench.respond(1, "", "WARNING: slow\nERROR: Unsupported URL: https://example.com/\n");
    assert_eq!(
        error(bench.probe(Payload::Text("https://example.com/".into()))),
        "Unsupported URL: https://example.com/"
    );
    assert_eq!(bench.calls().len(), 1);
}

#[test]
fn missing_tools_are_named_before_anything_runs() {
    let sandbox = Sandbox::new();
    let fake = Fake::new(&sandbox);
    let action = Action::new(Kind::Ytdlp(Fetch::default()));
    let result = run(&action, Payload::Text(VIDEO.into()), &Progress::default(), &sandbox.storage(), &fake);
    assert_eq!(error(result), fill(tr().tool_missing, "yt-dlp"));
    assert!(fake.calls().is_empty());
}

#[test]
fn options_become_ytdlp_arguments() {
    let _ru = language(Language::Russian);
    let media = video(VIDEO);
    let plain = arguments(&media, &Fetch::default());
    assert!(has(&plain, ["-S", "res,ext:mp4:m4a"]) && plain.contains(&"--no-playlist".into()));
    assert!(!plain.iter().any(|arg| arg == "-x" || arg == "--write-subs" || arg == "--embed-thumbnail"));
    let limited = arguments(&media, &Fetch { quality: Quality::Height(720), ..Fetch::default() });
    assert!(has(&limited, ["-S", "res:720,ext:mp4:m4a"]));
    let everything = Fetch {
        mode: Mode::Audio,
        quality: Quality::Height(720),
        subtitles: true,
        cover: true,
        sponsors: true,
        playlist: true,
    };
    let args = arguments(&media, &everything);
    assert!(has(&args, ["--audio-format", "mp3"]) && args.contains(&"-x".into()));
    assert!(!args.contains(&"-S".into()), "audio ignores the video quality");
    assert!(has(&args, ["--sub-langs", "ru,en-US"]) && has(&args, ["--convert-subs", "srt"]));
    assert!(args.contains(&"--embed-thumbnail".into()) && has(&args, ["--sponsorblock-remove", "sponsor"]));
    assert!(args.contains(&"--yes-playlist".into()));
    let other = parse_info(br#"{"title": "x", "extractor_key": "Vimeo"}"#, "https://vimeo.com/1", None).expect("media");
    let args = arguments(&other, &everything);
    assert!(
        !args.iter().any(|arg| arg.starts_with("--sponsorblock") || arg == "--write-subs" || arg == "--yes-playlist")
    );
}

#[test]
fn subtitles_prefer_the_interface_language_then_english() {
    let keys = |list: &[&str]| list.iter().map(|key| key.to_string()).collect::<Vec<_>>();
    let _ru = language(Language::Russian);
    assert_eq!(pick(&keys(&["de", "en-GB"]), &keys(&["ru", "en"])), keys(&["ru", "en-GB"]));
    assert_eq!(pick(&keys(&["live_chat", "ja"]), &[]), keys(&["ja"]), "falls back to the original language");
    assert!(pick(&keys(&["live_chat"]), &[]).is_empty());
    let _en = language(Language::English);
    assert_eq!(pick(&keys(&["en"]), &keys(&["en", "ru"])), keys(&["en"]));
}

#[test]
fn progress_lines_become_an_overall_fraction() {
    assert_eq!(fraction("[traydex] 50 200 NA NA"), Some(0.25));
    assert_eq!(fraction("[traydex] 10 10 3 4"), Some(0.75), "third of four videos done");
    assert_eq!(fraction("[traydex] 300.5 200.0 NA NA"), Some(1.));
    assert_eq!(fraction("[traydex] 100 NA NA NA"), None);
    assert_eq!(fraction("[traydex] 100 0 NA NA"), None);
    assert_eq!(fraction("[download] 50.0% of 10MiB"), None);
}

#[test]
fn download_keeps_finished_files_and_reports_progress() {
    let bench = Bench::new();
    bench.produce(
        &["Клип.mp3", "Клип.ru.srt", "Клип.mp4.part", "Клип.f137.mp4.part-Frag3"],
        0,
        "[youtube] abc: Downloading\n[traydex] 50 100 1 2\n[traydex] 0 100 1 2\n",
        "",
    );
    let progress = Progress::default();
    let fetch = Fetch { mode: Mode::Audio, ..Fetch::default() };
    let outcome = download_media(&video(VIDEO), &fetch, &progress, &bench.storage, &bench.fake).expect("downloaded");
    let names: Vec<String> = outcome.created.iter().map(|path| base(path)).collect();
    assert_eq!(names, ["Клип.mp3", "Клип.ru.srt"]);
    assert!(outcome.created.iter().all(|path| bench.storage.owned(path)), "files land on the shelf storage");
    assert_eq!(outcome.message, fill(tr().converted, 2));
    assert_eq!(progress.get().fraction, Some(0.25), "progress never goes back");
    let args = bench.calls().pop().expect("download");
    assert_eq!(args.last().map(String::as_str), Some(VIDEO));
}

#[test]
fn failed_or_stopped_download_leaves_nothing_behind() {
    let bench = Bench::new();
    bench.produce(&["Клип.mp4.part"], 1, "", "ERROR: [youtube] abc: Video unavailable");
    let failed = download_media(&video(VIDEO), &Fetch::default(), &Progress::default(), &bench.storage, &bench.fake);
    assert_eq!(error(failed), "[youtube] abc: Video unavailable");
    assert_eq!(fs::read_dir(bench.storage.content()).expect("content").count(), 0);

    bench.fake.state().streamer = Some(Box::new(|invocation: &Invocation| {
        let args = invocation.args();
        let folder = args.iter().position(|arg| arg == "-P").and_then(|at| args.get(at + 1)).expect("folder");
        fs::write(Path::new(folder).join("Клип.mp4"), "half").expect("partial");
        Err(io::ErrorKind::Interrupted.into())
    }));
    let stopped = download_media(&video(VIDEO), &Fetch::default(), &Progress::default(), &bench.storage, &bench.fake);
    assert!(stopped.is_err());
    assert_eq!(fs::read_dir(bench.storage.content()).expect("content").count(), 0, "partial files are removed");
}
