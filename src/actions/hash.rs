use super::upload::{curl, exchange};
use super::{Algorithm, Outcome, Progress, Show, base};
use crate::i18n::{fill, tr};
use crate::system::Services;
use anyhow::{Result, bail};
use md5::Md5;
use sha2::{Digest as _, Sha256};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

const CHUNK: usize = 1 << 20;
const VIRUSTOTAL_FILES: &str = "https://www.virustotal.com/api/v3/files/";
const VIRUSTOTAL_REPORT: &str = "https://www.virustotal.com/gui/file/";

#[derive(Clone, Debug, PartialEq)]
pub enum Verdict {
    Checking,
    Clean { total: u64 },
    Flagged { detected: u64, total: u64 },
    Unknown,
    Failed(String),
}

impl Verdict {
    pub fn summary(&self) -> String {
        match self {
            Verdict::Checking => tr().vt_checking.into(),
            Verdict::Clean { total } => fill(tr().vt_clean, format!("0/{total}")),
            Verdict::Flagged { detected, total } => fill(tr().vt_flagged, format!("{detected}/{total}")),
            Verdict::Unknown => tr().vt_unknown.into(),
            Verdict::Failed(reason) => fill(tr().vt_failed, reason),
        }
    }

    pub fn reported(&self) -> bool {
        matches!(self, Verdict::Clean { .. } | Verdict::Flagged { .. })
    }
}

#[derive(Clone, Debug)]
pub struct Digest {
    pub path: PathBuf,
    pub sha256: String,
    pub md5: String,
    pub verdict: Option<Verdict>,
}

pub struct Lookup {
    key: String,
}

impl Lookup {
    pub fn verdicts(&self, digests: &[Digest], progress: &Progress, system: &dyn Services) -> Vec<Verdict> {
        let mut quota = None;
        digests
            .iter()
            .map(|digest| {
                let verdict = quota.clone().unwrap_or_else(|| lookup(&digest.sha256, &self.key, progress, system));
                if progress.cancelled() {
                    return Verdict::Failed(tr().stopped.into());
                }
                if verdict == Verdict::Failed(tr().vt_quota.into()) {
                    quota = Some(verdict.clone());
                }
                verdict
            })
            .collect()
    }
}

impl Digest {
    pub fn get(&self, algorithm: Algorithm) -> &str {
        match algorithm {
            Algorithm::Sha256 => &self.sha256,
            Algorithm::Md5 => &self.md5,
        }
    }

    pub fn report(&self) -> String {
        format!("{VIRUSTOTAL_REPORT}{}", self.sha256)
    }
}

pub(super) fn hashes(files: &[PathBuf], algorithm: Algorithm, key: &str, progress: &Progress) -> Result<Outcome> {
    let files: Vec<&PathBuf> = files.iter().filter(|file| file.is_file()).collect();
    if files.is_empty() {
        bail!(tr().needs_file_input);
    }
    let total: u64 = files.iter().map(|file| fs::metadata(file).map_or(0, |meta| meta.len())).sum();
    let mut done = 0;
    let mut digests = Vec::new();
    for file in &files {
        let (sha256, md5) = digest(file, |read| {
            done += read;
            progress.fraction(done, total);
            !progress.cancelled()
        })?;
        digests.push(Digest { path: file.to_path_buf(), sha256, md5, verdict: None });
    }
    let key = key.trim();
    let lookup = (!key.is_empty()).then(|| Lookup { key: key.to_string() });
    if lookup.is_some() {
        for digest in &mut digests {
            digest.verdict = Some(Verdict::Checking);
        }
    }
    let clipboard = match digests.as_slice() {
        [digest] => digest.get(algorithm).to_string(),
        digests => digests
            .iter()
            .map(|digest| format!("{}  {}", digest.get(algorithm), base(&digest.path)))
            .collect::<Vec<_>>()
            .join("\r\n"),
    };
    let message = match digests.as_slice() {
        [_] => fill(tr().hash_copied, algorithm.label()),
        digests => fill(tr().hashes_copied, digests.len()),
    };
    Ok(Outcome { message, created: Vec::new(), clipboard: Some(clipboard), show: Some(Show::Hashes(digests, lookup)) })
}

fn digest(file: &Path, mut read: impl FnMut(u64) -> bool) -> Result<(String, String)> {
    let mut source = File::open(file)?;
    let (mut sha256, mut md5) = (Sha256::new(), Md5::new());
    let mut buffer = vec![0; CHUNK];
    loop {
        let count = source.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        sha256.update(&buffer[..count]);
        md5.update(&buffer[..count]);
        if !read(count as u64) {
            bail!(tr().stopped);
        }
    }
    Ok((format!("{:x}", sha256.finalize()), format!("{:x}", md5.finalize())))
}

fn lookup(sha256: &str, key: &str, progress: &Progress, system: &dyn Services) -> Verdict {
    let mut command = curl();
    command.args(["-H", "@-"]).arg(format!("{VIRUSTOTAL_FILES}{sha256}"));
    match exchange(&mut command, format!("x-apikey: {key}\n").into_bytes(), progress, system) {
        Ok((body, status)) => verdict(&status, &body),
        Err(error) => Verdict::Failed(format!("{error:#}")),
    }
}

fn verdict(status: &str, body: &str) -> Verdict {
    match status {
        "200" => {
            let value: serde_json::Value = serde_json::from_str(body).unwrap_or_default();
            let Some(stats) = value.pointer("/data/attributes/last_analysis_stats") else {
                return Verdict::Unknown;
            };
            let count = |name: &str| stats.get(name).and_then(serde_json::Value::as_u64).unwrap_or(0);
            let detected = count("malicious") + count("suspicious");
            let total = detected + count("undetected") + count("harmless");
            match (detected, total) {
                (_, 0) => Verdict::Unknown,
                (0, total) => Verdict::Clean { total },
                (detected, total) => Verdict::Flagged { detected, total },
            }
        }
        "404" => Verdict::Unknown,
        "401" | "403" => Verdict::Failed(tr().key_rejected.into()),
        "429" => Verdict::Failed(tr().vt_quota.into()),
        status => Verdict::Failed(format!("HTTP {status}")),
    }
}

#[cfg(test)]
mod tests;
