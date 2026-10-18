use crate::{Ctx, input, wait, win};
use anyhow::{Context, Result, bail};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::UI::WindowsAndMessaging::WM_APP;

pub const TITLE: &str = "Traydex Test";
pub const RELAY: &str = "Traydex.Test.Relay";
pub const MUTEX: &str = "Local\\Traydex.Test.Singleton";
const QUIT: u32 = WM_APP + 5;

#[derive(Deserialize, Clone, Debug)]
pub struct Item {
    pub id: String,
    pub kind: String,
    pub path: PathBuf,
    pub text: Option<String>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Diag {
    pub pid: u32,
    pub page: String,
    pub layout: String,
    pub dark: bool,
    pub language: String,
    pub scale: f32,
    pub size: [f32; 2],
    pub dropping: bool,
    pub over: Option<String>,
    pub pinned: bool,
    pub preview_loaded: bool,
    pub items: Vec<Item>,
    pub marks: BTreeMap<String, [f32; 4]>,
}

impl Diag {
    pub fn has(&self, mark: &str) -> bool {
        self.marks.contains_key(mark)
    }
}

pub struct App {
    pub pid: u32,
    child: Child,
    data: PathBuf,
}

pub fn relay_of(pid: u32) -> Option<HWND> {
    win::windows().into_iter().find(|hwnd| win::pid(*hwnd) == pid && win::class(*hwnd) == RELAY)
}

impl App {
    pub fn launch(ctx: &Ctx, args: &[&OsStr]) -> Result<App> {
        Self::launch_with(ctx, args, None)
    }

    pub fn launch_with(ctx: &Ctx, args: &[&OsStr], frames: Option<&Path>) -> Result<App> {
        let mut command = Command::new(&ctx.sandbox.exe);
        command.arg("--test-profile").arg(&ctx.sandbox.profile).args(args);
        if let Some(folder) = ctx.ffmpeg.as_deref().and_then(Path::parent) {
            let path = std::env::var_os("PATH").unwrap_or_default();
            let paths = std::iter::once(folder.to_path_buf()).chain(std::env::split_paths(&path));
            command.env("PATH", std::env::join_paths(paths)?);
        }
        if let Some(frames) = frames {
            command.env("ZED_MEASUREMENTS", "1").stderr(fs::File::create(frames)?);
        }
        let child = command.spawn().context("couldn't start the test instance")?;
        let pid = child.id();
        input::allow_pid(pid);
        let app = App { pid, child, data: ctx.sandbox.data() };
        wait("the test instance created its relay window", Duration::from_secs(20), || relay_of(pid))?;
        wait("the test instance created the panel", Duration::from_secs(10), || app.panel())?;
        Ok(app)
    }

    pub fn run_once(ctx: &Ctx, args: &[&OsStr]) -> Result<std::process::ExitStatus> {
        let mut child =
            Command::new(&ctx.sandbox.exe).arg("--test-profile").arg(&ctx.sandbox.profile).args(args).spawn()?;
        input::allow_pid(child.id());
        let started = Instant::now();
        loop {
            if let Some(status) = child.try_wait()? {
                return Ok(status);
            }
            if started.elapsed() > Duration::from_secs(15) {
                let _ = child.kill();
                bail!("the second launch didn't exit within 15 s");
            }
            crate::pause(50)?;
        }
    }

    pub fn panel(&self) -> Option<HWND> {
        win::windows()
            .into_iter()
            .find(|hwnd| win::pid(*hwnd) == self.pid && win::title(*hwnd) == TITLE && win::class(*hwnd) != RELAY)
    }

    pub fn relay(&self) -> Option<HWND> {
        relay_of(self.pid)
    }

    pub fn shown(&self) -> bool {
        self.panel().is_some_and(win::visible)
    }

    pub fn alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    pub fn data(&self) -> &Path {
        &self.data
    }

    pub fn diag(&self) -> Option<Diag> {
        let text = fs::read(self.data.join("diagnostics.json")).ok()?;
        let diag: Diag = serde_json::from_slice(&text).ok()?;
        (diag.pid == self.pid).then_some(diag)
    }

    pub fn wait_shown(&self, shown: bool, what: &str, timeout: Duration) -> Result<()> {
        wait(what, timeout, || (self.shown() == shown).then_some(()))
    }

    pub fn wait_diag(&self, what: &str, timeout: Duration, ready: impl Fn(&Diag) -> bool) -> Result<Diag> {
        wait(what, timeout, || self.diag().filter(|diag| self.shown() && ready(diag)))
    }

    pub fn saved(&self) -> Option<Value> {
        serde_json::from_slice(&fs::read(self.data.join("shelf.json")).ok()?).ok()
    }

    pub fn saved_paths(&self) -> Vec<PathBuf> {
        self.saved()
            .and_then(|saved| {
                saved["items"]
                    .as_array()
                    .map(|items| items.iter().filter_map(|item| item["path"].as_str().map(PathBuf::from)).collect())
            })
            .unwrap_or_default()
    }

    fn mark(&self, diag: &Diag, mark: &str) -> Result<(i32, i32)> {
        let [x, y, w, h] = *diag.marks.get(mark).with_context(|| format!("the panel has no element \"{mark}\""))?;
        Ok((((x + w / 2.) * diag.scale).round() as i32, ((y + h / 2.) * diag.scale).round() as i32))
    }

    pub fn rect(&self, diag: &Diag, mark: &str) -> Result<[i32; 4]> {
        let panel = self.panel().context("panel not found")?;
        let origin = win::client(panel);
        let [x, y, w, h] = *diag.marks.get(mark).with_context(|| format!("the panel has no element \"{mark}\""))?;
        let s = diag.scale;
        Ok([
            origin.left + (x * s).round() as i32,
            origin.top + (y * s).round() as i32,
            origin.left + ((x + w) * s).round() as i32,
            origin.top + ((y + h) * s).round() as i32,
        ])
    }

    pub fn point(&self, diag: &Diag, mark: &str) -> Result<POINT> {
        let panel = self.panel().context("panel not found")?;
        let origin = win::client(panel);
        let (x, y) = self.mark(diag, mark)?;
        Ok(POINT { x: origin.left + x, y: origin.top + y })
    }

    pub fn post_click(&self, mark: &str) -> Result<()> {
        self.post_click_with(mark, false)
    }

    pub fn post_click_with(&self, mark: &str, right: bool) -> Result<()> {
        crate::check()?;
        let diag = self.diag().context("no panel diagnostics")?;
        let (x, y) = self.mark(&diag, mark)?;
        win::post_click(self.panel().context("panel not found")?, x, y, right);
        Ok(())
    }

    pub fn post_leave(&self) {
        if let Some(panel) = self.panel() {
            win::post_leave(panel);
        }
    }

    pub fn quit(&mut self) -> Result<()> {
        let relay = self.relay().context("the relay window of the test instance wasn't found")?;
        win::post(relay, QUIT, 0, 0);
        let started = Instant::now();
        while self.alive() {
            if started.elapsed() > Duration::from_secs(15) {
                bail!("the test instance didn't exit within 15 s after the quit command");
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        Ok(())
    }

    pub fn stop(&mut self) {
        if !self.alive() {
            return;
        }
        if self.quit().is_err() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

impl Drop for App {
    fn drop(&mut self) {
        self.stop();
    }
}

pub fn settings(ctx: &Ctx) -> Value {
    json!({
        "items": [],
        "automatic": true,
        "autostart": false,
        "context_menu": false,
        "double_click_preview": true,
        "theme": "Light",
        "layout": "List",
        "transparency": 0,
        "blur": false,
        "language": "Russian",
        "pinned": false,
        "shake": false,
        "corner": null,
        "position": [ctx.layout.panel.x, ctx.layout.panel.y],
        "size": [540.0, 540.0],
        "shortcuts": {}
    })
}

fn unique() -> String {
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    format!("e2e{:06}", COUNTER.fetch_add(1, Ordering::Relaxed))
}

pub fn path_item(path: &Path) -> Value {
    let kind = if path.is_dir() {
        "Folder"
    } else if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("png")) {
        "Image"
    } else {
        "File"
    };
    json!({
        "id": unique(),
        "kind": kind,
        "title": path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        "path": path,
        "text": null,
        "managed": false
    })
}

pub fn note_item(ctx: &Ctx, text: &str) -> Result<Value> {
    let id = unique();
    let content = ctx.sandbox.data().join("content");
    fs::create_dir_all(&content)?;
    let path = content.join(format!("note-{id}.txt"));
    fs::write(&path, text)?;
    Ok(json!({
        "id": id,
        "kind": "Text",
        "title": text.lines().next().unwrap_or_default(),
        "path": path,
        "text": text,
        "managed": true
    }))
}

pub fn seed(ctx: &Ctx, mut settings: Value, items: Vec<Value>, actions: Value) -> Result<()> {
    let data = ctx.sandbox.data();
    fs::create_dir_all(&data)?;
    settings["items"] = Value::Array(items);
    fs::write(data.join("shelf.json"), serde_json::to_vec_pretty(&settings)?)?;
    fs::write(data.join("actions.json"), serde_json::to_vec_pretty(&actions)?)?;
    Ok(())
}
