use super::dnd::{center, zip_tile};
use super::{Session, Spec, Suite, none, spec};
use crate::app::{self, Diag};
use crate::frames::{FRAME, Frames, OPACITY, offset};
use crate::helper::{self, Payload};
use crate::{Ctx, input, mismatch, pause, wait, win};
use anyhow::{Context, Result};
use image::RgbaImage;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{POINT, RECT};
use windows::Win32::UI::Input::KeyboardAndMouse::{VIRTUAL_KEY, VK_CONTROL, VK_F9, VK_MENU, VK_SHIFT};

pub const SUITE: Suite = Suite { id: "motion", title: "Animation smoothness", specs, run };

const NOTES: usize = 100;
const SWEEP: Duration = Duration::from_secs(4);
const FADE_WATCH: Duration = Duration::from_millis(900);
const FADE_SETTLE_MS: u64 = 300;
const CLEAN_WATCH: Duration = Duration::from_millis(200);
const CLEAN_SETTLE: u64 = 150;
const CLEAN_TOLERANCE: u8 = 3;
const TOOLTIP_FREE: u64 = 300;
const BADGE_OFFSET: i32 = 30;
const CONTROLS: [&str; 5] = ["pin", "settings", "layout", "tab-1", "drag-all"];
const HOVER_WATCH: Duration = Duration::from_millis(600);
const HOVER_REACTION_MS: u64 = 50;
const HOVER_SETTLE_MS: u64 = 300;
const TOGGLE: [VIRTUAL_KEY; 4] = [VK_CONTROL, VK_SHIFT, VK_MENU, VK_F9];

fn specs() -> Vec<Spec> {
    vec![
        spec("hover-smooth", "Hovering rows with animations runs at 120+ FPS", true),
        spec("fade-smooth", "Showing and hiding the window with animations runs at 120+ FPS", true),
        spec("hover-gradual", "Row hover highlight changes gradually, and instantly without animations", true),
        spec(
            "hover-clean",
            "No artifacts during the hover transition: colors stay between the start and end colors",
            true,
        ),
        spec("fade-gradual", "The window appears and hides smoothly and without delay", true),
        spec("first-fade", "After a hidden start the animated window shows up visible the first time", true),
        spec("cancel-steady", "When a drag is cancelled the fading window keeps its contents", true),
    ]
}

fn log_path(ctx: &Ctx) -> Result<PathBuf> {
    fs::create_dir_all(&ctx.reports)?;
    Ok(ctx.reports.join("motion-frames.log"))
}

fn launch(ctx: &Ctx, animations: bool) -> Result<()> {
    launch_with(ctx, animations, |_| {})
}

fn launch_with(ctx: &Ctx, animations: bool, tweak: impl FnOnce(&mut serde_json::Value)) -> Result<()> {
    ctx.reset()?;
    let mut settings = app::settings(ctx);
    tweak(&mut settings);
    settings["window_animations"] = animations.into();
    settings["interface_animations"] = animations.into();
    settings["automatic"] = false.into();
    settings["pinned"] = true.into();
    settings["size"] = serde_json::json!([540.0, 700.0]);
    let items = (0..NOTES)
        .map(|index| app::note_item(ctx, &format!("Note {index}: a row with long text that gets cut off in the list")))
        .collect::<Result<Vec<_>>>()?;
    ctx.start_measured(settings, items, none(), &log_path(ctx)?)?;
    ctx.app()?.wait_shown(true, "the panel is visible after start", Duration::from_secs(10))?;
    Ok(())
}

fn rows(ctx: &Ctx) -> Result<(POINT, POINT)> {
    let app = ctx.app()?;
    let diag: Diag = app.wait_diag("shelf rows drawn", Duration::from_secs(10), |d| {
        d.marks.keys().filter(|mark| mark.starts_with("item:")).count() >= 5
    })?;
    let panel = app.panel().context("panel not found")?;
    let bottom = win::client(panel).bottom;
    let mut centers: Vec<POINT> = diag
        .marks
        .keys()
        .filter(|mark| mark.starts_with("item:"))
        .filter_map(|mark| app.rect(&diag, mark).ok())
        .filter(|rect| rect[3] < bottom - 60)
        .map(|rect| POINT { x: (rect[0] + rect[2]) / 2 - 60, y: (rect[1] + rect[3]) / 2 })
        .collect();
    centers.sort_by_key(|point| point.y);
    let first = *centers.first().context("no shelf rows")?;
    let last = *centers.last().context("no shelf rows")?;
    Ok((first, last))
}

fn sweep(ctx: &Ctx) -> Result<Frames> {
    let (top, bottom) = rows(ctx)?;
    input::move_to(top);
    pause(300)?;
    let path = log_path(ctx)?;
    let from = offset(&path);
    let started = Instant::now();
    while started.elapsed() < SWEEP {
        input::glide(bottom, 60, Duration::from_millis(8))?;
        input::glide(top, 60, Duration::from_millis(8))?;
    }
    pause(400)?;
    Ok(Frames::read(&path, from, FRAME))
}

fn focus_helper(ctx: &Ctx) -> Result<()> {
    input::click(center(ctx.layout.helper))?;
    wait("the helper window is active", Duration::from_secs(3), || {
        (win::foreground() == crate::helper::window()).then_some(())
    })?;
    Ok(())
}

fn toggles(ctx: &Ctx) -> Result<Frames> {
    let path = log_path(ctx)?;
    let from = offset(&path);
    for _ in 0..4 {
        focus_helper(ctx)?;
        input::chord(&TOGGLE)?;
        ctx.app()?.wait_shown(false, "the panel was hidden by the shortcut", Duration::from_secs(4))?;
        pause(300)?;
        input::chord(&TOGGLE)?;
        ctx.app()?.wait_shown(true, "the panel was shown by the shortcut", Duration::from_secs(4))?;
        pause(500)?;
    }
    Ok(Frames::read(&path, from, OPACITY))
}

fn within_budget(what: &str, frames: &Frames) -> Result<()> {
    println!();
    println!("    {what}: {}", frames.summary());
    frames.within_budget(what)
}

struct Hover {
    resting: [u8; 3],
    hovered: [u8; 3],
    timeline: Vec<(u64, [u8; 3])>,
}

impl Hover {
    fn first_change(&self) -> Option<u64> {
        self.timeline.iter().find(|(_, color)| *color != self.resting).map(|(at, _)| *at)
    }

    fn settled(&self) -> Option<u64> {
        self.timeline.iter().find(|(_, color)| *color == self.hovered).map(|(at, _)| *at)
    }

    fn steps(&self) -> usize {
        self.timeline.iter().filter(|(_, color)| *color != self.resting && *color != self.hovered).count()
    }

    fn summary(&self) -> String {
        format!("{:?} → {:?}, color changes (ms, color): {:?}", self.resting, self.hovered, self.timeline)
    }
}

fn hover_colors(ctx: &Ctx) -> Result<Hover> {
    let app = ctx.app()?;
    let diag = app.wait_diag("shelf rows drawn", Duration::from_secs(10), |d| {
        d.marks.keys().filter(|mark| mark.starts_with("item:")).count() >= 2
    })?;
    let mut rows: Vec<[i32; 4]> = diag
        .marks
        .keys()
        .filter(|mark| mark.starts_with("item:"))
        .filter_map(|mark| app.rect(&diag, mark).ok())
        .collect();
    rows.sort_by_key(|rect| rect[1]);
    let (Some(first), Some(second)) = (rows.first().copied(), rows.get(1).copied()) else {
        return Err(mismatch("shelf rows", "two rows", rows.len()));
    };
    let probe = POINT { x: first[0] + 6, y: (first[1] + first[3]) / 2 };
    let pixel = || win::screen_pixel(probe).context("couldn't read a screen pixel");
    input::move_to(POINT { x: (second[0] + second[2]) / 2, y: (second[1] + second[3]) / 2 });
    pause(500)?;
    let resting = pixel()?;
    input::move_to(POINT { x: (first[0] + first[2]) / 2, y: (first[1] + first[3]) / 2 });
    let started = Instant::now();
    let mut timeline: Vec<(u64, [u8; 3])> = Vec::new();
    while started.elapsed() < HOVER_WATCH {
        let color = pixel()?;
        if timeline.last().is_none_or(|(_, last)| *last != color) {
            timeline.push((started.elapsed().as_millis() as u64, color));
        }
    }
    let hovered = pixel()?;
    Ok(Hover { resting, hovered, timeline })
}

fn watch(probe: POINT, during: impl FnOnce() -> Result<()>) -> Result<Vec<(u64, [u8; 3])>> {
    let started = Instant::now();
    let sampler = std::thread::spawn(move || {
        let mut timeline: Vec<(u64, [u8; 3])> = Vec::new();
        while started.elapsed() < FADE_WATCH {
            if let Some(color) = win::screen_pixel(probe)
                && timeline.last().is_none_or(|(_, last)| *last != color)
            {
                timeline.push((started.elapsed().as_millis() as u64, color));
            }
        }
        timeline
    });
    let result = during();
    let timeline = sampler.join().map_err(|_| anyhow::anyhow!("the pixel sampling thread crashed"))?;
    result.map(|()| timeline)
}

fn gradual(what: &str, timeline: &[(u64, [u8; 3])]) -> Result<()> {
    let (Some((_, from)), Some((end, to))) = (timeline.first(), timeline.last()) else {
        return Err(mismatch(what, "pixel samples", "empty"));
    };
    let steps = timeline.iter().filter(|(_, color)| color != from && color != to).count();
    let started = timeline.get(1).map(|(at, _)| *at).unwrap_or_default();
    println!("    {what}: {} ms from the first change to the end, {steps} intermediate colors", end - started);
    if from == to {
        return Err(mismatch(what, "the color under the panel differs from the panel", format!("{timeline:?}")));
    }
    if steps < 3 || end - started > FADE_SETTLE_MS {
        return Err(mismatch(what, format!("smooth, at most {FADE_SETTLE_MS} ms"), format!("{timeline:?}")));
    }
    Ok(())
}

fn fade_gradual(ctx: &Ctx) -> Result<()> {
    launch(ctx, true)?;
    let (top, _) = rows(ctx)?;
    let app = ctx.app()?;
    let diag = app.wait_diag("shelf rows drawn", Duration::from_secs(5), |d| d.has("items"))?;
    let items = app.rect(&diag, "items")?;
    let probe = POINT { x: items[0] + 6, y: top.y };
    focus_helper(ctx)?;
    pause(300)?;
    let hiding = watch(probe, || {
        input::chord(&TOGGLE)?;
        app.wait_shown(false, "the panel was hidden by the shortcut", Duration::from_secs(4))
    })?;
    println!();
    gradual("hiding", &hiding)?;
    pause(300)?;
    let showing = watch(probe, || {
        input::chord(&TOGGLE)?;
        app.wait_shown(true, "the panel was shown by the shortcut", Duration::from_secs(4))
    })?;
    gradual("showing", &showing)
}

fn first_fade(ctx: &Ctx) -> Result<()> {
    ctx.reset()?;
    let mut settings = app::settings(ctx);
    settings["window_animations"] = true.into();
    settings["interface_animations"] = true.into();
    settings["automatic"] = false.into();
    settings["pinned"] = true.into();
    ctx.start(settings, vec![], none(), &["--autostart"])?;
    let app = ctx.app()?;
    pause(1000)?;
    if app.shown() {
        return Err(mismatch("the panel after a hidden start", "hidden", "visible"));
    }
    let panel = app.panel().context("panel not found")?;
    let area = win::client(panel);
    let probe = POINT { x: (area.left + area.right) / 2, y: area.bottom - 40 };
    focus_helper(ctx)?;
    pause(300)?;
    let showing = watch(probe, || {
        input::chord(&TOGGLE)?;
        app.wait_shown(true, "the panel was shown by the shortcut", Duration::from_secs(4))
    })?;
    println!();
    gradual("first show", &showing)
}

struct Transition {
    resting: RgbaImage,
    frames: Vec<RgbaImage>,
    settled: RgbaImage,
}

struct Artifact {
    frame: usize,
    pixels: usize,
    excess: u8,
    bounds: [u32; 4],
    sample: Option<([u8; 3], [u8; 3], [u8; 3])>,
}

impl Transition {
    fn capture(area: RECT, change: impl FnOnce() -> Result<()>) -> Result<Transition> {
        let grab = || win::screen_region(area).context("couldn't capture the screen area");
        let resting = grab()?;
        change()?;
        let started = Instant::now();
        let mut frames = Vec::new();
        while started.elapsed() < CLEAN_WATCH {
            frames.push(grab()?);
        }
        pause(CLEAN_SETTLE)?;
        Ok(Transition { resting, frames, settled: grab()? })
    }

    fn worst(&self) -> Option<Artifact> {
        let mut worst: Option<Artifact> = None;
        let width = self.resting.width().max(1);
        for (index, frame) in self.frames.iter().enumerate() {
            let mut pixels = 0;
            let mut excess = 0;
            let mut bounds = [u32::MAX, u32::MAX, 0, 0];
            let mut sample = None;
            for (position, ((from, to), now)) in
                self.resting.pixels().zip(self.settled.pixels()).zip(frame.pixels()).enumerate()
            {
                let over = (0..3)
                    .map(|c| {
                        let (low, high) = (from[c].min(to[c]), from[c].max(to[c]));
                        low.saturating_sub(now[c]).max(now[c].saturating_sub(high))
                    })
                    .max()
                    .unwrap_or_default();
                if over > CLEAN_TOLERANCE {
                    pixels += 1;
                    let (x, y) = (position as u32 % width, position as u32 / width);
                    bounds = [bounds[0].min(x), bounds[1].min(y), bounds[2].max(x), bounds[3].max(y)];
                    if over > excess {
                        excess = over;
                        sample = Some(([from[0], from[1], from[2]], [now[0], now[1], now[2]], [to[0], to[1], to[2]]));
                    }
                }
            }
            if pixels > 0 && worst.as_ref().is_none_or(|w| pixels > w.pixels) {
                worst = Some(Artifact { frame: index, pixels, excess, bounds, sample });
            }
        }
        worst
    }

    fn verify(&self, ctx: &Ctx, what: &str, name: &str) -> Result<()> {
        let changed = self.resting.pixels().zip(self.settled.pixels()).filter(|(a, b)| a != b).count();
        println!("    {what}: {} screenshots, {changed} changed pixels", self.frames.len());
        if changed == 0 {
            return Err(mismatch(what, "the highlight changes the row", "the row didn't change"));
        }
        let Some(worst) = self.worst() else {
            return Ok(());
        };
        let dir = super::artifacts(ctx, "motion", "hover-clean");
        let _ = self.resting.save(dir.join(format!("{name}-resting.png")));
        let _ = self.settled.save(dir.join(format!("{name}-settled.png")));
        if let Some(frame) = self.frames.get(worst.frame) {
            let _ = frame.save(dir.join(format!("{name}-artifact.png")));
        }
        Err(mismatch(
            what,
            "all pixels between the start and end colors",
            format!(
                "frame {}: {} pixels out of range, up to {} levels, area x {}..{}, y {}..{}, worst pixel before→now→after {:?}",
                worst.frame,
                worst.pixels,
                worst.excess,
                worst.bounds[0],
                worst.bounds[2],
                worst.bounds[1],
                worst.bounds[3],
                worst.sample
            ),
        ))
    }
}

fn hover_clean(ctx: &Ctx) -> Result<()> {
    println!();
    for (name, layout, theme) in
        [("list-light", "List", "Light"), ("grid-light", "Grid", "Light"), ("list-dark", "List", "Dark")]
    {
        launch_with(ctx, true, |settings| {
            settings["layout"] = layout.into();
            settings["theme"] = theme.into();
        })?;
        hover_clean_in(ctx, name)?;
    }
    Ok(())
}

fn hover_clean_in(ctx: &Ctx, name: &str) -> Result<()> {
    let app = ctx.app()?;
    let diag = app.wait_diag("shelf items drawn", Duration::from_secs(10), |d| {
        d.marks.keys().filter(|mark| mark.starts_with("item:")).count() >= 2
    })?;
    let mut items: Vec<[i32; 4]> = diag
        .marks
        .keys()
        .filter(|mark| mark.starts_with("item:"))
        .filter_map(|mark| app.rect(&diag, mark).ok())
        .collect();
    items.sort_by_key(|rect| (rect[1], rect[0]));
    let (Some(first), Some(second)) = (items.first().copied(), items.get(1).copied()) else {
        return Err(mismatch("shelf items", "two items", items.len()));
    };
    let area = RECT { left: first[0], top: first[1], right: first[2], bottom: first[3] };
    let on = |rect: [i32; 4]| POINT { x: rect[0] + BADGE_OFFSET, y: (rect[1] + rect[3]) / 2 };
    input::move_to(on(second));
    pause(TOOLTIP_FREE)?;
    let entering = Transition::capture(area, || {
        input::move_to(on(first));
        Ok(())
    })?;
    entering.verify(ctx, &format!("{name}: hover"), &format!("{name}-enter"))?;
    let leaving = Transition::capture(area, || {
        input::move_to(on(second));
        Ok(())
    })?;
    leaving.verify(ctx, &format!("{name}: leave"), &format!("{name}-leave"))?;
    for mark in CONTROLS {
        let Ok(rect) = app.rect(&diag, mark) else {
            return Err(mismatch("element for the hover check", mark, "not drawn"));
        };
        let area = RECT { left: rect[0], top: rect[1], right: rect[2], bottom: rect[3] };
        let target = POINT { x: (rect[0] + rect[2]) / 2, y: (rect[1] + rect[3]) / 2 };
        input::move_to(on(second));
        pause(TOOLTIP_FREE)?;
        let entering = Transition::capture(area, || {
            input::move_to(target);
            Ok(())
        })?;
        entering.verify(ctx, &format!("{name}, {mark}: hover"), &format!("{name}-{mark}-enter"))?;
        let leaving = Transition::capture(area, || {
            input::move_to(on(second));
            Ok(())
        })?;
        leaving.verify(ctx, &format!("{name}, {mark}: leave"), &format!("{name}-{mark}-leave"))?;
    }
    Ok(())
}

fn cancel_steady(ctx: &Ctx) -> Result<()> {
    ctx.reset()?;
    let mut settings = app::settings(ctx);
    settings["window_animations"] = true.into();
    ctx.start(settings, vec![], zip_tile(), &["--autostart"])?;
    pause(500)?;
    helper::arm(Payload::Text("E2E: cancel during the animation".into()));
    let from = center(ctx.layout.helper);
    let nudge = |by: i32| POINT { x: from.x + by, y: from.y + by };
    input::press(from)?;
    for by in [4, 8, 12, 18, 24] {
        input::move_to(nudge(by));
        pause(40)?;
    }
    let app = ctx.app()?;
    let mut tick = 0;
    wait("the panel showed the drop zones", Duration::from_secs(6), || {
        tick += 1;
        input::move_to(nudge(24 + tick % 2));
        app.diag().filter(|d| app.shown() && d.dropping)
    })?;
    pause(400)?;
    input::right_click()?;
    let mut seen = Vec::new();
    let started = Instant::now();
    while app.shown() && started.elapsed() < Duration::from_secs(2) {
        if let Some(diag) = app.diag().filter(|_| app.shown()) {
            seen.push((diag.page.clone(), diag.dropping));
        }
        std::thread::sleep(Duration::from_millis(4));
    }
    app.wait_shown(false, "the panel hid on the right click", Duration::from_secs(4))?;
    input::move_to(nudge(24));
    input::release(nudge(24))?;
    let changed: Vec<_> = seen.iter().filter(|(page, dropping)| page != "shelf" || !dropping).collect();
    if !changed.is_empty() {
        return Err(mismatch("contents of the fading panel", "drop zones until hidden", format!("{changed:?}")));
    }
    Ok(())
}

fn run(s: &mut Session) {
    s.run("hover-smooth", |ctx| {
        launch(ctx, false)?;
        let plain = sweep(ctx)?;
        println!();
        println!("    without animations: {}", plain.summary());
        launch(ctx, true)?;
        let animated = sweep(ctx)?;
        within_budget("hover with animations", &animated)
    });

    s.run("fade-smooth", |ctx| {
        launch(ctx, true)?;
        let frames = toggles(ctx)?;
        within_budget("showing and hiding with animations", &frames)
    });

    s.run("hover-gradual", |ctx| {
        launch(ctx, false)?;
        let plain = hover_colors(ctx)?;
        if plain.resting == plain.hovered {
            return Err(mismatch("row hover highlight", "a different color", plain.summary()));
        }
        if plain.steps() > 0 || plain.settled().is_none_or(|at| at > HOVER_REACTION_MS) {
            return Err(mismatch(
                "highlight without animations",
                "instant, with no intermediate colors",
                plain.summary(),
            ));
        }
        launch(ctx, true)?;
        let animated = hover_colors(ctx)?;
        println!();
        println!("    with animations: {}", animated.summary());
        if animated.first_change().is_none_or(|at| at > HOVER_REACTION_MS) {
            return Err(mismatch(
                "highlight response with animations",
                format!("≤ {HOVER_REACTION_MS} ms"),
                animated.summary(),
            ));
        }
        if animated.steps() == 0 {
            return Err(mismatch("highlight with animations", "intermediate colors", animated.summary()));
        }
        if animated.settled().is_none_or(|at| at > HOVER_SETTLE_MS) {
            return Err(mismatch(
                "the animated highlight reached its end",
                format!("≤ {HOVER_SETTLE_MS} ms"),
                animated.summary(),
            ));
        }
        Ok(())
    });

    s.run("hover-clean", hover_clean);
    s.run("fade-gradual", fade_gradual);
    s.run("first-fade", first_fade);
    s.run("cancel-steady", cancel_steady);
}
