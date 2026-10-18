use super::*;
use crate::testing::{Call, Fake, Sandbox, archive};
use std::sync::Arc;

fn serve(fake: &Fake, entries: &'static [(&'static str, &'static [u8])]) {
    fake.state().downloader = Some(Box::new(move |_, target, _| {
        archive(target, entries);
        Ok(())
    }));
}

fn downloads(fake: &Fake) -> usize {
    fake.calls().iter().filter(|call| matches!(call, Call::Download(..))).count()
}

#[test]
fn presence_prefers_bundled_tool() {
    let sandbox = Sandbox::new();
    let fake = Fake::new(&sandbox);
    let storage = sandbox.storage();
    assert!(Tool::Ffmpeg.presence(&storage, &fake) == Presence::Missing);
    assert!(!Presence::Missing.ready());
    let system = sandbox.file("bin/ffmpeg.exe", "MZ");
    fake.state().search_path = Some(sandbox.join("bin").into_os_string());
    assert!(Tool::Ffmpeg.presence(&storage, &fake) == Presence::System);
    assert_eq!(Tool::Ffmpeg.locate(&storage, &fake), Some(system));
    let bundled = sandbox.file("root/tools/ffmpeg.exe", "MZ");
    assert!(Tool::Ffmpeg.presence(&storage, &fake) == Presence::Bundled);
    assert!(Presence::Bundled.ready() && Presence::System.ready());
    assert_eq!(Tool::Ffmpeg.locate(&storage, &fake), Some(bundled));
}

#[test]
fn folders_on_search_path_are_not_mistaken_for_tools() {
    let sandbox = Sandbox::new();
    let fake = Fake::new(&sandbox);
    sandbox.folder("bin/ffmpeg.exe");
    fake.state().search_path = Some(sandbox.join("bin").into_os_string());
    assert!(Tool::Ffmpeg.presence(&sandbox.storage(), &fake) == Presence::Missing);
    fake.state().search_path = None;
    assert!(Tool::Ffmpeg.locate(&sandbox.storage(), &fake).is_none());
}

#[test]
fn missing_tool_error_names_it() {
    let sandbox = Sandbox::new();
    let fake = Fake::new(&sandbox);
    let failure = Tool::Ffmpeg.require(&sandbox.storage(), &fake).expect_err("missing");
    assert_eq!(failure.to_string(), fill(tr().tool_missing, "ffmpeg"));
    assert_eq!(Tool::Ffmpeg.download_label(), fill(tr().tool_download, "ffmpeg"));
}

#[test]
fn install_downloads_and_unpacks_into_tools_folder() {
    let sandbox = Sandbox::new();
    let fake = Fake::new(&sandbox);
    let storage = sandbox.storage();
    serve(
        &fake,
        &[("ffmpeg-7.1-essentials/README.txt", b"readme"), ("ffmpeg-7.1-essentials/bin/ffmpeg.exe", b"MZ ffmpeg")],
    );
    let installed = Tool::Ffmpeg.install(&storage, &fake, &Progress::default()).expect("install");
    assert_eq!(installed, storage.root().join("tools").join("ffmpeg.exe"));
    assert_eq!(fs::read(&installed).expect("exe"), b"MZ ffmpeg");
    let folder = storage.root().join("tools");
    let names: Vec<_> = fs::read_dir(&folder).expect("tools").flatten().map(|entry| entry.file_name()).collect();
    assert_eq!(names, ["ffmpeg.exe"]);
    assert_eq!(fake.calls(), [Call::Download(Tool::Ffmpeg.url().into(), folder.join("ffmpeg.zip"))]);
    assert!(Tool::Ffmpeg.url().starts_with("https://"));
}

#[test]
fn install_is_skipped_when_tool_is_present() {
    let sandbox = Sandbox::new();
    let fake = Fake::new(&sandbox);
    let system = sandbox.file("bin/ffmpeg.exe", "MZ");
    fake.state().search_path = Some(sandbox.join("bin").into_os_string());
    assert_eq!(Tool::Ffmpeg.install(&sandbox.storage(), &fake, &Progress::default()).expect("present"), system);
    assert_eq!(downloads(&fake), 0);
}

#[test]
fn failed_download_cleans_up() {
    let sandbox = Sandbox::new();
    let fake = Fake::new(&sandbox);
    let storage = sandbox.storage();
    fake.state().downloader = Some(Box::new(|_, target, _| {
        fs::write(target, b"partial").expect("partial");
        Err(anyhow::anyhow!("connection reset"))
    }));
    let failure = Tool::Ffmpeg.install(&storage, &fake, &Progress::default()).expect_err("failure");
    assert_eq!(failure.to_string(), fill(tr().tool_failed, "ffmpeg"));
    assert_eq!(format!("{failure:#}"), format!("{}: connection reset", fill(tr().tool_failed, "ffmpeg")));
    assert!(!storage.root().join("tools").exists());
}

#[test]
fn archive_without_tool_is_rejected() {
    let sandbox = Sandbox::new();
    let fake = Fake::new(&sandbox);
    let storage = sandbox.storage();
    serve(&fake, &[("other/bin/ffprobe.exe", b"MZ")]);
    assert!(Tool::Ffmpeg.install(&storage, &fake, &Progress::default()).is_err());
    assert!(!storage.root().join("tools").exists());
}

#[test]
fn corrupt_archive_is_rejected() {
    let sandbox = Sandbox::new();
    let fake = Fake::new(&sandbox);
    let storage = sandbox.storage();
    fake.state().downloader = Some(Box::new(|_, target, _| {
        fs::write(target, b"<html>not a zip</html>").expect("write");
        Ok(())
    }));
    assert!(Tool::Ffmpeg.install(&storage, &fake, &Progress::default()).is_err());
    assert!(!storage.root().join("tools").exists());
}

#[test]
fn cancelled_install_leaves_no_files() {
    let sandbox = Sandbox::new();
    let fake = Fake::new(&sandbox);
    let storage = sandbox.storage();
    fake.state().downloader = Some(Box::new(|_, target, progress| {
        archive(target, &[("bin/ffmpeg.exe", b"MZ")]);
        progress.cancel();
        Ok(())
    }));
    let failure = Tool::Ffmpeg.install(&storage, &fake, &Progress::default()).expect_err("cancelled");
    assert_eq!(format!("{failure:#}"), format!("{}: {}", fill(tr().tool_failed, "ffmpeg"), tr().operation_cancelled));
    assert!(!storage.root().join("tools").exists());
}

#[test]
fn concurrent_installs_download_once() {
    let sandbox = Sandbox::new();
    let fake = Arc::new(Fake::new(&sandbox));
    let storage = sandbox.storage();
    serve(&fake, &[("bin/ffmpeg.exe", b"MZ")]);
    let workers: Vec<_> = (0..4)
        .map(|_| {
            let (fake, storage) = (fake.clone(), storage.clone());
            std::thread::spawn(move || Tool::Ffmpeg.install(&storage, &*fake, &Progress::default()))
        })
        .collect();
    for worker in workers {
        worker.join().expect("thread").expect("install");
    }
    assert_eq!(downloads(&fake), 1);
}

#[test]
fn sweep_removes_unfinished_downloads_only() {
    let sandbox = Sandbox::new();
    let storage = sandbox.storage();
    let exe = sandbox.file("root/tools/ffmpeg.exe", "MZ");
    let zip = sandbox.file("root/tools/ffmpeg.zip", "partial");
    let tmp = sandbox.file("root/tools/ffmpeg.tmp", "partial");
    Tool::sweep(&storage);
    assert!(exe.exists());
    assert!(!zip.exists() && !tmp.exists());
    fs::remove_file(&exe).expect("remove");
    sandbox.file("root/tools/ffmpeg.zip", "partial");
    Tool::sweep(&storage);
    assert!(!storage.root().join("tools").exists());
    Tool::sweep(&storage);
}

#[test]
fn remove_deletes_bundled_tool_and_empty_folder() {
    let sandbox = Sandbox::new();
    let storage = sandbox.storage();
    assert!(Tool::Ffmpeg.remove(&storage).is_err());
    sandbox.file("root/tools/ffmpeg.exe", "MZ");
    Tool::Ffmpeg.remove(&storage).expect("remove");
    assert!(!storage.root().join("tools").exists());
    sandbox.file("root/tools/ffmpeg.exe", "MZ");
    let keep = sandbox.file("root/tools/notes.txt", "x");
    Tool::Ffmpeg.remove(&storage).expect("remove");
    assert!(keep.exists());
}

#[test]
fn plain_executable_is_downloaded_without_unpacking() {
    let sandbox = Sandbox::new();
    let fake = Fake::new(&sandbox);
    let storage = sandbox.storage();
    fake.state().downloader = Some(Box::new(|_, target, _| {
        fs::write(target, b"MZ yt-dlp").expect("exe");
        Ok(())
    }));
    let installed = Tool::YtDlp.install(&storage, &fake, &Progress::default()).expect("install");
    assert_eq!(installed, storage.root().join("tools").join("yt-dlp.exe"));
    assert_eq!(fs::read(&installed).expect("exe"), b"MZ yt-dlp");
    let names: Vec<_> =
        fs::read_dir(storage.root().join("tools")).expect("tools").flatten().map(|e| e.file_name()).collect();
    assert_eq!(names, ["yt-dlp.exe"], "no temporary file is left");
    assert!(Tool::YtDlp.presence(&storage, &fake) == Presence::Bundled);
}

#[test]
fn failed_executable_download_leaves_no_partial_tool() {
    let sandbox = Sandbox::new();
    let fake = Fake::new(&sandbox);
    let storage = sandbox.storage();
    fake.state().downloader = Some(Box::new(|_, target, _| {
        fs::write(target, b"MZ half").expect("partial");
        Err(anyhow::anyhow!("connection reset"))
    }));
    assert!(Tool::YtDlp.install(&storage, &fake, &Progress::default()).is_err());
    assert!(!storage.root().join("tools").exists());
    assert!(Tool::YtDlp.presence(&storage, &fake) == Presence::Missing);
}
