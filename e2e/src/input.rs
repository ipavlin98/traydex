use crate::win;
use anyhow::{Result, bail};
use std::cell::RefCell;
use std::thread::sleep;
use std::time::Duration;
use windows::Win32::{
    Foundation::{HWND, POINT},
    UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
};

thread_local! {
    static PIDS: RefCell<Vec<u32>> = const { RefCell::new(Vec::new()) };
    static WINDOWS: RefCell<Vec<isize>> = const { RefCell::new(Vec::new()) };
    static HELD: RefCell<Held> = const { RefCell::new(Held { button: false, keys: Vec::new() }) };
}

struct Held {
    button: bool,
    keys: Vec<u16>,
}

pub fn allow_pid(pid: u32) {
    PIDS.with_borrow_mut(|pids| pids.push(pid));
}

pub fn allow_window(hwnd: HWND) {
    WINDOWS.with_borrow_mut(|windows| windows.push(hwnd.0 as isize));
}

pub fn forget_window(hwnd: HWND) {
    WINDOWS.with_borrow_mut(|windows| windows.retain(|w| *w != hwnd.0 as isize));
}

fn allowed(hwnd: HWND) -> bool {
    if hwnd.is_invalid() {
        return false;
    }
    let root = unsafe { GetAncestor(hwnd, GA_ROOT) };
    let root = if root.is_invalid() { hwnd } else { root };
    WINDOWS.with_borrow(|windows| windows.contains(&(root.0 as isize)))
        || PIDS.with_borrow(|pids| pids.contains(&win::pid(root)) || pids.contains(&win::pid(hwnd)))
}

fn ensure_point(point: POINT) -> Result<()> {
    let target = win::root_at(point);
    if !allowed(target) {
        bail!(
            "input cancelled: at ({}, {}) there is a foreign window \"{}\" ({})",
            point.x,
            point.y,
            win::title(target),
            win::class(target)
        );
    }
    Ok(())
}

fn ensure_foreground() -> Result<()> {
    let target = win::foreground();
    if !allowed(target) {
        bail!("key input cancelled: a foreign window \"{}\" ({}) is active", win::title(target), win::class(target));
    }
    Ok(())
}

fn send(inputs: &[INPUT]) {
    unsafe { SendInput(inputs, size_of::<INPUT>() as i32) };
}

fn mouse(flags: MOUSE_EVENT_FLAGS, dx: i32, dy: i32) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 { mi: MOUSEINPUT { dx, dy, dwFlags: flags, ..Default::default() } },
    }
}

fn key_input(key: u16, up: bool) -> INPUT {
    let extended = matches!(VIRTUAL_KEY(key), VK_LEFT | VK_RIGHT | VK_UP | VK_DOWN | VK_DELETE | VK_HOME | VK_END);
    let mut flags = if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) };
    if extended {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(key),
                wScan: unsafe { MapVirtualKeyW(key as u32, MAPVK_VK_TO_VSC) } as u16,
                dwFlags: flags,
                ..Default::default()
            },
        },
    }
}

fn absolute(point: POINT) -> (i32, i32) {
    let (left, top, width, height) = unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        )
    };
    let scale = |value: i32, origin: i32, span: i32| {
        (((value - origin) as i64 * 65535 + (span as i64 - 1) / 2) / (span as i64 - 1).max(1)) as i32
    };
    (scale(point.x, left, width), scale(point.y, top, height))
}

pub fn move_to(point: POINT) {
    let (x, y) = absolute(point);
    send(&[mouse(MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK, x, y)]);
}

pub fn glide(to: POINT, steps: u32, pause: Duration) -> Result<()> {
    let from = win::cursor();
    for step in 1..=steps.max(1) {
        crate::check()?;
        let t = step as f32 / steps.max(1) as f32;
        move_to(POINT {
            x: from.x + ((to.x - from.x) as f32 * t).round() as i32,
            y: from.y + ((to.y - from.y) as f32 * t).round() as i32,
        });
        sleep(pause);
    }
    Ok(())
}

pub fn press(point: POINT) -> Result<()> {
    crate::check()?;
    ensure_point(point)?;
    move_to(point);
    sleep(Duration::from_millis(60));
    ensure_point(point)?;
    HELD.with_borrow_mut(|held| held.button = true);
    send(&[mouse(MOUSEEVENTF_LEFTDOWN, 0, 0)]);
    sleep(Duration::from_millis(60));
    Ok(())
}

pub fn hover(point: POINT) -> Result<()> {
    crate::check()?;
    ensure_point(point)?;
    move_to(point);
    sleep(Duration::from_millis(30));
    let actual = win::cursor();
    if (actual.x - point.x).abs() > 1 || (actual.y - point.y).abs() > 1 {
        bail!("the cursor didn't reach ({}, {}): it's at ({}, {})", point.x, point.y, actual.x, actual.y);
    }
    Ok(())
}

pub fn wheel(point: POINT, notches: i32) -> Result<()> {
    hover(point)?;
    let delta = (-notches * WHEEL_DELTA as i32) as u32;
    send(&[INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 { mi: MOUSEINPUT { mouseData: delta, dwFlags: MOUSEEVENTF_WHEEL, ..Default::default() } },
    }]);
    sleep(Duration::from_millis(60));
    Ok(())
}

pub fn text(text: &str) -> Result<()> {
    crate::check()?;
    ensure_foreground()?;
    for unit in text.encode_utf16() {
        let key = |up: bool| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wScan: unit,
                    dwFlags: if up { KEYEVENTF_UNICODE | KEYEVENTF_KEYUP } else { KEYEVENTF_UNICODE },
                    ..Default::default()
                },
            },
        };
        send(&[key(false), key(true)]);
        sleep(Duration::from_millis(15));
    }
    Ok(())
}

pub fn right_click_at(point: POINT) -> Result<()> {
    hover(point)?;
    right_click()
}

pub fn release(point: POINT) -> Result<()> {
    crate::check()?;
    ensure_point(point)?;
    send(&[mouse(MOUSEEVENTF_LEFTUP, 0, 0)]);
    HELD.with_borrow_mut(|held| held.button = false);
    sleep(Duration::from_millis(60));
    Ok(())
}

pub fn right_click() -> Result<()> {
    crate::check()?;
    send(&[mouse(MOUSEEVENTF_RIGHTDOWN, 0, 0)]);
    sleep(Duration::from_millis(60));
    send(&[mouse(MOUSEEVENTF_RIGHTUP, 0, 0)]);
    sleep(Duration::from_millis(60));
    Ok(())
}

pub fn click(point: POINT) -> Result<()> {
    press(point)?;
    release(point)
}

pub fn drag(from: POINT, to: POINT, steps: u32) -> Result<()> {
    press(from)?;
    for offset in [3, 6, 10, 16] {
        crate::check()?;
        move_to(POINT { x: from.x + offset, y: from.y + offset });
        sleep(Duration::from_millis(40));
    }
    glide(to, steps, Duration::from_millis(25))?;
    sleep(Duration::from_millis(250));
    release(to)
}

pub fn chord(keys: &[VIRTUAL_KEY]) -> Result<()> {
    crate::check()?;
    ensure_foreground()?;
    for key in keys {
        HELD.with_borrow_mut(|held| held.keys.push(key.0));
        send(&[key_input(key.0, false)]);
        sleep(Duration::from_millis(30));
    }
    for key in keys.iter().rev() {
        send(&[key_input(key.0, true)]);
        HELD.with_borrow_mut(|held| held.keys.retain(|k| *k != key.0));
        sleep(Duration::from_millis(30));
    }
    sleep(Duration::from_millis(80));
    Ok(())
}

pub fn double_click_time() -> u64 {
    unsafe { GetDoubleClickTime() as u64 }
}

pub fn drag_threshold() -> i32 {
    unsafe { GetSystemMetrics(SM_CXDRAG).max(GetSystemMetrics(SM_CYDRAG)) }
}

pub fn release_all() {
    let (button, keys) =
        HELD.with_borrow_mut(|held| (std::mem::take(&mut held.button), std::mem::take(&mut held.keys)));
    if button {
        send(&[key_input(VK_ESCAPE.0, false), key_input(VK_ESCAPE.0, true)]);
        sleep(Duration::from_millis(100));
        send(&[mouse(MOUSEEVENTF_LEFTUP, 0, 0)]);
    }
    for key in keys.into_iter().rev() {
        send(&[key_input(key, true)]);
    }
}
