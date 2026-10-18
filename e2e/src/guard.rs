use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock, mpsc};
use std::time::{Duration, Instant};
use windows::Win32::{
    Foundation::*,
    System::{Console::SetConsoleCtrlHandler, LibraryLoader::GetModuleHandleW, Threading::GetCurrentThreadId},
    UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
};
use windows::core::BOOL;

pub const STOP_CHORD: &str = "Ctrl+Alt+Shift+Q";
const STOP_KEY: usize = b'Q' as usize;

static ARMED: AtomicBool = AtomicBool::new(false);
static ARMED_UNTIL: AtomicU64 = AtomicU64::new(0);
static TRIPPED: AtomicBool = AtomicBool::new(false);
static REASON: Mutex<Option<String>> = Mutex::new(None);
static THREAD: AtomicU32 = AtomicU32::new(0);
static EPOCH: OnceLock<Instant> = OnceLock::new();
static DOWN: [AtomicBool; 256] = [const { AtomicBool::new(false) }; 256];
static RELEASE: [AtomicBool; 256] = [const { AtomicBool::new(false) }; 256];

fn now() -> u64 {
    EPOCH.get_or_init(Instant::now).elapsed().as_millis() as u64
}

pub fn trip(reason: &str) {
    ARMED.store(false, Ordering::SeqCst);
    if !TRIPPED.swap(true, Ordering::SeqCst) {
        *REASON.lock().unwrap_or_else(|e| e.into_inner()) = Some(reason.to_string());
    }
}

pub fn tripped() -> Option<String> {
    TRIPPED.load(Ordering::SeqCst).then(|| REASON.lock().unwrap_or_else(|e| e.into_inner()).clone().unwrap_or_default())
}

pub fn arm(limit: Duration) {
    for (down, release) in DOWN.iter().zip(&RELEASE) {
        release.store(down.load(Ordering::SeqCst), Ordering::SeqCst);
    }
    ARMED_UNTIL.store(now() + limit.as_millis() as u64, Ordering::SeqCst);
    ARMED.store(true, Ordering::SeqCst);
}

fn locked() -> bool {
    ARMED.load(Ordering::SeqCst) && now() < ARMED_UNTIL.load(Ordering::SeqCst)
}

fn held(keys: [VIRTUAL_KEY; 2]) -> bool {
    keys.iter().any(|key| DOWN[key.0 as usize].load(Ordering::SeqCst))
}

fn stop_chord_held() -> bool {
    held([VK_LCONTROL, VK_RCONTROL]) && held([VK_LMENU, VK_RMENU]) && held([VK_LSHIFT, VK_RSHIFT])
}

fn physical(key: usize, up: bool) -> bool {
    DOWN[key].store(!up, Ordering::SeqCst);
    if !up && key == STOP_KEY && stop_chord_held() {
        trip(&format!("stopped with {STOP_CHORD}"));
        return false;
    }
    if !locked() {
        return true;
    }
    up && RELEASE[key].swap(false, Ordering::SeqCst)
}

fn button(message: u32, data: &MSLLHOOKSTRUCT) -> Option<(usize, bool)> {
    let extra = if (data.mouseData >> 16) as u16 == XBUTTON1 { VK_XBUTTON1 } else { VK_XBUTTON2 };
    match message {
        WM_LBUTTONDOWN => Some((VK_LBUTTON.0 as usize, false)),
        WM_LBUTTONUP => Some((VK_LBUTTON.0 as usize, true)),
        WM_RBUTTONDOWN => Some((VK_RBUTTON.0 as usize, false)),
        WM_RBUTTONUP => Some((VK_RBUTTON.0 as usize, true)),
        WM_MBUTTONDOWN => Some((VK_MBUTTON.0 as usize, false)),
        WM_MBUTTONUP => Some((VK_MBUTTON.0 as usize, true)),
        WM_XBUTTONDOWN => Some((extra.0 as usize, false)),
        WM_XBUTTONUP => Some((extra.0 as usize, true)),
        _ => None,
    }
}

unsafe extern "system" fn mouse(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code >= 0
        && let Some(data) = unsafe { (lp.0 as *const MSLLHOOKSTRUCT).as_ref() }
        && data.flags & (LLMHF_INJECTED | LLMHF_LOWER_IL_INJECTED) == 0
    {
        let pass = match button(wp.0 as u32, data) {
            Some((key, up)) => physical(key, up),
            None => !locked(),
        };
        if !pass {
            return LRESULT(1);
        }
    }
    unsafe { CallNextHookEx(None, code, wp, lp) }
}

unsafe extern "system" fn keyboard(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code >= 0
        && let Some(data) = unsafe { (lp.0 as *const KBDLLHOOKSTRUCT).as_ref() }
        && data.flags.0 & (LLKHF_INJECTED.0 | LLKHF_LOWER_IL_INJECTED.0) == 0
    {
        let up = matches!(wp.0 as u32, WM_KEYUP | WM_SYSKEYUP);
        if !physical(data.vkCode as usize & 0xFF, up) {
            return LRESULT(1);
        }
    }
    unsafe { CallNextHookEx(None, code, wp, lp) }
}

unsafe extern "system" fn console(_: u32) -> BOOL {
    trip("Ctrl+C in the console");
    BOOL(1)
}

pub fn start() -> anyhow::Result<()> {
    let (tx, rx) = mpsc::channel();
    now();
    std::thread::Builder::new().name("e2e-guard".into()).spawn(move || unsafe {
        THREAD.store(GetCurrentThreadId(), Ordering::SeqCst);
        let instance = GetModuleHandleW(None).unwrap_or_default();
        let hooks = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse), Some(instance.into()), 0)
            .and_then(|m| SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard), Some(instance.into()), 0).map(|k| (m, k)));
        let installed = hooks.is_ok();
        let _ = tx.send(hooks.as_ref().map(|_| ()).map_err(|e| e.to_string()));
        if !installed {
            return;
        }
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
            DispatchMessageW(&msg);
        }
        if let Ok((m, k)) = hooks {
            let _ = UnhookWindowsHookEx(m);
            let _ = UnhookWindowsHookEx(k);
        }
    })?;
    rx.recv()?.map_err(|e| anyhow::anyhow!("couldn't install input hooks: {e}"))?;
    unsafe { SetConsoleCtrlHandler(Some(console), true)? };
    Ok(())
}

pub fn stop() {
    ARMED.store(false, Ordering::SeqCst);
    let thread = THREAD.load(Ordering::SeqCst);
    if thread != 0 {
        unsafe {
            let _ = PostThreadMessageW(thread, WM_QUIT, WPARAM(0), LPARAM(0));
        }
    }
}
