use super::child::NO_WINDOW;
use crate::actions::{CONNECT_TIMEOUT, curl_exe, curl_failure};
use crate::i18n::tr;
use crate::system::Progress;
use anyhow::{Result, bail};
use std::io::Read;
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::Duration;

const LOG_LIMIT: usize = 4096;
const CANCEL_POLL: Duration = Duration::from_millis(100);

pub fn download(url: &str, target: &Path, progress: &Progress) -> Result<()> {
    let mut child = Command::new(curl_exe())
        .args(["-fL", "--connect-timeout", CONNECT_TIMEOUT, "--progress-bar", "-o"])
        .arg(target)
        .arg(url)
        .creation_flags(NO_WINDOW)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()?;
    let _tether = super::child::tether(&child);
    let log = Arc::new(Mutex::new(Vec::new()));
    let reader = child.stderr.take().map(|mut stderr| {
        let log = log.clone();
        let progress = progress.clone();
        thread::spawn(move || {
            let mut buffer = [0u8; 512];
            while let Ok(read) = stderr.read(&mut buffer) {
                if read == 0 {
                    break;
                }
                let mut log = log.lock().unwrap_or_else(PoisonError::into_inner);
                log.extend_from_slice(&buffer[..read]);
                if let Some(fraction) = percent(&log) {
                    progress.update(|stage| stage.fraction = Some(fraction));
                }
                if log.len() > LOG_LIMIT {
                    let excess = log.len() - LOG_LIMIT / 4;
                    log.drain(..excess);
                }
            }
        })
    });
    let status = loop {
        if progress.cancelled() {
            let _ = child.kill();
            let _ = child.wait();
            if let Some(reader) = reader {
                let _ = reader.join();
            }
            bail!(tr().operation_cancelled);
        }
        if let Some(status) = child.try_wait()? {
            break status;
        }
        thread::sleep(CANCEL_POLL);
    };
    if let Some(reader) = reader {
        let _ = reader.join();
    }
    if !status.success() {
        let log = log.lock().unwrap_or_else(PoisonError::into_inner);
        bail!(curl_failure(status.code(), &log));
    }
    Ok(())
}

fn percent(log: &[u8]) -> Option<f32> {
    let text = String::from_utf8_lossy(log);
    let end = text.rfind('%')?;
    let number: String = text
        .get(..end)?
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    number.parse::<f32>().ok().map(|value| value / 100.)
}

#[cfg(test)]
mod tests;
