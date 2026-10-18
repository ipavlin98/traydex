use super::{Session, Spec, Suite, none, spec, write};
use crate::app::{self, App, MUTEX};
use crate::{expect_eq, mismatch, pause, wait, win};
use anyhow::{Context, Result};
use std::ffi::OsStr;
use std::fs;
use std::time::{Duration, Instant};
use windows::Win32::UI::WindowsAndMessaging::{ENDSESSION_CLOSEAPP, WM_ENDSESSION, WM_QUERYENDSESSION};

pub const SUITE: Suite = Suite { id: "lifecycle", title: "Lifecycle", specs, run };

fn specs() -> Vec<Spec> {
    vec![
        spec("normal-start", "A normal start shows the panel", false),
        spec("hidden-start", "A start with --autostart stays hidden", false),
        spec("second-launch", "A second launch shows the window and passes paths via WM_COPYDATA", false),
        spec("exit", "Quit: the process exits, data is saved, temp files are removed", false),
        spec("system-close", "Windows closed from outside, as during a package update: quit with saving", false),
        spec("end-session", "Windows session end: quit with saving within seconds", false),
    ]
}

fn close_windows(app: &App) -> Result<()> {
    win::close(app.relay().context("relay window not found")?);
    if let Some(panel) = app.panel() {
        win::close(panel);
    }
    Ok(())
}

fn end_session(app: &App) -> Result<()> {
    let relay = app.relay().context("relay window not found")?;
    let reason = ENDSESSION_CLOSEAPP as isize;
    win::post(relay, WM_QUERYENDSESSION, 0, reason);
    win::post(relay, WM_ENDSESSION, 1, reason);
    Ok(())
}

fn closed_by_windows(ctx: &crate::Ctx, name: &str, request: fn(&App) -> Result<()>) -> Result<()> {
    ctx.reset()?;
    let folder = ctx.sandbox.folder(name)?;
    let sent = write(&folder.join("до закрытия.txt"), b"saved before closing")?;
    ctx.start(app::settings(ctx), vec![], none(), &[])?;
    App::run_once(ctx, &[OsStr::new("--send"), sent.as_os_str()])?;
    let (pid, data) = {
        let app = ctx.app()?;
        app.wait_shown(true, "the panel is visible before the Windows request", Duration::from_secs(10))?;
        wait("the path is added before the Windows request", Duration::from_secs(5), || {
            app.saved_paths().contains(&sent).then_some(())
        })?;
        request(&app)?;
        (app.pid, app.data().to_path_buf())
    };
    wait("the process exited on the Windows request", Duration::from_secs(5), || (!win::alive(pid)).then_some(()))?;
    expect_eq("mutex released", false, win::mutex_exists(MUTEX))?;
    let saved: serde_json::Value = serde_json::from_slice(&fs::read(data.join("shelf.json"))?)?;
    let paths: Vec<String> = saved["items"]
        .as_array()
        .map(|items| items.iter().filter_map(|i| i["path"].as_str().map(String::from)).collect())
        .unwrap_or_default();
    expect_eq("saved shelf items", vec![sent.display().to_string()], paths)?;
    Ok(())
}

fn run(s: &mut Session) {
    s.run("normal-start", |ctx| {
        ctx.reset()?;
        ctx.start(app::settings(ctx), vec![], none(), &[])?;
        let app = ctx.app()?;
        app.wait_shown(true, "the panel is visible after a normal start", Duration::from_secs(10))?;
        let diag =
            app.wait_diag("the panel is drawn and reports its state", Duration::from_secs(10), |d| d.page == "shelf")?;
        expect_eq("panel language from the settings", "Russian", diag.language.as_str())?;
        let relay = app.relay().context("relay window not found")?;
        expect_eq("relay window title", app::TITLE.to_string(), win::title(relay))?;
        Ok(())
    });

    s.run("hidden-start", |ctx| {
        ctx.reset()?;
        ctx.start(app::settings(ctx), vec![], none(), &["--autostart"])?;
        let app = ctx.app()?;
        let started = Instant::now();
        while started.elapsed() < Duration::from_secs(3) {
            if app.shown() {
                return Err(mismatch("the panel appeared on a hidden start", "hidden", "visible"));
            }
            pause(100)?;
        }
        expect_eq("the test instance mutex exists", true, win::mutex_exists(MUTEX))?;
        Ok(())
    });

    s.run("second-launch", |ctx| {
        ctx.reset()?;
        let folder = ctx.sandbox.folder("lifecycle")?;
        let file = write(&folder.join("второй запуск.txt"), b"second launch")?;
        let sub = folder.join("Папка с пробелами");
        fs::create_dir_all(&sub)?;
        ctx.start(app::settings(ctx), vec![], none(), &["--autostart"])?;
        let first = ctx.app()?.pid;
        let status = App::run_once(ctx, &[])?;
        expect_eq("exit code of the second launch", Some(0), status.code())?;
        ctx.app()?.wait_shown(true, "the second launch showed the panel", Duration::from_secs(5))?;
        let status = App::run_once(ctx, &[OsStr::new("--send"), file.as_os_str(), sub.as_os_str()])?;
        expect_eq("exit code of the launch with paths", Some(0), status.code())?;
        let app = ctx.app()?;
        wait("the passed paths are saved on the shelf", Duration::from_secs(5), || {
            let saved = app.saved_paths();
            (saved.contains(&file) && saved.contains(&sub)).then_some(())
        })?;
        app.wait_diag("the passed paths are shown on the panel", Duration::from_secs(5), |d| {
            d.items.iter().any(|i| i.path == file) && d.items.iter().any(|i| i.path == sub)
        })?;
        expect_eq("one main process is left", first, app.pid)?;
        let others: Vec<u32> =
            win::processes_under(&ctx.sandbox.root).into_iter().filter(|pid| *pid != first).collect();
        expect_eq("extra test Traydex processes", Vec::<u32>::new(), others)?;
        Ok(())
    });

    s.run("exit", |ctx| {
        ctx.reset()?;
        let folder = ctx.sandbox.folder("exit")?;
        let kept = write(&folder.join("seeded.txt"), b"seeded")?;
        let sent = write(&folder.join("sent.txt"), b"sent")?;
        ctx.start(app::settings(ctx), vec![app::path_item(&kept)], none(), &[])?;
        App::run_once(ctx, &[OsStr::new("--send"), sent.as_os_str()])?;
        let (pid, data) = {
            let app = ctx.app()?;
            wait("the path is added before quitting", Duration::from_secs(5), || {
                app.saved_paths().contains(&sent).then_some(())
            })?;
            (app.pid, app.data().to_path_buf())
        };
        ctx.quit()?;
        expect_eq("process exited", false, win::alive(pid))?;
        expect_eq("relay window closed", None, app::relay_of(pid).map(|h| h.0 as isize))?;
        expect_eq("mutex released", false, win::mutex_exists(MUTEX))?;
        let saved: serde_json::Value = serde_json::from_slice(&fs::read(data.join("shelf.json"))?)?;
        let mut paths: Vec<String> = saved["items"]
            .as_array()
            .map(|items| items.iter().filter_map(|i| i["path"].as_str().map(String::from)).collect())
            .unwrap_or_default();
        paths.sort();
        let mut expected = vec![kept.display().to_string(), sent.display().to_string()];
        expected.sort();
        expect_eq("saved shelf items", expected, paths)?;
        let leftovers: Vec<String> = fs::read_dir(&data)?
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".tmp"))
            .collect();
        expect_eq("unfinished temp files in the profile", Vec::<String>::new(), leftovers)?;
        let temp: Vec<String> = fs::read_dir(ctx.sandbox.profile.join("Temp"))
            .map(|entries| entries.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect())
            .unwrap_or_default();
        expect_eq("the test profile temp folder is empty", Vec::<String>::new(), temp)?;
        Ok(())
    });

    s.run("system-close", |ctx| closed_by_windows(ctx, "system-close", close_windows));

    s.run("end-session", |ctx| closed_by_windows(ctx, "end-session", end_session));
}
