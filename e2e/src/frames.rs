use crate::{mismatch, win};
use anyhow::Result;
use std::fs;
use std::path::Path;

pub const FRAME: &str = "frame at: ";
pub const OPACITY: &str = "opacity at: ";
const TARGET_FPS: f64 = 120.;
const BURST_GAP_MS: f64 = 100.;

pub struct Frames {
    durations: Vec<f64>,
    intervals: Vec<f64>,
}

impl Frames {
    pub fn read(path: &Path, from: u64, stamp: &str) -> Frames {
        let text = fs::read(path).unwrap_or_default();
        let text = String::from_utf8_lossy(text.get(from as usize..).unwrap_or_default());
        let durations =
            text.lines().filter_map(|line| line.strip_prefix("frame duration: ")).filter_map(millis).collect();
        let same_frame = refresh_period() / 2.;
        let mut stamps: Vec<f64> = Vec::new();
        for at in text
            .lines()
            .filter_map(|line| line.strip_prefix(stamp))
            .filter_map(|micros| micros.trim().parse::<f64>().ok())
            .map(|micros| micros / 1000.)
        {
            if stamps.last().is_none_or(|last| at - last >= same_frame) {
                stamps.push(at);
            }
        }
        let intervals = stamps
            .windows(2)
            .filter_map(|pair| match pair {
                [earlier, later] => Some(later - earlier),
                _ => None,
            })
            .filter(|interval| *interval < BURST_GAP_MS)
            .collect();
        Frames { durations, intervals }
    }

    fn rate(&self) -> f64 {
        let total: f64 = self.intervals.iter().sum();
        if total <= 0. { 0. } else { self.intervals.len() as f64 * 1000. / total }
    }

    pub fn summary(&self) -> String {
        format!(
            "{} frames, render: median {:.1} ms, p95 {:.1} ms, max {:.1} ms; rate {:.0} FPS, interval p95 {:.1} ms, max {:.1} ms",
            self.durations.len(),
            percentile(&self.durations, 0.5),
            percentile(&self.durations, 0.95),
            percentile(&self.durations, 1.),
            self.rate(),
            percentile(&self.intervals, 0.95),
            percentile(&self.intervals, 1.)
        )
    }

    pub fn within_budget(&self, what: &str) -> Result<()> {
        let target = win::refresh_rate().map_or(TARGET_FPS, |hz| TARGET_FPS.min(f64::from(hz)));
        let budget = 1000. / target;
        let expected = format!("≥ {target:.0} FPS, interval and render p95 ≤ {budget:.1} ms");
        if self.rate() < target
            || percentile(&self.intervals, 0.95) > budget
            || percentile(&self.durations, 0.95) > budget
        {
            return Err(mismatch(what, expected, self.summary()));
        }
        Ok(())
    }
}

pub fn offset(path: &Path) -> u64 {
    fs::metadata(path).map(|m| m.len()).unwrap_or_default()
}

fn refresh_period() -> f64 {
    1000. / win::refresh_rate().map_or(TARGET_FPS, f64::from)
}

fn percentile(values: &[f64], share: f64) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let index = ((sorted.len() as f64 - 1.) * share).round() as usize;
    sorted.get(index).copied().unwrap_or_default()
}

fn millis(value: &str) -> Option<f64> {
    let value = value.trim();
    if let Some(ms) = value.strip_suffix("ms") {
        return ms.parse().ok();
    }
    if let Some(us) = value.strip_suffix("µs") {
        return us.parse::<f64>().ok().map(|us| us / 1000.);
    }
    if let Some(ns) = value.strip_suffix("ns") {
        return ns.parse::<f64>().ok().map(|ns| ns / 1_000_000.);
    }
    value.strip_suffix('s').and_then(|s| s.parse::<f64>().ok()).map(|s| s * 1000.)
}
