use gpui::{AssetSource, SharedString};
use std::borrow::Cow;

pub struct Assets;

fn shape(name: &str) -> Option<&'static str> {
    Some(match name {
        "drop" => "<path d='M12 4v10m-4-4 4 4 4-4'/><path d='M4 15v3a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-3'/>",
        "file" => "<path d='M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8Z'/><path d='M14 3v5h5'/>",
        "file-archive" => {
            "<path d='M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8Z'/><path d='M14 3v5h5'/><path d='M10 6v1M10 9.5v1'/><rect x='8.5' y='13' width='3' height='4' rx='1'/>"
        }
        "unpack" => {
            "<rect x='2' y='3' width='20' height='5' rx='1'/><path d='M4 8v11a2 2 0 0 0 2 2h2M20 8v11a2 2 0 0 1-2 2h-2'/><path d='m9 15 3-3 3 3M12 12v9'/>"
        }
        "folder" => {
            "<path d='M3 6.5A1.5 1.5 0 0 1 4.5 5H9l2 2.5h8.5A1.5 1.5 0 0 1 21 9v9.5a1.5 1.5 0 0 1-1.5 1.5h-15A1.5 1.5 0 0 1 3 18.5Z'/>"
        }
        "image" => {
            "<rect x='3' y='4' width='18' height='16' rx='2'/><circle cx='9' cy='9.5' r='1.5'/><path d='m21 16-5-5-9 9'/>"
        }
        "text" => "<path d='M5 6h14M5 10h14M5 14h10M5 18h7'/>",
        "note" => {
            "<path d='M7.5 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2v3.5'/><path d='M7 7.5h6M7 11.5h4'/><path d='M18.13 11.87a1.84 1.84 0 0 1 2.6 2.6L14.5 20.7l-3.47.93.93-3.47Z'/>"
        }
        "link" => {
            "<path d='M10 14a4 4 0 0 0 5.66 0l3-3a4 4 0 0 0-5.66-5.66l-1 1'/><path d='M14 10a4 4 0 0 0-5.66 0l-3 3a4 4 0 0 0 5.66 5.66l1-1'/>"
        }
        "copy" => {
            "<rect x='8' y='8' width='12' height='12' rx='2'/><path d='M16 8V6a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h2'/>"
        }
        "eye" => {
            "<path d='M2.5 12S6 5.5 12 5.5 21.5 12 21.5 12 18 18.5 12 18.5 2.5 12 2.5 12Z'/><circle cx='12' cy='12' r='3'/>"
        }
        "open" => "<path d='M14 4h6v6M20 4l-9 9'/><path d='M18 14v4a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h4'/>",
        "close" => "<path d='M6 6l12 12M18 6 6 18'/>",
        "stop" => "<circle cx='12' cy='12' r='9'/><rect x='9' y='9' width='6' height='6' rx='1'/>",
        "plus" => "<path d='M12 5v14M5 12h14'/>",
        "alert" => "<circle cx='12' cy='12' r='9'/><path d='M12 7.5v5.5M12 16.5h.01'/>",
        "search" => "<circle cx='10.5' cy='10.5' r='6.5'/><path d='m15.3 15.3 4.7 4.7'/>",
        "pin" => "<path d='M9 4h6M10 4v5l-3 4h10l-3-4V4M12 13v7'/>",
        "trash" => "<path d='M4 7h16M10 7V4h4v3M6 7l1 13h10l1-13'/>",
        "settings" => {
            "<path d='M4 7h10M18 7h2M4 17h2M10 17h10'/><circle cx='16' cy='7' r='2'/><circle cx='8' cy='17' r='2'/>"
        }
        "sun" => {
            "<circle cx='12' cy='12' r='4'/><path d='M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4'/>"
        }
        "moon" => "<path d='M20 14.5A8 8 0 1 1 9.5 4a6.5 6.5 0 0 0 10.5 10.5Z'/>",
        "monitor" => "<rect x='3' y='4' width='18' height='12' rx='2'/><path d='M8 20h8M12 16v4'/>",
        "check" => "<path d='m5 12 5 5 9-10'/>",
        "grid" => {
            "<rect x='4' y='4' width='6.5' height='6.5' rx='1.5'/><rect x='13.5' y='4' width='6.5' height='6.5' rx='1.5'/><rect x='4' y='13.5' width='6.5' height='6.5' rx='1.5'/><rect x='13.5' y='13.5' width='6.5' height='6.5' rx='1.5'/>"
        }
        "list" => "<path d='M9 6h11M9 12h11M9 18h11M4.5 6h.01M4.5 12h.01M4.5 18h.01'/>",
        "chevron" => "<path d='m6 9 6 6 6-6'/>",
        "zap" => "<path d='M13 3 5 13h6l-1 8 8-10h-6l1-8Z'/>",
        "upload" => "<path d='M12 15V4m-4 4 4-4 4 4'/><path d='M4 15v3a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-3'/>",
        "terminal" => "<rect x='3' y='4' width='18' height='16' rx='2'/><path d='m7 9 3 3-3 3M13 15h4'/>",
        "loader" => "<path d='M12 3a9 9 0 1 0 9 9'/>",
        "arrow-up" => "<path d='M12 19V5M5 12l7-7 7 7'/>",
        "arrow-down" => "<path d='M12 5v14M19 12l-7 7-7-7'/>",
        "chevron-right" => "<path d='m9 6 6 6-6 6'/>",
        "chevron-left" => "<path d='m15 6-6 6 6 6'/>",
        "arrow-left" => "<path d='M19 12H5M11 6l-6 6 6 6'/>",
        "refresh" => {
            "<path d='M3 12a9 9 0 0 1 15.4-6.4L21 8'/><path d='M21 3v5h-5'/><path d='M21 12a9 9 0 0 1-15.4 6.4L3 16'/><path d='M3 21v-5h5'/>"
        }
        "reset" => "<path d='M4.5 13a7.5 7.5 0 1 0 2.2-6.3L4 9.3'/><path d='M4 4.5v4.8h4.8'/>",
        "server" => {
            "<rect x='4' y='4' width='16' height='7' rx='1.5'/><rect x='4' y='13' width='16' height='7' rx='1.5'/><path d='M8 7.5h.01M8 16.5h.01'/>"
        }
        "smartphone" => "<rect x='7' y='3' width='10' height='18' rx='2'/><path d='M11 17.5h2'/>",
        "code" => "<path d='m8 8-4 4 4 4M16 8l4 4-4 4M13.5 5l-3 14'/>",
        "hash" => "<path d='M5 9h15M4 15h15M10 3 8 21M16 3l-2 18'/>",
        "scan-text" => {
            "<path d='M4 8V6a2 2 0 0 1 2-2h2M16 4h2a2 2 0 0 1 2 2v2M20 16v2a2 2 0 0 1-2 2h-2M8 20H6a2 2 0 0 1-2-2v-2M8 9h8M8 12h8M8 15h5'/>"
        }
        "shrink" => "<path d='M4 14h6v6M20 10h-6V4M14 10l7-7M3 21l7-7'/>",
        "file-text" => {
            "<path d='M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8Z'/><path d='M14 3v5h5'/><path d='M9 13h6M9 17h4'/>"
        }
        "rename" => "<path d='M12 20h8'/><path d='M16.5 3.5a2.12 2.12 0 0 1 3 3L7 19l-4 1 1-4Z'/>",
        "qr" => {
            "<rect x='4' y='4' width='6' height='6' rx='1'/><rect x='14' y='4' width='6' height='6' rx='1'/><rect x='4' y='14' width='6' height='6' rx='1'/><path d='M14 14h2v2h-2zM18 18h2v2h-2zM14 18v2M18 14h2'/>"
        }
        "video" => "<rect x='3' y='5' width='18' height='14' rx='3'/><path d='M10 9.2v5.6l4.8-2.8Z'/>",
        "puzzle" => "<path d='M5 8h3.5a2 2 0 1 1 4 0H16v3.5a2 2 0 1 1 0 4V19H5Z'/>",
        "shield" => "<path d='M12 3 5 6v5c0 4.5 3 8.5 7 10 4-1.5 7-5.5 7-10V6Z'/>",
        "film" => {
            "<rect x='3' y='4' width='18' height='16' rx='2'/><path d='M7 4v16M17 4v16M3 9h4M3 15h4M17 9h4M17 15h4'/>"
        }
        "grip" => {
            "<circle cx='9' cy='6' r='.9' fill='white'/><circle cx='15' cy='6' r='.9' fill='white'/><circle cx='9' cy='12' r='.9' fill='white'/><circle cx='15' cy='12' r='.9' fill='white'/><circle cx='9' cy='18' r='.9' fill='white'/><circle cx='15' cy='18' r='.9' fill='white'/>"
        }
        "corner-top-left" => {
            "<rect x='3' y='5' width='18' height='14' rx='2'/><path d='M3 7a2 2 0 0 1 2-2h4v5H3Z' fill='white'/>"
        }
        "corner-top-right" => {
            "<rect x='3' y='5' width='18' height='14' rx='2'/><path d='M21 7a2 2 0 0 0-2-2h-4v5h6Z' fill='white'/>"
        }
        "corner-bottom-left" => {
            "<rect x='3' y='5' width='18' height='14' rx='2'/><path d='M3 17a2 2 0 0 0 2 2h4v-5H3Z' fill='white'/>"
        }
        "corner-bottom-right" => {
            "<rect x='3' y='5' width='18' height='14' rx='2'/><path d='M21 17a2 2 0 0 1-2 2h-4v-5h6Z' fill='white'/>"
        }
        _ => return None,
    })
}

fn brand(name: &str) -> Option<&'static str> {
    Some(match name {
        "brand" => {
            "<svg xmlns='http://www.w3.org/2000/svg' width='48' height='48' viewBox='8 8 48 48'><path d='M32 12 51.97 26.51 44.34 49.99H19.66L12.03 26.51Z' fill='white' stroke='white' stroke-width='5' stroke-linejoin='round'/></svg>"
        }
        "brand-arrow" => {
            "<svg xmlns='http://www.w3.org/2000/svg' width='48' height='48' viewBox='8 8 48 48'><path d='M32 23v17.5M24.5 33.5 32 41l7.5-7.5' fill='none' stroke='white' stroke-width='4.5' stroke-linecap='round' stroke-linejoin='round'/></svg>"
        }
        _ => return None,
    })
}

pub fn inset(name: &str) -> f32 {
    match name {
        "folder" | "image" | "alert" | "note" => 2.125,
        "upload" | "search" => 3.125,
        "link" => 3.3,
        "file" | "file-archive" | "text" | "check" => 4.125,
        "chevron-left" => 8.125,
        _ => 0.,
    }
}

const RING_RADIUS: f32 = 9.;

fn ring(name: &str) -> Option<String> {
    let circle = |extra: String| {
        format!(
            "<svg xmlns='http://www.w3.org/2000/svg' width='24' height='24' viewBox='0 0 24 24' fill='none' stroke='white' stroke-width='2.5' stroke-linecap='round'><circle cx='12' cy='12' r='{RING_RADIUS}' {extra}/></svg>"
        )
    };
    if name == "ring-track" {
        return Some(circle(String::new()));
    }
    let percent: u32 = name.strip_prefix("ring-")?.parse().ok()?;
    let length = std::f32::consts::TAU * RING_RADIUS;
    let filled = length * percent.min(100) as f32 / 100.;
    Some(circle(format!(
        "transform='rotate(-90 12 12)' stroke-dasharray='{filled:.2} {length:.2}'{}",
        if percent == 0 { " stroke-opacity='0'" } else { "" }
    )))
}

impl AssetSource for Assets {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        if path.starts_with("<svg") {
            return Ok(Some(Cow::Owned(path.as_bytes().to_vec())));
        }
        if let Some(svg) = brand(path) {
            return Ok(Some(Cow::Borrowed(svg.as_bytes())));
        }
        if let Some(svg) = ring(path) {
            return Ok(Some(Cow::Owned(svg.into_bytes())));
        }
        Ok(shape(path).map(|shape| {
            Cow::Owned(format!("<svg xmlns='http://www.w3.org/2000/svg' width='24' height='24' viewBox='0 0 24 24' fill='none' stroke='white' stroke-width='1.75' stroke-linecap='round' stroke-linejoin='round'>{shape}</svg>").into_bytes())
        }))
    }

    fn list(&self, _: &str) -> anyhow::Result<Vec<SharedString>> {
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests;
