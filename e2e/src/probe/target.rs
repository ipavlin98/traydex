use super::Probe;
use crate::app::Diag;
use crate::win;
use anyhow::{Context, Result, bail};
use windows::Win32::Foundation::{HWND, POINT, RECT};

pub(super) fn fits(diag: &Diag, panel: HWND) -> bool {
    let client = win::client(panel);
    let [width, height] = diag.size.map(|logical| (logical * diag.scale).round() as i32);
    (width - (client.right - client.left)).abs() <= 2 && (height - (client.bottom - client.top)).abs() <= 2
}

pub struct Target {
    pub name: String,
    pub rect: RECT,
    pub point: POINT,
}

impl Probe<'_> {
    pub(super) fn target(&self, spec: &str) -> Result<Target> {
        let diag = self.diag()?;
        let panel = self.panel()?;
        if !win::visible(panel) {
            bail!("the panel is hidden, \"{spec}\" is unavailable");
        }
        let client = win::client(panel);
        if !fits(&diag, panel) {
            bail!(
                "the app layout {}×{} logical px doesn't match the window {}×{} physical px (scale {}), so the marks are wrong",
                diag.size[0],
                diag.size[1],
                client.right - client.left,
                client.bottom - client.top,
                diag.scale
            );
        }
        let scale = diag.scale;
        let screen = |[x, y, w, h]: [f32; 4]| RECT {
            left: client.left + (x * scale).round() as i32,
            top: client.top + (y * scale).round() as i32,
            right: client.left + ((x + w) * scale).round() as i32,
            bottom: client.top + ((y + h) * scale).round() as i32,
        };
        let (name, rect) = if let Some(raw) = spec.strip_prefix('@') {
            let (x, y) = raw
                .split_once(',')
                .and_then(|(x, y)| Some((x.trim().parse::<f32>().ok()?, y.trim().parse::<f32>().ok()?)))
                .with_context(|| format!("\"{spec}\": expected @x,y in logical pixels"))?;
            (spec.to_string(), screen([x, y, 1. / scale, 1. / scale]))
        } else if let Some((prefix, index)) = spec.split_once('#') {
            let index: usize = index.parse().with_context(|| format!("\"{spec}\": a number must follow #"))?;
            let mut matching: Vec<(&String, RECT)> = diag
                .marks
                .iter()
                .filter(|(mark, _)| mark.split_once(':').is_some_and(|(head, _)| head == prefix))
                .map(|(mark, bounds)| (mark, screen(*bounds)))
                .collect();
            matching.sort_by_key(|(_, rect)| (rect.top, rect.left));
            let count = matching.len();
            let (mark, rect) = matching
                .into_iter()
                .nth(index)
                .with_context(|| format!("\"{spec}\": there are {count} \"{prefix}:…\" elements on screen"))?;
            (mark.clone(), rect)
        } else {
            let bounds = diag.marks.get(spec).with_context(|| {
                let similar: Vec<&str> = diag
                    .marks
                    .keys()
                    .map(String::as_str)
                    .filter(|mark| mark.contains(spec) || spec.contains(*mark))
                    .take(12)
                    .collect();
                format!(
                    "the panel has no mark \"{spec}\"{}; the marks step lists them all",
                    if similar.is_empty() { String::new() } else { format!(", similar: {}", similar.join(", ")) }
                )
            })?;
            (spec.to_string(), screen(*bounds))
        };
        let point = POINT { x: (rect.left + rect.right) / 2, y: (rect.top + rect.bottom) / 2 };
        if !win::inside(&client, point) {
            bail!(
                "\"{name}\" is outside the visible part of the window (center ({}, {}), window x {}..{}, y {}..{})",
                point.x,
                point.y,
                client.left,
                client.right,
                client.top,
                client.bottom
            );
        }
        if name.starts_with("item")
            && let Some(list) = diag.marks.get("items")
            && !win::inside(&screen(*list), point)
        {
            bail!("\"{name}\" is scrolled out of the list; scroll first: scroll items <notches>");
        }
        Ok(Target { name, rect, point })
    }

    pub(super) fn under(&self, diag: &Diag, point: POINT) -> Option<String> {
        let client = win::client(self.panel().ok()?);
        if !win::inside(&client, point) {
            return None;
        }
        let x = (point.x - client.left) as f32 / diag.scale;
        let y = (point.y - client.top) as f32 / diag.scale;
        diag.marks
            .iter()
            .filter(|(_, [mx, my, mw, mh])| x >= *mx && y >= *my && x < mx + mw && y < my + mh)
            .min_by(|(_, a), (_, b)| (a[2] * a[3]).total_cmp(&(b[2] * b[3])))
            .map(|(mark, _)| mark.clone())
    }
}
