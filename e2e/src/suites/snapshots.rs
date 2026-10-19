use super::{Session, Spec, Suite, artifacts, spec, write};
use crate::app::Diag;
use crate::snap::{self, CELL, CELL_LIMIT, CHANNEL, RATIO};
use crate::{Ctx, app, input, mismatch, skip, win};
use anyhow::{Context, Result};
use image::RgbaImage;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::HWND;

pub const SUITE: Suite = Suite { id: "snapshots", title: "Snapshot baselines", specs, run };

const STATES: [(&str, &str); 5] = [
    ("empty", "empty shelf"),
    ("list", "shelf as a list"),
    ("grid", "shelf as a grid"),
    ("settings", "settings"),
    ("actions", "tile editor"),
];
const THEMES: [(&str, &str, &str); 2] = [("light", "Light", "light"), ("dark", "Dark", "dark")];
const LANGUAGES: [(&str, &str); 2] = [("ru", "Russian"), ("en", "English")];
const SCREEN_STEP: f32 = 0.85;
const MAX_SCREENS: usize = 12;
const SCROLL_SETTLE: Duration = Duration::from_millis(600);
const MASKED: [&str; 1] = ["version"];

fn id(state: &str, theme: &str, language: &str) -> String {
    format!("{state}-{theme}-{language}")
}

fn specs() -> Vec<Spec> {
    let mut specs = Vec::new();
    for (theme, _, theme_title) in THEMES {
        for (language, _) in LANGUAGES {
            for (state, title) in STATES {
                specs.push(spec(
                    &id(state, theme, language),
                    &format!("Snapshot: {title}, {theme_title} theme, {language}"),
                    false,
                ));
            }
        }
    }
    specs
}

fn items(ctx: &Ctx) -> Result<Vec<serde_json::Value>> {
    let folder = ctx.sandbox.folder("snap")?;
    let report = write(&folder.join("Отчёт за квартал.txt"), &vec![b'a'; 2048])?;
    let table = write(&folder.join("data.csv"), &vec![b'1'; 512])?;
    let documents = folder.join("Документы");
    fs::create_dir_all(&documents)?;
    Ok(vec![
        app::path_item(&report),
        app::path_item(&table),
        app::path_item(&documents),
        app::note_item(ctx, "Купить молоко и хлеб")?,
    ])
}

fn launch(ctx: &Ctx, theme: &str, language: &str, filled: bool) -> Result<()> {
    ctx.reset()?;
    let items = if filled { items(ctx)? } else { Vec::new() };
    let mut settings = app::settings(ctx);
    settings["theme"] = theme.into();
    settings["language"] = language.into();
    ctx.start(settings, items, super::dnd::zip_tile(), &[])?;
    let app = ctx.app()?;
    app.wait_diag("panel drawn", Duration::from_secs(10), |d| d.page == "shelf")?;
    Ok(())
}

fn part_name(name: &str, index: usize) -> String {
    if index == 0 { name.to_string() } else { format!("{name}-{}", index + 1) }
}

fn baseline(ctx: &Ctx, name: &str, index: usize) -> PathBuf {
    ctx.baselines.join(format!("{}.png", part_name(name, index)))
}

fn shift(before: &Diag, after: &Diag, view: [f32; 4]) -> f32 {
    let [x, y, w, h] = view;
    before
        .marks
        .iter()
        .filter(|(_, [mx, my, mw, mh])| {
            let (cx, cy) = (mx + mw / 2., my + mh / 2.);
            cx > x && cx < x + w && cy > y && cy < y + h
        })
        .filter_map(|(name, rect)| after.marks.get(name).map(|moved| (rect[1] - moved[1]).abs()))
        .fold(0., f32::max)
}

fn shot(ctx: &Ctx, panel: HWND, scroll: Option<&str>) -> Result<RgbaImage> {
    let mut image = snap::stable(panel)?;
    let app = ctx.app()?;
    let diag = app.diag().context("no diagnostics")?;
    let window = win::rect(panel);
    let (width, height) = image.dimensions();
    let to_image = |[left, top, right, bottom]: [i32; 4]| {
        let x = |v: i32| (v - window.left).clamp(0, width as i32) as u32;
        let y = |v: i32| (v - window.top).clamp(0, height as i32) as u32;
        [x(left), y(top), x(right), y(bottom)]
    };
    let clip = match scroll {
        Some(mark) => to_image(app.rect(&diag, mark)?),
        None => [0, 0, width, height],
    };
    for mark in MASKED.iter().filter(|mark| diag.has(mark)) {
        let [left, top, right, bottom] = to_image(app.rect(&diag, mark)?);
        let (left, top, right, bottom) = (left.max(clip[0]), top.max(clip[1]), right.min(clip[2]), bottom.min(clip[3]));
        if left >= right || top >= bottom {
            continue;
        }
        let fill = *image.get_pixel(right - 1, top);
        for x in left..right {
            for y in top..bottom {
                image.put_pixel(x, y, fill);
            }
        }
    }
    Ok(image)
}

fn scrolled(ctx: &Ctx, panel: HWND, mark: &str, notches: i32) -> Result<(RgbaImage, f32)> {
    let app = ctx.app()?;
    let before = app.diag().context("no diagnostics")?;
    let view = *before.marks.get(mark).with_context(|| format!("the panel has no scroll area \"{mark}\""))?;
    win::post_wheel(panel, app.point(&before, mark)?, notches);
    app.post_leave();
    let image = shot(ctx, panel, Some(mark))?;
    let started = Instant::now();
    let mut moved = 0.;
    while started.elapsed() < SCROLL_SETTLE {
        moved = app.diag().map_or(0., |after| shift(&before, &after, view));
        if moved >= 1. {
            break;
        }
        crate::pause(50)?;
    }
    let height = app.diag().context("no diagnostics")?.marks.get(mark).map_or(0., |rect| rect[3]);
    if (height - view[3]).abs() >= 1. {
        return Err(mismatch(&format!("height of the scroll area \"{mark}\" after scrolling"), view[3], height));
    }
    Ok((image, moved))
}

fn screens(ctx: &Ctx, panel: HWND, mark: &str, first: RgbaImage) -> Result<Vec<RgbaImage>> {
    let mut parts = vec![first];
    let (_, step) = scrolled(ctx, panel, mark, 1)?;
    if step < 1. {
        return Ok(parts);
    }
    scrolled(ctx, panel, mark, -1)?;
    let diag = ctx.app()?.diag().context("no diagnostics")?;
    let height = diag.marks.get(mark).map_or(0., |view| view[3]);
    let notches = ((height * SCREEN_STEP) / step).floor().max(1.) as i32;
    while parts.len() < MAX_SCREENS {
        let (image, moved) = scrolled(ctx, panel, mark, notches)?;
        if moved < 1. && parts.last() == Some(&image) {
            break;
        }
        parts.push(image);
    }
    Ok(parts)
}

fn capture(ctx: &Ctx, name: &str, scroll: Option<&str>) -> Result<()> {
    let app = ctx.app()?;
    let panel = app.panel().context("panel not found")?;
    if win::inside(&win::rect(panel), win::cursor()) {
        input::move_to(super::dnd::center(ctx.layout.helper));
    }
    app.post_leave();
    let first = shot(ctx, panel, scroll)?;
    let parts = match scroll {
        Some(mark) => screens(ctx, panel, mark, first)?,
        None => vec![first],
    };
    if ctx.update {
        for (index, image) in parts.iter().enumerate() {
            update(ctx, &part_name(name, index), image)?;
        }
        let mut index = parts.len();
        while baseline(ctx, name, index).is_file() {
            fs::remove_file(baseline(ctx, name, index))?;
            ctx.changes.borrow_mut().push(format!("{}/{}.png: deleted", ctx.config, part_name(name, index)));
            index += 1;
        }
        return Ok(());
    }
    if !ctx.baselines.is_dir() {
        let known: Vec<String> = ctx
            .baselines
            .parent()
            .and_then(|dir| fs::read_dir(dir).ok())
            .map(|entries| {
                entries
                    .flatten()
                    .filter(|e| e.path().is_dir())
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        return Err(skip(format!(
            "no baselines for configuration {} (available: {}); take them with .\\build.ps1 -E2E baselines",
            ctx.config,
            if known.is_empty() { "none".into() } else { known.join(", ") }
        )));
    }
    if !baseline(ctx, name, 0).is_file() {
        return Err(skip(format!("no baseline {name}.png for configuration {}", ctx.config)));
    }
    let expected = (0..).take_while(|index| baseline(ctx, name, *index).is_file()).count();
    for (index, image) in parts.iter().enumerate().take(expected) {
        compare(ctx, &part_name(name, index), image)?;
    }
    if parts.len() != expected {
        return Err(mismatch("number of scroll screens on the page", expected, parts.len()));
    }
    Ok(())
}

fn update(ctx: &Ctx, name: &str, image: &RgbaImage) -> Result<()> {
    let path = ctx.baselines.join(format!("{name}.png"));
    let status = match image::open(&path).ok().map(|old| snap::compare(&old.to_rgba8(), image)) {
        Some(Some(diff)) if diff.passes() => "unchanged",
        Some(_) => "updated",
        None => "created",
    };
    if status != "unchanged" {
        fs::create_dir_all(&ctx.baselines)?;
        image.save(&path)?;
    }
    ctx.changes.borrow_mut().push(format!("{}/{name}.png — {status}", ctx.config));
    Ok(())
}

fn compare(ctx: &Ctx, name: &str, image: &RgbaImage) -> Result<()> {
    let expected = image::open(ctx.baselines.join(format!("{name}.png")))?.to_rgba8();
    let dir = || artifacts(ctx, SUITE.id, name);
    match snap::compare(&expected, image) {
        None => {
            let _ = image.save(dir().join("actual.png"));
            let _ = expected.save(dir().join("baseline.png"));
            Err(mismatch("the snapshot size doesn't match the baseline", expected.dimensions(), image.dimensions()))
        }
        Some(diff) if !diff.passes() => {
            let _ = image.save(dir().join("actual.png"));
            let _ = expected.save(dir().join("baseline.png"));
            let _ = diff.map.save(dir().join("diff.png"));
            Err(mismatch(
                &format!("snapshot {name} differs from the baseline"),
                format!(
                    "at most {:.1}% of pixels and at most {CELL_LIMIT} in a {CELL}×{CELL} square with a channel difference over {CHANNEL}",
                    RATIO * 100.
                ),
                format!(
                    "{:.3}% ({} of {}), {} in the densest square",
                    diff.ratio() * 100.,
                    diff.differing,
                    diff.total,
                    diff.densest
                ),
            ))
        }
        Some(_) => Ok(()),
    }
}

fn open_page(ctx: &Ctx, button: &str, page: &str) -> Result<()> {
    let app = ctx.app()?;
    app.post_click(button)?;
    app.wait_diag(&format!("page {page} is open"), Duration::from_secs(5), |d| d.page == page)?;
    Ok(())
}

fn run(s: &mut Session) {
    for (theme, mode, _) in THEMES {
        for (language, name) in LANGUAGES {
            let check = |ctx: &Ctx| -> Result<()> {
                let diag = ctx.app()?.diag().context("no diagnostics")?;
                if diag.dark != (mode == "Dark") || diag.language != name {
                    return Err(mismatch("panel theme and language", (mode, name), (diag.dark, diag.language)));
                }
                Ok(())
            };
            s.run(&id("empty", theme, language), |ctx| {
                launch(ctx, mode, name, false)?;
                check(ctx)?;
                capture(ctx, &id("empty", theme, language), None)
            });
            s.run(&id("list", theme, language), |ctx| {
                launch(ctx, mode, name, true)?;
                ctx.app()?
                    .wait_diag("items shown", Duration::from_secs(5), |d| d.items.len() == 4 && d.layout == "list")?;
                check(ctx)?;
                capture(ctx, &id("list", theme, language), Some("items"))
            });
            s.run(&id("grid", theme, language), |ctx| {
                if ctx.app().is_err() {
                    launch(ctx, mode, name, true)?;
                }
                let app = ctx.app()?;
                app.post_click("layout")?;
                app.wait_diag("the shelf switched to the grid", Duration::from_secs(5), |d| d.layout == "grid")?;
                drop(app);
                capture(ctx, &id("grid", theme, language), Some("items"))
            });
            s.run(&id("settings", theme, language), |ctx| {
                if ctx.app().is_err() {
                    launch(ctx, mode, name, true)?;
                }
                open_page(ctx, "settings", "settings")?;
                capture(ctx, &id("settings", theme, language), Some("settings-page"))
            });
            s.run(&id("actions", theme, language), |ctx| {
                if ctx.app().is_err() {
                    launch(ctx, mode, name, true)?;
                }
                open_page(ctx, "actions", "actions")?;
                capture(ctx, &id("actions", theme, language), Some("actions-page"))
            });
            s.ctx.stop();
        }
    }
}
