use crate::{app, explorer, win};
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::Duration;

pub const PREFIX: &str = "traydex-e2e-";
const RUN: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const TEST_NAME: &str = "Traydex Test";
const TEST_MENUS: [&str; 2] =
    ["Software\\Classes\\*\\shell\\TraydexTest", "Software\\Classes\\Directory\\shell\\TraydexTest"];

pub struct Sandbox {
    pub root: PathBuf,
    pub profile: PathBuf,
    pub files: PathBuf,
    pub exe: PathBuf,
}

impl Sandbox {
    pub fn new() -> Self {
        let root = std::env::temp_dir().join("traydex-e2e");
        Self {
            profile: root.join("profile"),
            files: root.join("files"),
            exe: root.join("bin").join("traydex.exe"),
            root,
        }
    }

    pub fn data(&self) -> PathBuf {
        self.profile.join("Traydex")
    }

    pub fn prepare(&self, source: &Path) -> Result<()> {
        fs::create_dir_all(self.exe.parent().unwrap_or(&self.root))?;
        fs::create_dir_all(&self.files)?;
        fs::copy(source, &self.exe).with_context(|| format!("couldn't copy {}", source.display()))?;
        Ok(())
    }

    pub fn reset_profile(&self) -> Result<()> {
        remove(&self.profile)?;
        fs::create_dir_all(&self.profile)?;
        Ok(())
    }

    pub fn folder(&self, name: &str) -> Result<PathBuf> {
        let path = self.files.join(format!("{PREFIX}{name}"));
        remove(&path)?;
        fs::create_dir_all(&path)?;
        Ok(path)
    }

    pub fn autostart_value() -> Option<String> {
        win::reg_read(RUN, TEST_NAME)
    }

    pub fn menu_command(index: usize) -> Option<String> {
        win::reg_read(&format!("{}\\command", TEST_MENUS[index]), "")
    }
}

pub fn remove(path: &Path) -> Result<()> {
    let mut last = None;
    for _ in 0..30 {
        match fs::remove_dir_all(path) {
            Ok(()) => return Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => last = Some(error),
        }
        sleep(Duration::from_millis(100));
    }
    Err(last.map(anyhow::Error::from).unwrap_or_else(|| anyhow::anyhow!("couldn't delete")))
        .with_context(|| format!("couldn't delete {}", path.display()))
}

pub fn leftovers(sandbox: &Sandbox) -> Vec<String> {
    let mut found = Vec::new();
    if !win::processes_under(&sandbox.root).is_empty() {
        found.push("running test instances".into());
    }
    if Sandbox::autostart_value().is_some() {
        found.push(format!("autostart value \"{TEST_NAME}\""));
    }
    for key in TEST_MENUS {
        if win::reg_key_exists(key) {
            found.push(format!("registry key HKCU\\{key}"));
        }
    }
    if sandbox.root.exists() {
        found.push(format!("folder {}", sandbox.root.display()));
    }
    found
}

pub fn cleanup(sandbox: &Sandbox) -> Vec<String> {
    let mut problems = Vec::new();
    for hwnd in win::windows() {
        if win::class(hwnd) == explorer::CLASS && win::title(hwnd).starts_with(PREFIX) {
            win::close(hwnd);
        }
        if win::class(hwnd) == explorer::CLASS && Path::new(&win::title(hwnd)).starts_with(&sandbox.root) {
            win::close(hwnd);
        }
    }
    let ours = win::processes_under(&sandbox.root);
    for pid in &ours {
        if let Some(relay) = app::relay_of(*pid) {
            win::post(relay, windows::Win32::UI::WindowsAndMessaging::WM_APP + 5, 0, 0);
        }
    }
    for _ in 0..60 {
        if ours.iter().all(|pid| !win::alive(*pid)) {
            break;
        }
        sleep(Duration::from_millis(50));
    }
    for pid in win::processes_under(&sandbox.root) {
        win::terminate(pid);
    }
    sleep(Duration::from_millis(200));
    win::reg_delete_value(RUN, TEST_NAME);
    for key in TEST_MENUS {
        win::reg_delete_tree(key);
    }
    if let Err(error) = remove(&sandbox.root) {
        problems.push(format!("{error:#}"));
    }
    for leftover in leftovers(sandbox) {
        problems.push(format!("left over: {leftover}"));
    }
    problems
}
