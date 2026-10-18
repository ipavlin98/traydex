use super::dnd::center;
use super::{Session, Spec, Suite, none, spec};
use crate::{Ctx, app, expect_eq, input, mismatch, pause, wait, win};
use anyhow::{Context, Result};
use std::time::Duration;
use windows::Win32::Foundation::{POINT, RECT};
use windows::Win32::UI::Input::KeyboardAndMouse::{VK_CONTROL, VK_ESCAPE, VK_F9, VK_MENU, VK_SHIFT};

pub const SUITE: Suite = Suite { id: "input", title: "Input and window", specs, run };

fn specs() -> Vec<Spec> {
    vec![
        spec("hotkey", "The global shortcut shows and hides the panel", true),
        spec("dismiss", "Esc and a click outside hide the panel, a pinned one stays", true),
        spec("hot-corner", "The hot corner shows the panel", true),
        spec("move-resize", "Position and size survive a restart", true),
        spec("cursor-shape", "The cursor changes shape on the first mouse move, not the next one", true),
    ]
}

const TOGGLE: [windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY; 4] = [VK_CONTROL, VK_SHIFT, VK_MENU, VK_F9];

fn focus_helper(ctx: &Ctx) -> Result<()> {
    input::click(center(ctx.layout.helper))?;
    wait("the helper window is active", Duration::from_secs(3), || {
        (win::foreground() == crate::helper::window()).then_some(())
    })?;
    Ok(())
}

fn toggle(ctx: &Ctx, shown: bool) -> Result<()> {
    input::chord(&TOGGLE)?;
    ctx.app()?.wait_shown(
        shown,
        if shown { "the panel was shown by the shortcut" } else { "the panel was hidden by the shortcut" },
        Duration::from_secs(4),
    )
}

fn caption(ctx: &Ctx) -> Result<POINT> {
    let app = ctx.app()?;
    let diag = app.wait_diag("the panel title bar is drawn", Duration::from_secs(5), |d| {
        d.has("home") && d.has("title-buttons")
    })?;
    let home = app.rect(&diag, "home")?;
    let buttons = app.rect(&diag, "title-buttons")?;
    let bar = app.rect(&diag, "title-bar")?;
    Ok(POINT { x: (home[2] + buttons[0]) / 2, y: (bar[1] + bar[3]) / 2 })
}

fn activate(ctx: &Ctx) -> Result<()> {
    let point = caption(ctx)?;
    input::click(point)?;
    let pid = ctx.app()?.pid;
    wait("the panel is active", Duration::from_secs(3), || (win::pid(win::foreground()) == pid).then_some(()))?;
    Ok(())
}

pub(super) fn outer_corners() -> Vec<(&'static str, POINT, (i32, i32))> {
    let (screen, _) = win::monitor(POINT { x: 0, y: 0 });
    let corners = [
        ("TopLeft", POINT { x: screen.left, y: screen.top }, (-1, -1)),
        ("TopRight", POINT { x: screen.right - 1, y: screen.top }, (1, -1)),
        ("BottomRight", POINT { x: screen.right - 1, y: screen.bottom - 1 }, (1, 1)),
        ("BottomLeft", POINT { x: screen.left, y: screen.bottom - 1 }, (-1, 1)),
    ];
    corners
        .into_iter()
        .filter(|(_, tip, (dx, dy))| {
            !win::on_desktop(POINT { x: tip.x + dx, y: tip.y }) && !win::on_desktop(POINT { x: tip.x, y: tip.y + dy })
        })
        .collect()
}

fn width(rect: RECT) -> i32 {
    rect.right - rect.left
}

fn height(rect: RECT) -> i32 {
    rect.bottom - rect.top
}

fn settled(panel: windows::Win32::Foundation::HWND) -> Result<RECT> {
    let mut last = win::client(panel);
    wait("the panel stopped moving", Duration::from_secs(3), || {
        std::thread::sleep(Duration::from_millis(150));
        let now = win::client(panel);
        let still = now == last;
        last = now;
        still.then_some(now)
    })
}

fn show_rect(rect: RECT) -> String {
    format!("({}, {}) {}×{}", rect.left, rect.top, rect.right - rect.left, rect.bottom - rect.top)
}

fn pointer_after(point: POINT, what: &str, expected: win::Pointer) -> Result<()> {
    input::hover(point)?;
    pause(400)?;
    expect_eq(what, expected, win::pointer())
}

fn run(s: &mut Session) {
    s.run("hotkey", |ctx| {
        ctx.reset()?;
        ctx.start(app::settings(ctx), vec![], none(), &["--autostart"])?;
        pause(600)?;
        focus_helper(ctx)?;
        toggle(ctx, true)?;
        pause(700)?;
        toggle(ctx, false)?;
        Ok(())
    });

    s.run("dismiss", |ctx| {
        ctx.reset()?;
        ctx.start(app::settings(ctx), vec![], none(), &[])?;
        ctx.app()?.wait_shown(true, "the panel is visible", Duration::from_secs(5))?;
        activate(ctx)?;
        input::chord(&[VK_ESCAPE])?;
        ctx.app()?.wait_shown(false, "Esc hid the panel", Duration::from_secs(4))?;
        pause(600)?;
        focus_helper(ctx)?;
        toggle(ctx, true)?;
        pause(400)?;
        input::click(center(ctx.layout.helper))?;
        ctx.app()?.wait_shown(false, "a click outside hid the panel", Duration::from_secs(4))?;
        pause(700)?;
        toggle(ctx, true)?;
        pause(400)?;
        let pin = {
            let app = ctx.app()?;
            let diag = app.wait_diag("the pin button is drawn", Duration::from_secs(5), |d| d.has("pin"))?;
            app.point(&diag, "pin")?
        };
        input::click(pin)?;
        ctx.app()?.wait_diag("the panel is pinned", Duration::from_secs(4), |d| d.pinned)?;
        input::click(center(ctx.layout.helper))?;
        pause(1500)?;
        if !ctx.app()?.shown() {
            return Err(mismatch("the pinned panel hid after a click outside", "visible", "hidden"));
        }
        input::click(pin)?;
        ctx.app()?.wait_diag("unpinned", Duration::from_secs(4), |d| !d.pinned)?;
        input::click(center(ctx.layout.helper))?;
        ctx.app()?.wait_shown(false, "after unpinning, a click outside hid the panel", Duration::from_secs(4))?;
        Ok(())
    });

    s.run("hot-corner", |ctx| {
        let (corner, tip, _) = outer_corners()
            .into_iter()
            .next()
            .context("the primary monitor has no outer corner (every corner borders another monitor)")?;
        ctx.reset()?;
        let mut settings = app::settings(ctx);
        settings["corner"] = corner.into();
        ctx.start(settings, vec![], none(), &["--autostart"])?;
        pause(600)?;
        let away = center(ctx.layout.helper);
        input::glide(away, 10, Duration::from_millis(15))?;
        pause(300)?;
        input::glide(tip, 30, Duration::from_millis(15))?;
        ctx.app()?.wait_shown(true, &format!("hot corner {corner} showed the panel"), Duration::from_secs(4))?;
        input::glide(away, 20, Duration::from_millis(15))?;
        Ok(())
    });

    s.run("cursor-shape", |ctx| {
        ctx.reset()?;
        ctx.start(app::settings(ctx), vec![], none(), &[])?;
        let app = ctx.app()?;
        app.wait_shown(true, "the panel is visible", Duration::from_secs(5))?;
        app.post_click("actions")?;
        let diag = app.wait_diag("the tiles page is open", Duration::from_secs(5), |d| {
            d.page == "actions" && d.has("chip:add-folder")
        })?;
        let [left, top, right, bottom] = app.rect(&diag, "chip:add-folder")?;
        let y = (top + bottom) / 2;
        let beside = POINT { x: left - ctx.layout.dpi as i32 * 8 / 96, y };
        let inside = POINT { x: (left + right) / 2, y };
        pointer_after(beside, "cursor in the page margin", win::Pointer::Arrow)?;
        pointer_after(inside, "cursor on a card after one move", win::Pointer::Hand)?;
        pointer_after(beside, "cursor back in the margin after one move", win::Pointer::Arrow)?;
        Ok(())
    });

    s.run("move-resize", |ctx| {
        ctx.reset()?;
        ctx.start(app::settings(ctx), vec![], none(), &[])?;
        let before = {
            let app = ctx.app()?;
            app.wait_shown(true, "the panel is visible", Duration::from_secs(5))?;
            win::client(app.panel().context("panel not found")?)
        };
        let grab = caption(ctx)?;
        let shift = POINT { x: -(ctx.layout.dpi as i32) * 120 / 96, y: ctx.layout.dpi as i32 * 60 / 96 };
        let slack = 2 * input::drag_threshold() + 2;
        let panel = ctx.app()?.panel().context("panel not found")?;
        input::drag(grab, POINT { x: grab.x + shift.x, y: grab.y + shift.y }, 20)?;
        let moved = settled(panel)?;
        let near = |a: i32, b: i32| (a - b).abs() <= slack;
        if !near(moved.left, before.left + shift.x)
            || !near(moved.top, before.top + shift.y)
            || width(moved) != width(before)
            || height(moved) != height(before)
        {
            return Err(mismatch(
                &format!("panel position after dragging the title bar (tolerance {slack} px, the drag threshold)"),
                show_rect(RECT {
                    left: before.left + shift.x,
                    top: before.top + shift.y,
                    right: before.right + shift.x,
                    bottom: before.bottom + shift.y,
                }),
                show_rect(moved),
            ));
        }
        let grow = ctx.layout.dpi as i32 * 50 / 96;
        let corner = POINT { x: moved.right - 3, y: moved.bottom - 3 };
        input::drag(corner, POINT { x: corner.x + grow, y: corner.y + grow }, 20)?;
        let resized = settled(panel)?;
        if resized.left != moved.left
            || resized.top != moved.top
            || !near(width(resized), width(moved) + grow)
            || !near(height(resized), height(moved) + grow)
        {
            return Err(mismatch(
                &format!("panel size after dragging the corner (tolerance {slack} px)"),
                show_rect(RECT { right: moved.right + grow, bottom: moved.bottom + grow, ..moved }),
                show_rect(resized),
            ));
        }
        let scale = ctx.layout.dpi as f64 / 96.;
        let expected_size = (width(resized) as f64 / scale, height(resized) as f64 / scale);
        wait("position and size written to shelf.json", Duration::from_secs(4), || {
            let saved = ctx.app().ok()?.saved()?;
            let position = (saved["position"][0].as_i64()?, saved["position"][1].as_i64()?);
            let size = (saved["size"][0].as_f64()?, saved["size"][1].as_f64()?);
            (position == (resized.left as i64, resized.top as i64)
                && (size.0 - expected_size.0).abs() < 1.
                && (size.1 - expected_size.1).abs() < 1.)
                .then_some(())
        })?;
        ctx.quit()?;
        let args: Vec<&std::ffi::OsStr> = Vec::new();
        *ctx.app.borrow_mut() = Some(app::App::launch(ctx, &args)?);
        let app = ctx.app()?;
        app.wait_shown(true, "the panel is visible after the restart", Duration::from_secs(5))?;
        let panel = app.panel().context("panel not found")?;
        wait("position and size restored", Duration::from_secs(3), || (win::client(panel) == resized).then_some(()))
            .map_err(|_| {
                mismatch("position and size after the restart", show_rect(resized), show_rect(win::client(panel)))
            })?;
        expect_eq("the panel is in place", true, app.shown())?;
        Ok(())
    });
}
