mod steps;
mod target;
mod window;

use crate::app::{self, App, Diag};
use crate::sandbox::{self, Sandbox};
use crate::{Ctx, Layout, guard, helper, input, win};
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::cell::RefCell;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use steps::Step;
use target::fits;
use windows::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError, HWND};
use windows::Win32::System::Threading::CreateMutexW;
use windows::core::w;

pub const USAGE: &str = "Agent UI check (via .\\build.ps1 -E2E probe [options] \"step\" \"step\" ...):
  Starts a separate test Traydex with a clean profile, runs the steps in order and closes it.
  Elements are addressed only by marks: a name (settings, item-title:ID), the N-th on screen (item#0, tile#2)
  or logical client-area coordinates (@120,48).

Options:
  --input              real mouse and keyboard (SendInput): the window is brought to the front and made topmost,
                       user input is blocked until the end, stop with Ctrl+Alt+Shift+Q. Needs the user's permission.
                       Without --input the window is shown without activation; clicks and keys are sent as window messages.
  --notes N --files N --images N   shelf items (3 notes by default)
  --layout list|grid  --theme light|dark|system  --language ru|en|zh|hi|es|fr  --size WxH (logical pixels)
  --animations         turn on window and interface animations
  --measure            record frames (the fps and sweep steps print the frame rate)
  --timeout S          limit for the whole check (default 120)

Steps:
  state | marks [prefix] | shot [name] [mark]
  click|rclick|dclick <mark> | hover <mark> (--input) | leave | scroll <mark> <wheel notches, + is down>
  key <combo: esc, down, ctrl+a, ctrl+shift+alt+f9> | type <text>
  wait <ms> | until page=X|mark=X|gone=X|shown|hidden
  sweep <mark> <mark> <seconds> (--input) | fps (--measure)

Output: the state after each step in the console, screenshots and the log in target\\e2e\\probe\\<time>\\.
Exit codes: 0 all steps done; 1 a step failed (failure.png next to it); 2 the check didn't start or was stopped.";

const LOCK_MARGIN: u64 = 30;
const COUNTDOWN: u64 = 3;
const SETTLE_QUIET: Duration = Duration::from_millis(150);
const SETTLE_LIMIT: Duration = Duration::from_secs(2);
const GRID: i32 = 5;
const DEFAULT_NOTES: usize = 3;

struct Options {
    input: bool,
    notes: usize,
    files: usize,
    images: usize,
    settings: Vec<(&'static str, Value)>,
    measure: bool,
    timeout: u64,
    steps: Vec<Step>,
}

fn choice(name: &str, value: &str, allowed: &[(&str, &str)]) -> Result<Value> {
    allowed
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(value))
        .map(|(_, stored)| Value::from(*stored))
        .with_context(|| {
            format!(
                "{name}: \"{value}\" isn't supported, choices: {}",
                allowed.iter().map(|(k, _)| *k).collect::<Vec<_>>().join(", ")
            )
        })
}

fn parse(args: &[String]) -> Result<Options> {
    let mut options = Options {
        input: false,
        notes: 0,
        files: 0,
        images: 0,
        settings: Vec::new(),
        measure: false,
        timeout: 120,
        steps: Vec::new(),
    };
    let mut items = false;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        let mut value = || args.next().with_context(|| format!("no value for {arg}")).map(String::as_str);
        let count =
            |raw: &str| raw.parse::<usize>().with_context(|| format!("{arg}: expected a number, got \"{raw}\""));
        match arg.as_str() {
            "--input" => options.input = true,
            "--measure" => options.measure = true,
            "--animations" => {
                options.settings.push(("window_animations", true.into()));
                options.settings.push(("interface_animations", true.into()));
            }
            "--notes" | "--files" | "--images" => {
                let n = count(value()?)?;
                items = true;
                match arg.as_str() {
                    "--notes" => options.notes = n,
                    "--files" => options.files = n,
                    _ => options.images = n,
                }
            }
            "--layout" => {
                options.settings.push(("layout", choice(arg, value()?, &[("list", "List"), ("grid", "Grid")])?))
            }
            "--theme" => options
                .settings
                .push(("theme", choice(arg, value()?, &[("light", "Light"), ("dark", "Dark"), ("system", "System")])?)),
            "--language" => options.settings.push((
                "language",
                choice(
                    arg,
                    value()?,
                    &[
                        ("ru", "Russian"),
                        ("en", "English"),
                        ("zh", "Chinese"),
                        ("hi", "Hindi"),
                        ("es", "Spanish"),
                        ("fr", "French"),
                    ],
                )?,
            )),
            "--size" => {
                let raw = value()?;
                let (width, height) = raw
                    .split_once(['x', 'X'])
                    .and_then(|(w, h)| Some((w.trim().parse::<f32>().ok()?, h.trim().parse::<f32>().ok()?)))
                    .with_context(|| format!("--size: expected WxH, e.g. 540x700, got \"{raw}\""))?;
                options.settings.push(("size", json!([width, height])));
            }
            "--timeout" => options.timeout = count(value()?)?.max(10) as u64,
            other if other.starts_with("--") => bail!("unknown option \"{other}\""),
            step => options.steps.push(Step::parse(step)?),
        }
    }
    if !items {
        options.notes = DEFAULT_NOTES;
    }
    if options.steps.is_empty() {
        options.steps.push(Step::parse("state")?);
    }
    for step in &options.steps {
        step.allowed(options.input, options.measure)?;
    }
    Ok(options)
}

pub fn main(args: &[String], layout: Layout) -> i32 {
    let options = match parse(args) {
        Ok(options) => options,
        Err(error) => {
            eprintln!("{error:#}\n\n{USAGE}");
            return 2;
        }
    };
    let _runner = unsafe { CreateMutexW(None, true, w!("Local\\Traydex.E2E.Runner")) };
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        eprintln!("Check not started: an E2E run or another check is in progress.");
        return 2;
    }
    let exe_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)).unwrap_or_default();
    let source = exe_dir.join("traydex.exe");
    if !source.is_file() {
        eprintln!("Check not started: {} not found", source.display());
        return 2;
    }
    if options.input && !win::input_desktop_available() {
        eprintln!("Check not started: the desktop is unavailable (the screen is locked or a secure desktop is open).");
        return 2;
    }
    let out = exe_dir.parent().unwrap_or(&exe_dir).join("e2e").join("probe").join(crate::stamp(true));
    if let Err(error) = fs::create_dir_all(&out) {
        eprintln!("Check not started: can't create {}: {error}", out.display());
        return 2;
    }
    let sandbox = Sandbox::new();
    for problem in sandbox::cleanup(&sandbox) {
        eprintln!("Couldn't clean up before start: {problem}");
    }
    let code = match prepare(&options, &sandbox, &source) {
        Ok(()) => session(options, sandbox, layout, out.clone()),
        Err(error) => {
            eprintln!("Check not started: {error:#}");
            2
        }
    };
    println!("Check folder: {}", out.display());
    code
}

fn prepare(options: &Options, sandbox: &Sandbox, source: &Path) -> Result<()> {
    sandbox.prepare(source)?;
    if options.input {
        guard::start()?;
    }
    let _ = crate::RUN_DEADLINE.set(Instant::now() + Duration::from_secs(options.timeout));
    let root = sandbox.root.clone();
    let limit = Duration::from_secs(options.timeout + LOCK_MARGIN);
    std::thread::spawn(move || {
        std::thread::sleep(limit);
        guard::trip("watchdog: the check hung");
        std::thread::sleep(Duration::from_secs(10));
        input::release_all();
        let _ = sandbox::cleanup(&Sandbox {
            profile: root.join("profile"),
            files: root.join("files"),
            exe: root.join("bin").join("traydex.exe"),
            root,
        });
        eprintln!("Watchdog: the check hung and was stopped, leftovers removed. Exit code 2.");
        std::process::exit(2);
    });
    Ok(())
}

fn session(options: Options, sandbox: Sandbox, layout: Layout, out: PathBuf) -> i32 {
    if options.input {
        println!("Mouse and keyboard input will be blocked during the check. Stop: {}.", guard::STOP_CHORD);
        for left in (1..=COUNTDOWN).rev() {
            println!("Starting in {left} s...");
            std::thread::sleep(Duration::from_secs(1));
            if let Some(reason) = guard::tripped() {
                println!("Cancelled before start: {reason}");
                guard::stop();
                return 2;
            }
        }
        guard::arm(Duration::from_secs(options.timeout + LOCK_MARGIN));
        input::allow_pid(std::process::id());
        if let Err(error) = helper::start(layout.helper) {
            println!("Couldn't create the helper window: {error:#}");
            guard::stop();
            return 2;
        }
        helper::status("Agent UI check");
    }
    let ctx = Ctx {
        sandbox,
        layout,
        reports: out.clone(),
        baselines: out.clone(),
        config: String::new(),
        update: false,
        powershell: None,
        ffmpeg: None,
        app: RefCell::new(None),
        changes: RefCell::new(Vec::new()),
    };
    let mut probe = Probe {
        ctx: &ctx,
        input: options.input,
        frames: options.measure.then(|| out.join("frames.log")),
        out,
        frames_from: 0,
        shots: 0,
    };
    let code = match probe.launch(&options) {
        Err(error) => {
            println!("Couldn't start the test Traydex: {error:#}");
            probe.failure();
            2
        }
        Ok(()) => probe.run(&options.steps),
    };
    probe.keep_artifacts();
    input::release_all();
    ctx.stop();
    if options.input {
        helper::stop();
        guard::stop();
    }
    for problem in sandbox::cleanup(&ctx.sandbox) {
        println!("Couldn't clean up: {problem}");
    }
    code
}

struct Probe<'a> {
    ctx: &'a Ctx,
    input: bool,
    out: PathBuf,
    frames: Option<PathBuf>,
    frames_from: u64,
    shots: usize,
}

impl Probe<'_> {
    fn app(&self) -> Result<std::cell::Ref<'_, App>> {
        self.ctx.app()
    }

    fn launch(&mut self, options: &Options) -> Result<()> {
        let ctx = self.ctx;
        let mut settings = app::settings(ctx);
        settings["automatic"] = false.into();
        settings["pinned"] = true.into();
        settings["size"] = json!([540.0, 700.0]);
        for (key, value) in &options.settings {
            settings[*key] = value.clone();
        }
        let mut items = Vec::new();
        for index in 0..options.notes {
            items.push(app::note_item(ctx, &format!("Note {index}: sample text"))?);
        }
        if options.files + options.images > 0 {
            let folder = ctx.sandbox.folder("probe")?;
            for index in 0..options.files {
                let path = folder.join(format!("file {index}.txt"));
                fs::write(&path, format!("Sample file {index}"))?;
                items.push(app::path_item(&path));
            }
            for index in 0..options.images {
                let path = folder.join(format!("picture {index}.png"));
                let shade = (index * 40 % 200) as u8;
                image::RgbaImage::from_fn(96, 64, |x, y| image::Rgba([shade, (x * 2) as u8, (y * 3) as u8, 255]))
                    .save(&path)?;
                items.push(app::path_item(&path));
            }
        }
        app::seed(ctx, settings, items, crate::suites::none())?;
        let args: &[&OsStr] = if self.input { &[] } else { &[OsStr::new("--inactive")] };
        *ctx.app.borrow_mut() = Some(App::launch_with(ctx, args, self.frames.as_deref())?);
        let app = self.app()?;
        let panel = app.panel().context("panel not found")?;
        app.wait_diag("the panel is shown and laid out for the window size", Duration::from_secs(10), |diag| {
            !diag.marks.is_empty() && fits(diag, panel)
        })?;
        drop(app);
        if self.input {
            self.front()?;
        }
        self.settle();
        self.on_screen()?;
        println!("Test Traydex started ({}).", if self.input { "real input" } else { "input as window messages" });
        self.state()
    }

    fn panel(&self) -> Result<HWND> {
        self.app()?.panel().context("the test Traydex panel wasn't found")
    }

    fn diag(&self) -> Result<Diag> {
        self.app()?.diag().context("the test Traydex didn't write diagnostics")
    }

    fn settle(&self) {
        let Ok(app) = self.app() else {
            return;
        };
        let file = app.data().join("diagnostics.json");
        let started = Instant::now();
        let mut last = fs::read(&file).unwrap_or_default();
        let mut quiet = Instant::now();
        while started.elapsed() < SETTLE_LIMIT && quiet.elapsed() < SETTLE_QUIET {
            std::thread::sleep(Duration::from_millis(30));
            let now = fs::read(&file).unwrap_or_default();
            if now != last {
                last = now;
                quiet = Instant::now();
            }
        }
    }

    fn brief(&self) -> String {
        let (Ok(panel), Ok(diag)) = (self.panel(), self.diag()) else {
            return "panel unavailable".into();
        };
        if !win::visible(panel) {
            return "panel hidden".into();
        }
        let cursor = win::cursor();
        let under = self.under(&diag, cursor).map_or("outside the window".into(), |mark| format!("over \"{mark}\""));
        format!("page {} · focus: {} · cursor ({}, {}) {under}", diag.page, self.focus(panel), cursor.x, cursor.y)
    }

    fn state(&self) -> Result<()> {
        let panel = self.panel()?;
        let diag = self.diag()?;
        let window = win::rect(panel);
        let covered = self.occluders(panel);
        println!(
            "Window \"{}\": {}, focus: {}, {}, {}",
            app::TITLE,
            if win::visible(panel) { "visible" } else { "hidden" },
            self.focus(panel),
            if win::topmost(panel) { "topmost" } else { "normal z-order" },
            if covered.is_empty() { "not covered".to_string() } else { format!("covered by: {}", covered.join(", ")) }
        );
        println!(
            "  screen: x {}..{}, y {}..{} (physical px), scale {} → {}×{} logical px",
            window.left,
            window.right,
            window.top,
            window.bottom,
            diag.scale,
            ((window.right - window.left) as f32 / diag.scale).round(),
            ((window.bottom - window.top) as f32 / diag.scale).round()
        );
        println!(
            "  page {} · layout {} · theme {} · language {} · items {} · {}",
            diag.page,
            diag.layout,
            if diag.dark { "dark" } else { "light" },
            diag.language,
            diag.items.len(),
            if diag.pinned { "pinned" } else { "not pinned" }
        );
        let cursor = win::cursor();
        println!(
            "  cursor ({}, {}) {}; marks {}",
            cursor.x,
            cursor.y,
            self.under(&diag, cursor).map_or("outside the window".into(), |mark| format!("over \"{mark}\"")),
            diag.marks.len()
        );
        Ok(())
    }

    fn shot(&mut self, name: Option<&str>, mark: Option<&str>) -> Result<PathBuf> {
        let panel = self.panel()?;
        let image = win::capture(panel).context("couldn't capture the window (PrintWindow)")?;
        let image = match mark {
            None => image,
            Some(mark) => {
                let target = self.target(mark)?;
                let window = win::rect(panel);
                let (x, y) =
                    ((target.rect.left - window.left).max(0) as u32, (target.rect.top - window.top).max(0) as u32);
                let width = ((target.rect.right - target.rect.left) as u32).min(image.width().saturating_sub(x));
                let height = ((target.rect.bottom - target.rect.top) as u32).min(image.height().saturating_sub(y));
                image::imageops::crop_imm(&image, x, y, width, height).to_image()
            }
        };
        self.shots += 1;
        let file = name.map_or_else(|| format!("shot-{}", self.shots), str::to_string);
        let path = self.out.join(if file.ends_with(".png") { file } else { format!("{file}.png") });
        image.save(&path).with_context(|| format!("couldn't save {}", path.display()))?;
        Ok(path)
    }

    fn failure(&mut self) {
        if let Ok(path) = self.shot(Some("failure"), None) {
            println!("  window screenshot at the moment of failure: {}", path.display());
        }
        let _ = self.state();
    }

    fn keep_artifacts(&self) {
        if let Ok(app) = self.app() {
            let _ = fs::copy(app.data().join("diagnostics.json"), self.out.join("diagnostics.json"));
            if let Ok(entries) = fs::read_dir(app.data().join("logs")) {
                for entry in entries.flatten() {
                    let _ = fs::copy(entry.path(), self.out.join(entry.file_name()));
                }
            }
        }
    }

    fn run(&mut self, steps: &[Step]) -> i32 {
        for (index, step) in steps.iter().enumerate() {
            let result = crate::check().and_then(|()| self.ready()).and_then(|()| self.execute(step));
            self.settle();
            match result {
                Ok(note) => println!(
                    "[{}] {step} → {}{}",
                    index + 1,
                    self.brief(),
                    note.map(|n| format!("\n    {n}")).unwrap_or_default()
                ),
                Err(error) => {
                    println!("[{}] {step} → FAILED: {error:#}", index + 1);
                    if let Some(reason) = guard::tripped() {
                        println!("Check stopped: {reason}");
                        return 2;
                    }
                    self.failure();
                    return 1;
                }
            }
        }
        0
    }
}
