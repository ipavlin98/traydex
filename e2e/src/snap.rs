use crate::{pause, win};
use anyhow::{Result, bail};
use image::{Rgba, RgbaImage};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::HWND;

pub const CHANNEL: u8 = 24;
pub const RATIO: f64 = 0.002;
pub const CELL: u32 = 16;
pub const CELL_LIMIT: u32 = 12;

pub fn stable(hwnd: HWND) -> Result<RgbaImage> {
    let started = Instant::now();
    let mut previous: Option<RgbaImage> = None;
    while started.elapsed() < Duration::from_secs(8) {
        pause(250)?;
        let Some(current) = win::capture(hwnd) else {
            continue;
        };
        if previous.as_ref() == Some(&current) {
            return Ok(current);
        }
        previous = Some(current);
    }
    bail!("the window didn't settle: no two consecutive frames matched within 8 s")
}

pub struct Diff {
    pub differing: u64,
    pub total: u64,
    pub densest: u32,
    pub map: RgbaImage,
}

impl Diff {
    pub fn ratio(&self) -> f64 {
        self.differing as f64 / self.total.max(1) as f64
    }

    pub fn passes(&self) -> bool {
        self.ratio() <= RATIO && self.densest <= CELL_LIMIT
    }
}

pub fn compare(expected: &RgbaImage, actual: &RgbaImage) -> Option<Diff> {
    if expected.dimensions() != actual.dimensions() {
        return None;
    }
    let mut map = RgbaImage::new(expected.width(), expected.height());
    let columns = expected.width().div_ceil(CELL) as usize;
    let mut cells = vec![0u32; columns * expected.height().div_ceil(CELL) as usize];
    let mut differing = 0;
    for (x, y, a) in expected.enumerate_pixels() {
        let b = actual.get_pixel(x, y);
        let delta = (0..3).map(|i| a[i].abs_diff(b[i])).max().unwrap_or(0);
        let gray = ((a[0] as u32 + a[1] as u32 + a[2] as u32) / 12) as u8;
        if delta > CHANNEL {
            differing += 1;
            if let Some(cell) = cells.get_mut((y / CELL) as usize * columns + (x / CELL) as usize) {
                *cell += 1;
            }
            map.put_pixel(x, y, Rgba([255, 0, 0, 255]));
        } else {
            map.put_pixel(x, y, Rgba([gray, gray, gray, 255]));
        }
    }
    let densest = cells.into_iter().max().unwrap_or(0);
    Some(Diff { differing, total: expected.width() as u64 * expected.height() as u64, densest, map })
}

pub fn config(dpi: u32) -> String {
    let build = win::reg_read_machine("SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion", "CurrentBuildNumber")
        .unwrap_or_else(|| "unknown".into());
    let light =
        win::reg_dword("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize", "AppsUseLightTheme")
            .unwrap_or(1);
    format!("win{build}-{dpi}dpi-{}", if light == 0 { "dark" } else { "light" })
}
