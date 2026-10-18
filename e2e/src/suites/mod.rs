mod dnd;
mod files;
mod input;
mod lifecycle;
mod motion;
mod shell;
mod snapshots;

use crate::report::{Entry, Status};
use crate::{Ctx, Mismatch, SCENARIO_DEADLINE, Stop, guard, helper, win};
use anyhow::Result;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

pub struct Spec {
    pub id: String,
    pub title: String,
    pub desktop: bool,
}

pub fn spec(id: &str, title: &str, desktop: bool) -> Spec {
    Spec { id: id.into(), title: title.into(), desktop }
}

pub struct Suite {
    pub id: &'static str,
    pub title: &'static str,
    pub specs: fn() -> Vec<Spec>,
    pub run: fn(&mut Session),
}

pub const ALL: &[Suite] =
    &[lifecycle::SUITE, dnd::SUITE, snapshots::SUITE, input::SUITE, shell::SUITE, files::SUITE, motion::SUITE];

pub struct Session<'a> {
    pub ctx: &'a Ctx,
    suite: &'static Suite,
    specs: Vec<Spec>,
    entries: Vec<Entry>,
    timeout: u64,
}

impl Session<'_> {
    pub fn run(&mut self, id: &str, body: impl FnOnce(&Ctx) -> Result<()>) {
        let Some(spec) = self.specs.iter().find(|spec| spec.id == id) else {
            return;
        };
        let mut entry = Entry {
            suite: self.suite.id.into(),
            id: spec.id.clone(),
            title: spec.title.clone(),
            status: Status::NotExecuted,
            reason: None,
            expected: None,
            actual: None,
            seconds: 0.,
            artifacts: Vec::new(),
        };
        if let Some(reason) = guard::tripped() {
            entry.reason = Some(format!("run stopped: {reason}"));
            self.entries.push(entry);
            return;
        }
        helper::status(&format!("{}: {}", self.suite.title, spec.title));
        print!("  {} … ", spec.id);
        let _ = std::io::Write::flush(&mut std::io::stdout());
        *SCENARIO_DEADLINE.lock().unwrap_or_else(|e| e.into_inner()) =
            Some(Instant::now() + Duration::from_secs(self.timeout));
        let started = Instant::now();
        let result = body(self.ctx);
        *SCENARIO_DEADLINE.lock().unwrap_or_else(|e| e.into_inner()) = None;
        crate::input::release_all();
        helper::disarm();
        entry.seconds = started.elapsed().as_secs_f64();
        match result {
            Ok(()) => entry.status = Status::Passed,
            Err(error) => {
                if let Some(stop) = error.downcast_ref::<Stop>() {
                    match stop {
                        Stop::Skip(reason) => entry.reason = Some(reason.clone()),
                        Stop::Aborted(reason) => entry.reason = Some(format!("run stopped: {reason}")),
                        Stop::Timeout(reason) => {
                            entry.status = Status::Failed;
                            entry.reason = Some(reason.clone());
                        }
                    }
                } else {
                    entry.status = Status::Failed;
                    if let Some(mismatch) = error.downcast_ref::<Mismatch>() {
                        entry.reason = Some(mismatch.what.clone());
                        entry.expected = Some(mismatch.expected.clone());
                        entry.actual = Some(mismatch.actual.clone());
                    } else {
                        entry.reason = Some(format!("{error:#}"));
                    }
                }
                if entry.status == Status::Failed {
                    entry.artifacts.extend(self.collect(&spec.id));
                }
            }
        }
        println!(
            "{}{}",
            match entry.status {
                Status::Passed => "passed",
                Status::Failed => "FAILED",
                Status::NotExecuted => "not run",
            },
            entry.reason.as_ref().map(|r| format!(" — {r}")).unwrap_or_default()
        );
        self.entries.push(entry);
    }

    fn collect(&self, id: &str) -> Vec<String> {
        let folder = format!("{}-{id}", self.suite.id);
        let dir = artifacts(self.ctx, self.suite.id, id);
        let mut saved: Vec<String> = fs::read_dir(&dir)
            .map(|entries| entries.flatten().map(|e| format!("{folder}/{}", e.file_name().to_string_lossy())).collect())
            .unwrap_or_default();
        if let Ok(app) = self.ctx.app()
            && let Some(panel) = app.panel().filter(|panel| win::visible(*panel))
            && let Some(image) = win::capture(panel)
            && image.save(dir.join("window.png")).is_ok()
        {
            saved.push(format!("{folder}/window.png"));
        }
        if let Ok(entries) = fs::read_dir(self.ctx.sandbox.data().join("logs")) {
            for entry in entries.flatten() {
                if fs::copy(entry.path(), dir.join(entry.file_name())).is_ok() {
                    saved.push(format!("{folder}/{}", entry.file_name().to_string_lossy()));
                }
            }
        }
        saved
    }
}

pub fn artifacts(ctx: &Ctx, suite: &str, id: &str) -> PathBuf {
    let dir = ctx.reports.join(format!("{suite}-{id}"));
    let _ = fs::create_dir_all(&dir);
    dir
}

pub fn execute(ctx: &Ctx, suite: &'static Suite, timeout: u64) -> Vec<Entry> {
    let mut session = Session { ctx, suite, specs: (suite.specs)(), entries: Vec::new(), timeout };
    (suite.run)(&mut session);
    let mut entries = std::mem::take(&mut session.entries);
    for spec in &session.specs {
        if !entries.iter().any(|entry| entry.id == spec.id) {
            entries.push(Entry {
                suite: suite.id.into(),
                id: spec.id.clone(),
                title: spec.title.clone(),
                status: Status::NotExecuted,
                reason: Some(
                    guard::tripped().map_or("the scenario didn't run".into(), |r| format!("run stopped: {r}")),
                ),
                expected: None,
                actual: None,
                seconds: 0.,
                artifacts: Vec::new(),
            });
        }
    }
    entries
}

pub fn isolation(
    ctx: &Ctx,
    files: &[(PathBuf, Option<(u64, SystemTime)>)],
    registry: &[(String, String, Option<String>)],
    real: bool,
) -> Entry {
    let mut entry = Entry {
        suite: "isolation".into(),
        id: "real-profile-untouched".into(),
        title: "The real Traydex profile and registry keys didn't change".into(),
        status: Status::Passed,
        reason: None,
        expected: None,
        actual: None,
        seconds: 0.,
        artifacts: Vec::new(),
    };
    let _ = ctx;
    let mut changed = Vec::new();
    for (key, name, before) in registry {
        let after = win::reg_read(key, name);
        if &after != before {
            changed.push(format!("HKCU\\{key} \"{name}\": was {before:?}, now {after:?}"));
        }
    }
    if !real {
        for (path, before) in files {
            let after = fs::metadata(path).ok().and_then(|m| Some((m.len(), m.modified().ok()?)));
            if &after != before {
                changed.push(format!("{} changed", path.display()));
            }
        }
    }
    if !changed.is_empty() {
        entry.status = Status::Failed;
        entry.reason = Some("the test run touched the real profile".into());
        entry.actual = Some(changed.join("; "));
        entry.expected = Some("unchanged".into());
    } else if real {
        entry.reason = Some(
            "profile files weren't compared: your Traydex is running and may change them itself; the registry didn't change".into(),
        );
    }
    println!(
        "  isolation/real-profile-untouched … {}",
        if entry.status == Status::Passed { "passed" } else { "FAILED" }
    );
    entry
}

pub fn write(path: &Path, bytes: &[u8]) -> Result<PathBuf> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, bytes)?;
    Ok(path.to_path_buf())
}

pub fn pattern(seed: u32, length: usize) -> Vec<u8> {
    let mut state = seed.wrapping_mul(2_654_435_761).max(1);
    (0..length)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state as u8
        })
        .collect()
}

pub fn same_file(a: &Path, b: &Path) -> bool {
    matches!((fs::read(a), fs::read(b)), (Ok(x), Ok(y)) if x == y)
}

pub fn none() -> serde_json::Value {
    serde_json::json!([])
}
