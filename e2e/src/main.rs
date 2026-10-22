mod app;
mod explorer;
mod frames;
mod guard;
mod helper;
mod input;
mod probe;
mod report;
mod sandbox;
mod snap;
mod suites;
mod win;

use anyhow::{Result, bail};
use report::{Report, Status};
use sandbox::Sandbox;
use std::cell::{Ref, RefCell};
use std::ffi::OsStr;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use windows::Win32::{
    Foundation::{ERROR_ALREADY_EXISTS, GetLastError, POINT, RECT},
    System::{
        Com::{COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx},
        SystemInformation::GetLocalTime,
        Threading::CreateMutexW,
    },
    UI::{
        HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext},
        Input::KeyboardAndMouse::{MOD_ALT, MOD_CONTROL, MOD_SHIFT, VK_F9},
    },
};
use windows::core::w;

#[derive(Debug)]
pub enum Stop {
    Aborted(String),
    Timeout(String),
    Skip(String),
}

impl fmt::Display for Stop {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Stop::Aborted(reason) => write!(f, "run stopped: {reason}"),
            Stop::Timeout(reason) => write!(f, "{reason}"),
            Stop::Skip(reason) => write!(f, "{reason}"),
        }
    }
}

impl std::error::Error for Stop {}

#[derive(Debug)]
pub struct Mismatch {
    pub what: String,
    pub expected: String,
    pub actual: String,
}

impl fmt::Display for Mismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.what)
    }
}

impl std::error::Error for Mismatch {}

pub fn mismatch(what: &str, expected: impl fmt::Debug, actual: impl fmt::Debug) -> anyhow::Error {
    Mismatch { what: what.to_string(), expected: format!("{expected:?}"), actual: format!("{actual:?}") }.into()
}

pub fn expect_eq<T: fmt::Debug + PartialEq>(what: &str, expected: T, actual: T) -> Result<()> {
    if expected != actual {
        return Err(mismatch(what, expected, actual));
    }
    Ok(())
}

pub fn skip(reason: impl Into<String>) -> anyhow::Error {
    Stop::Skip(reason.into()).into()
}

static RUN_DEADLINE: OnceLock<Instant> = OnceLock::new();
const LOCK_MARGIN: u64 = 60;
static SCENARIO_DEADLINE: Mutex<Option<Instant>> = Mutex::new(None);

pub fn check() -> Result<()> {
    if let Some(reason) = guard::tripped() {
        bail!(Stop::Aborted(reason));
    }
    let now = Instant::now();
    if RUN_DEADLINE.get().is_some_and(|deadline| now >= *deadline) {
        guard::trip("the run timed out");
        bail!(Stop::Aborted("the run timed out".into()));
    }
    if SCENARIO_DEADLINE.lock().unwrap_or_else(|e| e.into_inner()).is_some_and(|deadline| now >= deadline) {
        bail!(Stop::Timeout("the scenario timed out".into()));
    }
    Ok(())
}

pub fn pause(ms: u64) -> Result<()> {
    let until = Instant::now() + Duration::from_millis(ms);
    loop {
        check()?;
        let now = Instant::now();
        if now >= until {
            return Ok(());
        }
        std::thread::sleep((until - now).min(Duration::from_millis(25)));
    }
}

pub fn wait<T>(what: &str, timeout: Duration, mut probe: impl FnMut() -> Option<T>) -> Result<T> {
    let started = Instant::now();
    loop {
        check()?;
        if let Some(value) = probe() {
            return Ok(value);
        }
        if started.elapsed() >= timeout {
            return Err(Mismatch {
                what: format!("gave up waiting: {what}"),
                expected: format!("{what} within {} s", timeout.as_secs_f32()),
                actual: "the condition never became true".into(),
            }
            .into());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

pub fn poll<T>(timeout: Duration, mut probe: impl FnMut() -> Option<T>) -> Result<Option<T>> {
    let started = Instant::now();
    loop {
        check()?;
        if let Some(value) = probe() {
            return Ok(Some(value));
        }
        if started.elapsed() >= timeout {
            return Ok(None);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

pub struct Layout {
    pub panel: POINT,
    pub helper: RECT,
    pub explorer: RECT,
    pub dpi: u32,
}

pub struct Ctx {
    pub sandbox: Sandbox,
    pub layout: Layout,
    pub reports: PathBuf,
    pub baselines: PathBuf,
    pub config: String,
    pub update: bool,
    pub powershell: Option<PathBuf>,
    pub ffmpeg: Option<PathBuf>,
    pub app: RefCell<Option<app::App>>,
    pub changes: RefCell<Vec<String>>,
}

impl Ctx {
    pub fn stop(&self) {
        if let Some(mut app) = self.app.borrow_mut().take() {
            app.stop();
        }
    }

    pub fn reset(&self) -> Result<()> {
        self.stop();
        self.sandbox.reset_profile()
    }

    pub fn start(
        &self,
        settings: serde_json::Value,
        items: Vec<serde_json::Value>,
        actions: serde_json::Value,
        args: &[&str],
    ) -> Result<()> {
        self.stop();
        app::seed(self, settings, items, actions)?;
        let args: Vec<&OsStr> = args.iter().map(OsStr::new).collect();
        *self.app.borrow_mut() = Some(app::App::launch(self, &args)?);
        Ok(())
    }

    pub fn start_measured(
        &self,
        settings: serde_json::Value,
        items: Vec<serde_json::Value>,
        actions: serde_json::Value,
        frames: &std::path::Path,
    ) -> Result<()> {
        self.stop();
        app::seed(self, settings, items, actions)?;
        *self.app.borrow_mut() = Some(app::App::launch_with(self, &[], Some(frames))?);
        Ok(())
    }

    pub fn app(&self) -> Result<Ref<'_, app::App>> {
        Ref::filter_map(self.app.borrow(), Option::as_ref)
            .map_err(|_| anyhow::anyhow!("the test instance isn't running"))
    }

    pub fn quit(&self) -> Result<()> {
        let mut slot = self.app.borrow_mut();
        let app = slot.as_mut().ok_or_else(|| anyhow::anyhow!("the test instance isn't running"))?;
        let result = app.quit();
        if result.is_ok() {
            slot.take();
        }
        result
    }
}

enum Command {
    Run,
    Baselines,
    Cleanup,
    List,
}

struct Options {
    command: Command,
    suites: Vec<String>,
    scenario_timeout: u64,
    run_timeout: u64,
    countdown: u64,
    text: String,
}

const USAGE: &str = "Traydex E2E: system checks on the real desktop.

Commands (via .\\build.ps1 -E2E <command> ...):
  run [SUITE...]       full run, or only the given suites
  baselines            take or update snapshot baselines (the snapshots suite)
  cleanup              remove leftovers of an interrupted run
  list                 list suites and scenarios
  probe ...            one-off UI check for agents (details: probe --help)

Options for run and baselines (anywhere, e.g. run motion --countdown 3):
  --scenario-timeout S timeout of one scenario, s (default 90)
  --timeout S          timeout of the whole run, s (default 1500)
  --countdown S        pause before taking control, s (default 5, at least 3)

Exit codes: 0 all passed; 1 failures; 2 preflight failed or run stopped; 3 no failures, but some scenarios didn't run.";

fn parse(args: Vec<String>) -> Result<Options> {
    let text = args.join(" ");
    let mut args = args.into_iter();
    let mut command = None;
    let mut options = Options {
        command: Command::List,
        suites: Vec::new(),
        scenario_timeout: 90,
        run_timeout: 1500,
        countdown: 5,
        text,
    };
    while let Some(arg) = args.next() {
        if command.is_none() && !arg.starts_with('-') {
            command = Some(match arg.as_str() {
                "run" => Command::Run,
                "baselines" => Command::Baselines,
                "cleanup" => Command::Cleanup,
                "list" => Command::List,
                other => bail!("unknown command \"{other}\""),
            });
            continue;
        }
        let (name, inline) = match arg.split_once('=') {
            Some((name, value)) => (name.to_string(), Some(value.to_string())),
            None => (arg.clone(), None),
        };
        let mut value = || -> Result<u64> {
            let raw = inline.clone().or_else(|| args.next()).ok_or_else(|| anyhow::anyhow!("no value for {name}"))?;
            Ok(raw.parse()?)
        };
        match name.as_str() {
            "--scenario-timeout" => options.scenario_timeout = value()?.max(5),
            "--timeout" => options.run_timeout = value()?.max(30),
            "--countdown" => options.countdown = value()?.max(3),
            suite if !suite.starts_with('-') && matches!(command, Some(Command::Run)) => {
                if !suites::ALL.iter().any(|s| s.id == suite) {
                    bail!("unknown suite \"{suite}\"");
                }
                options.suites.push(suite.to_string());
            }
            other => bail!("unknown option \"{other}\""),
        }
    }
    options.command = command.ok_or_else(|| anyhow::anyhow!("no command given"))?;
    Ok(options)
}

fn stamp(compact: bool) -> String {
    let t = unsafe { GetLocalTime() };
    if compact {
        format!("{:04}{:02}{:02}-{:02}{:02}{:02}", t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond)
    } else {
        format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond)
    }
}

pub const NO_FFMPEG: &str = "not found in PATH or among Traydex tools";

fn installed_ffmpeg() -> Option<PathBuf> {
    let local = PathBuf::from(std::env::var_os("LOCALAPPDATA")?);
    Some(local.join("Traydex").join("tools").join("ffmpeg.exe")).filter(|path| path.is_file())
}

fn find_on_path(file: &str) -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH")?).map(|dir| dir.join(file)).find(|path| path.is_file())
}

fn layout() -> Layout {
    let (_, work) = win::monitor(POINT { x: 0, y: 0 });
    let dpi = unsafe { windows::Win32::UI::HiDpi::GetDpiForSystem() }.max(96);
    let scaled = |value: i32| value * dpi as i32 / 96;
    let panel_width = scaled(540);
    let panel = POINT { x: (work.right - panel_width - scaled(32)).max(work.left), y: work.top + scaled(32) };
    let helper = RECT {
        left: work.left + scaled(24),
        top: work.top + scaled(24),
        right: work.left + scaled(24 + 420),
        bottom: work.top + scaled(24 + 150),
    };
    let explorer_width = (panel.x - helper.left - scaled(48)).clamp(scaled(360), scaled(760));
    let explorer = RECT {
        left: helper.left,
        top: helper.bottom + scaled(24),
        right: helper.left + explorer_width,
        bottom: (helper.bottom + scaled(24 + 520)).min(work.bottom - scaled(24)),
    };
    Layout { panel, helper, explorer, dpi }
}

struct Isolation {
    files: Vec<(PathBuf, Option<(u64, std::time::SystemTime)>)>,
    registry: Vec<(String, String, Option<String>)>,
}

impl Isolation {
    fn capture() -> Self {
        let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_default().join("Traydex");
        let files = ["shelf.json", "actions.json"]
            .into_iter()
            .map(|name| {
                let path = local.join(name);
                let meta = std::fs::metadata(&path).ok().and_then(|m| Some((m.len(), m.modified().ok()?)));
                (path, meta)
            })
            .collect();
        let registry = [
            ("Software\\Microsoft\\Windows\\CurrentVersion\\Run", "Traydex"),
            ("Software\\Classes\\*\\shell\\Traydex\\command", ""),
            ("Software\\Classes\\Directory\\shell\\Traydex\\command", ""),
        ]
        .into_iter()
        .map(|(key, name)| (key.to_string(), name.to_string(), win::reg_read(key, name)))
        .collect();
        Self { files, registry }
    }
}

fn real_traydex() -> bool {
    win::mutex_exists("Local\\Traydex.Singleton") || win::find_class("Traydex.Relay").is_some()
}

const IMPACTS: &[&str] = &[
    "windows of the test Traydex (\"Traydex Test\"), Explorer (test folders only) and a topmost helper window will open and close;",
    "your mouse and keyboard will be fully blocked until the run ends: moves, clicks and key presses reach no window;",
    "the cursor and keyboard will be driven programmatically (SendInput): moves, clicks, drags, Ctrl+Shift+Alt+F9, Esc, Ctrl+A; window focus will switch;",
    "the clipboard will be overwritten;",
    "a small test picture is moved to your Recycle Bin and restored from it (removed from the Bin if the restore fails);",
    "HKCU temporarily gets a test autostart value \"Traydex Test\" and a context menu item \"TraydexTest\"; both are removed after the run;",
    "test files and the profile are created in %TEMP%\\traydex-e2e and deleted after the run;",
    "your Traydex profile, your files, display and security settings stay untouched, and no internet is used.",
];

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(("probe", rest)) = args.split_first().map(|(first, rest)| (first.as_str(), rest)) {
        unsafe {
            let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        }
        if rest.iter().any(|arg| arg == "--help") {
            println!("{}", probe::USAGE);
            std::process::exit(0);
        }
        std::process::exit(probe::main(rest, layout()));
    }
    let options = match parse(args) {
        Ok(options) => options,
        Err(error) => {
            eprintln!("{error}\n\n{USAGE}");
            std::process::exit(2);
        }
    };
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    let code = match options.command {
        Command::List => {
            for suite in suites::ALL {
                println!("{} — {}", suite.id, suite.title);
                for spec in (suite.specs)() {
                    let mark = if spec.desktop { " [drives the desktop]" } else { "" };
                    println!("    {} — {}{mark}", spec.id, spec.title);
                }
            }
            println!("isolation: runs at the end of every run");
            println!("    real-profile-untouched: the real Traydex profile and registry keys didn't change");
            0
        }
        Command::Cleanup => {
            let problems = sandbox::cleanup(&Sandbox::new());
            if problems.is_empty() {
                println!("Cleanup done: test processes, registry keys and folders are gone.");
                0
            } else {
                for problem in &problems {
                    println!("Failed: {problem}");
                }
                2
            }
        }
        Command::Run | Command::Baselines => run(options),
    };
    std::process::exit(code);
}

fn run(options: Options) -> i32 {
    let update = matches!(options.command, Command::Baselines);
    let exe_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)).unwrap_or_default();
    let reports = exe_dir.parent().unwrap_or(&exe_dir).join("e2e").join("reports").join(stamp(true));
    let mut report =
        Report { command: format!("traydex-e2e {}", options.text), started: stamp(false), ..Report::default() };
    let finish = |mut report: Report, reports: &Path| -> i32 {
        report.finished = stamp(false);
        report.exit_code = report.code();
        let written = report::write(reports, &report);
        println!(
            "\nResult: {} passed, {} failed, {} not run. Exit code {}.",
            report.count(Status::Passed),
            report.count(Status::Failed),
            report.count(Status::NotExecuted),
            report.exit_code
        );
        if let Some(stopped) = &report.stopped {
            println!("Run stopped: {stopped}");
        }
        match written {
            Ok(()) => println!("Report: {}", reports.join("report.md").display()),
            Err(error) => println!("Couldn't write the report: {error:#}"),
        }
        report.exit_code
    };
    let selected: Vec<&suites::Suite> = suites::ALL
        .iter()
        .filter(|suite| {
            if update {
                suite.id == "snapshots"
            } else {
                options.suites.is_empty() || options.suites.iter().any(|id| id == suite.id)
            }
        })
        .collect();
    let preflight_fail = |mut report: Report, reason: String| -> i32 {
        println!("Preflight failed: {reason}");
        report.stopped = Some(format!("preflight: {reason}"));
        finish(report, &reports)
    };

    let _runner = unsafe { CreateMutexW(None, true, w!("Local\\Traydex.E2E.Runner")) };
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        return preflight_fail(report, "another E2E run is in progress".into());
    }
    let source = exe_dir.join("traydex.exe");
    if !source.is_file() {
        return preflight_fail(report, format!("{} not found", source.display()));
    }
    let sandbox = Sandbox::new();
    let leftovers = sandbox::leftovers(&sandbox);
    if !leftovers.is_empty() {
        report.cleanup.push(format!("leftovers of a previous run found before start: {}", leftovers.join(", ")));
        report.cleanup.extend(sandbox::cleanup(&sandbox).into_iter().map(|p| format!("couldn't remove: {p}")));
    }
    report.preflight.push(format!("test instance: {}", source.display()));
    if !win::input_desktop_available() {
        return preflight_fail(
            report,
            "the desktop is unavailable (the screen is locked or a secure desktop is open)".into(),
        );
    }
    if !win::hotkey_free(MOD_CONTROL | MOD_SHIFT | MOD_ALT, VK_F9) {
        return preflight_fail(report, "the test shortcut Ctrl+Shift+Alt+F9 is taken by another app".into());
    }
    report.preflight.push("the test shortcut Ctrl+Shift+Alt+F9 is free".into());
    let real = real_traydex();
    let desktop: Vec<String> = selected
        .iter()
        .flat_map(|suite| {
            (suite.specs)().into_iter().filter(|spec| spec.desktop).map(move |spec| format!("{}/{}", suite.id, spec.id))
        })
        .collect();
    if real && !desktop.is_empty() {
        return preflight_fail(
            report,
            "your Traydex is running: its drag detection, hot corner and shelf would interfere with input checks and could catch test files. \
             Quit it (tray icon → Quit) or run only the suites that don't drive the desktop: run lifecycle snapshots shell"
                .into(),
        );
    }
    report.preflight.push(if real {
        "your Traydex is running; only suites without input control are selected, so it isn't affected".into()
    } else {
        "your Traydex isn't running".into()
    });
    let windows_dir = std::env::var_os("SystemRoot").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("C:\\Windows"));
    let powershell =
        Some(windows_dir.join("System32\\WindowsPowerShell\\v1.0\\powershell.exe")).filter(|p| p.is_file());
    let ffmpeg = find_on_path("ffmpeg.exe").or_else(installed_ffmpeg);
    report
        .preflight
        .push(format!("PowerShell: {}", powershell.as_ref().map_or("not found".into(), |p| p.display().to_string())));
    report.preflight.push(format!("ffmpeg: {}", ffmpeg.as_ref().map_or(NO_FFMPEG.into(), |p| p.display().to_string())));
    if !windows_dir.join("explorer.exe").is_file() {
        return preflight_fail(report, "explorer.exe not found".into());
    }

    let layout = layout();
    let config = snap::config(layout.dpi);
    let baselines = Path::new(env!("CARGO_MANIFEST_DIR")).join("baselines").join(&config);
    report.environment.push(("Snapshot configuration".into(), config.clone()));
    report.environment.push(("Scale".into(), format!("{}%", layout.dpi * 100 / 96)));
    report.environment.push(("Sandbox".into(), sandbox.root.display().to_string()));
    report
        .environment
        .push(("Timeouts".into(), format!("scenario {} s, run {} s", options.scenario_timeout, options.run_timeout)));

    if let Err(error) = guard::start() {
        return preflight_fail(report, format!("{error:#}"));
    }
    println!("Traydex E2E: {}", if update { "updating snapshot baselines" } else { "system run" });
    println!("Suites: {}", selected.iter().map(|s| s.id).collect::<Vec<_>>().join(", "));
    println!("\nDuring the run:");
    for impact in IMPACTS {
        println!("  • {impact}");
    }
    println!(
        "\nStop at any time: {} stops the run with cleanup and gives input back immediately, exit code 2.",
        guard::STOP_CHORD
    );
    println!("Ctrl+Alt+Del also gives input back in an emergency. The lock starts after the countdown.\n");
    for left in (1..=options.countdown).rev() {
        print!("\rStarting in {left} s... ");
        let _ = std::io::Write::flush(&mut std::io::stdout());
        let until = Instant::now() + Duration::from_secs(1);
        while Instant::now() < until {
            if guard::tripped().is_some() {
                println!("\nCancelled before start, nothing changed.");
                guard::stop();
                report.stopped = Some("cancelled by the user before start".into());
                return finish(report, &reports);
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    println!("\rRun started.                   ");
    guard::arm(Duration::from_secs(options.run_timeout + LOCK_MARGIN));
    let _ = RUN_DEADLINE.set(Instant::now() + Duration::from_secs(options.run_timeout));
    let watchdog_root = sandbox.root.clone();
    let watchdog_limit = Duration::from_secs(options.run_timeout + 90);
    std::thread::spawn(move || {
        std::thread::sleep(watchdog_limit);
        guard::trip("watchdog: the run hung");
        std::thread::sleep(Duration::from_secs(20));
        input::release_all();
        let sandbox = Sandbox {
            profile: watchdog_root.join("profile"),
            files: watchdog_root.join("files"),
            exe: watchdog_root.join("bin").join("traydex.exe"),
            root: watchdog_root,
        };
        let _ = sandbox::cleanup(&sandbox);
        eprintln!("Watchdog: the run hung and was stopped, leftovers removed. Exit code 2.");
        std::process::exit(2);
    });
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE);
    }
    input::allow_pid(std::process::id());
    if let Err(error) = sandbox.prepare(&source).and_then(|_| helper::start(layout.helper).map(|_| ())) {
        report.stopped = Some(format!("couldn't prepare the run: {error:#}"));
        guard::stop();
        report.cleanup.extend(sandbox::cleanup(&sandbox));
        return finish(report, &reports);
    }
    let isolation = Isolation::capture();
    let ctx = Ctx {
        sandbox,
        layout,
        reports: reports.clone(),
        baselines,
        config,
        update,
        powershell,
        ffmpeg,
        app: RefCell::new(None),
        changes: RefCell::new(Vec::new()),
    };
    for suite in &selected {
        helper::status(&format!("Suite: {}", suite.title));
        println!("\n== {} ==", suite.title);
        let entries = suites::execute(&ctx, suite, options.scenario_timeout);
        report.entries.extend(entries);
        ctx.stop();
        input::release_all();
    }
    report.entries.push(suites::isolation(&ctx, &isolation.files, &isolation.registry, real));
    report.stopped = guard::tripped();
    input::release_all();
    ctx.stop();
    helper::stop();
    guard::stop();
    report.baselines = ctx.changes.take();
    if update {
        println!(
            "
Baselines ({}):",
            ctx.config
        );
        for line in &report.baselines {
            println!("  {line}");
        }
    }
    report.cleanup.extend(sandbox::cleanup(&ctx.sandbox).into_iter().map(|p| format!("failed: {p}")));
    report.cleanup.push("the clipboard was overwritten with test data (by design)".into());
    finish(report, &reports)
}
