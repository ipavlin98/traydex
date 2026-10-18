use anyhow::Result;
use serde::Serialize;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

#[derive(Serialize, Clone, Copy, PartialEq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Passed,
    Failed,
    NotExecuted,
}

#[derive(Serialize, Clone, Debug)]
pub struct Entry {
    pub suite: String,
    pub id: String,
    pub title: String,
    pub status: Status,
    pub reason: Option<String>,
    pub expected: Option<String>,
    pub actual: Option<String>,
    pub seconds: f64,
    pub artifacts: Vec<String>,
}

#[derive(Serialize, Default)]
pub struct Report {
    pub command: String,
    pub started: String,
    pub finished: String,
    pub exit_code: i32,
    pub stopped: Option<String>,
    pub environment: Vec<(String, String)>,
    pub preflight: Vec<String>,
    pub entries: Vec<Entry>,
    pub baselines: Vec<String>,
    pub cleanup: Vec<String>,
}

impl Report {
    pub fn count(&self, status: Status) -> usize {
        self.entries.iter().filter(|entry| entry.status == status).count()
    }

    pub fn code(&self) -> i32 {
        if self.stopped.is_some() {
            2
        } else if self.count(Status::Failed) > 0 {
            1
        } else if self.count(Status::NotExecuted) > 0 {
            3
        } else {
            0
        }
    }
}

fn cell(text: &str) -> String {
    text.replace('|', "\\|").replace('\n', "<br>")
}

pub fn write(dir: &Path, report: &Report) -> Result<()> {
    fs::create_dir_all(dir)?;
    fs::write(dir.join("report.json"), serde_json::to_vec_pretty(report)?)?;
    let mut md = String::new();
    let _ = writeln!(md, "# Traydex E2E report\n");
    let _ = writeln!(md, "- Command: `{}`", report.command);
    let _ = writeln!(md, "- Started: {}, finished: {}", report.started, report.finished);
    let _ = writeln!(md, "- Exit code: **{}**", report.exit_code);
    let _ = writeln!(
        md,
        "- Result: {} passed, {} failed, {} not run",
        report.count(Status::Passed),
        report.count(Status::Failed),
        report.count(Status::NotExecuted)
    );
    if let Some(stopped) = &report.stopped {
        let _ = writeln!(md, "- **Run stopped:** {stopped}");
    }
    let _ = writeln!(md, "\n## Environment\n");
    for (key, value) in &report.environment {
        let _ = writeln!(md, "- {key}: {value}");
    }
    if !report.preflight.is_empty() {
        let _ = writeln!(md, "\n## Preflight\n");
        for line in &report.preflight {
            let _ = writeln!(md, "- {line}");
        }
    }
    let _ = writeln!(md, "\n## Scenarios\n");
    let _ = writeln!(md, "| Suite | Scenario | Status | Time, s | Reason |");
    let _ = writeln!(md, "|---|---|---|---|---|");
    for entry in &report.entries {
        let status = match entry.status {
            Status::Passed => "passed",
            Status::Failed => "**failed**",
            Status::NotExecuted => "not run",
        };
        let _ = writeln!(
            md,
            "| {} | {} — {} | {} | {:.1} | {} |",
            entry.suite,
            entry.id,
            cell(&entry.title),
            status,
            entry.seconds,
            cell(entry.reason.as_deref().unwrap_or(""))
        );
    }
    let failures: Vec<_> = report.entries.iter().filter(|entry| entry.status == Status::Failed).collect();
    if !failures.is_empty() {
        let _ = writeln!(md, "\n## Failures\n");
        for entry in failures {
            let _ = writeln!(md, "### {}/{} — {}\n", entry.suite, entry.id, entry.title);
            if let Some(reason) = &entry.reason {
                let _ = writeln!(md, "- Reason: {reason}");
            }
            if let Some(expected) = &entry.expected {
                let _ = writeln!(md, "- Expected: {expected}");
            }
            if let Some(actual) = &entry.actual {
                let _ = writeln!(md, "- Actual: {actual}");
            }
            for artifact in &entry.artifacts {
                let _ = writeln!(md, "- File: [{artifact}]({})", artifact.replace(' ', "%20"));
            }
            let _ = writeln!(md);
        }
    }
    if !report.baselines.is_empty() {
        let _ = writeln!(md, "\n## Baselines\n");
        for line in &report.baselines {
            let _ = writeln!(md, "- {line}");
        }
    }
    if !report.cleanup.is_empty() {
        let _ = writeln!(md, "\n## Cleanup\n");
        for line in &report.cleanup {
            let _ = writeln!(md, "- {line}");
        }
    }
    fs::write(dir.join("report.md"), md)?;
    Ok(())
}
