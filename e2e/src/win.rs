use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::Duration;
use windows::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    Storage::Xps::{PRINT_WINDOW_FLAGS, PrintWindow},
    System::{
        DataExchange::*,
        Diagnostics::ToolHelp::*,
        Memory::{GlobalLock, GlobalUnlock},
        Ole::CF_UNICODETEXT,
        Registry::*,
        Threading::*,
    },
    UI::{
        Input::KeyboardAndMouse::*,
        Shell::{DragQueryFileW, HDROP, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW},
        WindowsAndMessaging::*,
    },
};
use windows::core::{BOOL, HSTRING, PCWSTR, PWSTR};

const PW_RENDERFULLCONTENT: u32 = 2;

pub fn windows() -> Vec<HWND> {
    unsafe extern "system" fn collect(hwnd: HWND, list: LPARAM) -> BOOL {
        unsafe { (*(list.0 as *mut Vec<HWND>)).push(hwnd) };
        BOOL(1)
    }
    let mut list: Vec<HWND> = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(collect), LPARAM(&mut list as *mut _ as isize));
    }
    list
}

pub fn pid(hwnd: HWND) -> u32 {
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    pid
}

pub fn title(hwnd: HWND) -> String {
    let mut buffer = [0u16; 512];
    let length = unsafe { GetWindowTextW(hwnd, &mut buffer) } as usize;
    String::from_utf16_lossy(&buffer[..length.min(buffer.len())])
}

pub fn class(hwnd: HWND) -> String {
    let mut buffer = [0u16; 256];
    let length = unsafe { GetClassNameW(hwnd, &mut buffer) } as usize;
    String::from_utf16_lossy(&buffer[..length.min(buffer.len())])
}

pub fn visible(hwnd: HWND) -> bool {
    unsafe { IsWindowVisible(hwnd).as_bool() }
}

pub fn exists(hwnd: HWND) -> bool {
    unsafe { IsWindow(Some(hwnd)).as_bool() }
}

pub fn rect(hwnd: HWND) -> RECT {
    let mut rect = RECT::default();
    unsafe {
        let _ = GetWindowRect(hwnd, &mut rect);
    }
    rect
}

pub fn client(hwnd: HWND) -> RECT {
    let mut rect = RECT::default();
    let mut origin = POINT::default();
    unsafe {
        let _ = GetClientRect(hwnd, &mut rect);
        let _ = ClientToScreen(hwnd, &mut origin);
    }
    RECT { left: origin.x, top: origin.y, right: origin.x + rect.right, bottom: origin.y + rect.bottom }
}

pub fn root_at(point: POINT) -> HWND {
    unsafe { GetAncestor(WindowFromPoint(point), GA_ROOT) }
}

pub fn foreground() -> HWND {
    unsafe { GetForegroundWindow() }
}

pub fn cursor() -> POINT {
    let mut point = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut point);
    }
    point
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Pointer {
    Arrow,
    Hand,
    Other,
}

pub fn pointer() -> Pointer {
    let mut info = CURSORINFO { cbSize: size_of::<CURSORINFO>() as u32, ..Default::default() };
    if unsafe { GetCursorInfo(&mut info) }.is_err() {
        return Pointer::Other;
    }
    let shared = |id: PCWSTR| unsafe { LoadCursorW(None, id) }.ok();
    match Some(info.hCursor) {
        shape if shape == shared(IDC_ARROW) => Pointer::Arrow,
        shape if shape == shared(IDC_HAND) => Pointer::Hand,
        _ => Pointer::Other,
    }
}

pub fn inside(rect: &RECT, point: POINT) -> bool {
    point.x >= rect.left && point.x < rect.right && point.y >= rect.top && point.y < rect.bottom
}

pub fn post(hwnd: HWND, message: u32, wp: usize, lp: isize) -> bool {
    unsafe { PostMessageW(Some(hwnd), message, WPARAM(wp), LPARAM(lp)) }.is_ok()
}

fn pack(x: i32, y: i32) -> isize {
    ((y as u16 as isize) << 16) | x as u16 as isize
}

pub fn post_click(hwnd: HWND, x: i32, y: i32, right: bool) {
    let point = pack(x, y);
    let (down, up, held) = if right { (WM_RBUTTONDOWN, WM_RBUTTONUP, 2) } else { (WM_LBUTTONDOWN, WM_LBUTTONUP, 1) };
    post(hwnd, WM_MOUSEMOVE, 0, point);
    sleep(Duration::from_millis(120));
    post(hwnd, down, held, point);
    sleep(Duration::from_millis(80));
    post(hwnd, up, 0, point);
    sleep(Duration::from_millis(120));
}

pub fn post_leave(hwnd: HWND) {
    post(hwnd, 0x02A3, 0, 0);
}

pub fn post_key(hwnd: HWND, key: VIRTUAL_KEY) {
    post(hwnd, WM_KEYDOWN, key.0 as usize, 1);
    sleep(Duration::from_millis(60));
    post(hwnd, WM_KEYUP, key.0 as usize, (1 | (3 << 30)) as isize);
    sleep(Duration::from_millis(60));
}

pub fn capture(hwnd: HWND) -> Option<image::RgbaImage> {
    let rect = rect(hwnd);
    let (width, height) = (rect.right - rect.left, rect.bottom - rect.top);
    if width <= 0 || height <= 0 {
        return None;
    }
    unsafe {
        let screen = GetDC(None);
        let memory = CreateCompatibleDC(Some(screen));
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits = std::ptr::null_mut();
        let result =
            CreateDIBSection(Some(memory), &info, DIB_RGB_COLORS, &mut bits, None, 0).ok().and_then(|bitmap| {
                let previous = SelectObject(memory, bitmap.into());
                let printed = PrintWindow(hwnd, memory, PRINT_WINDOW_FLAGS(PW_RENDERFULLCONTENT)).as_bool();
                let _ = GdiFlush();
                let image = printed.then(|| {
                    let raw = std::slice::from_raw_parts(bits as *const u8, (width * height * 4) as usize);
                    let mut pixels = Vec::with_capacity(raw.len());
                    for chunk in raw.as_chunks::<4>().0 {
                        pixels.extend_from_slice(&[chunk[2], chunk[1], chunk[0], 255]);
                    }
                    image::RgbaImage::from_raw(width as u32, height as u32, pixels)
                });
                SelectObject(memory, previous);
                let _ = DeleteObject(bitmap.into());
                image.flatten()
            });
        let _ = DeleteDC(memory);
        ReleaseDC(None, screen);
        result
    }
}

pub fn screen_region(area: RECT) -> Option<image::RgbaImage> {
    let (width, height) = (area.right - area.left, area.bottom - area.top);
    if width <= 0 || height <= 0 {
        return None;
    }
    unsafe {
        let screen = GetDC(None);
        let memory = CreateCompatibleDC(Some(screen));
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits = std::ptr::null_mut();
        let result =
            CreateDIBSection(Some(memory), &info, DIB_RGB_COLORS, &mut bits, None, 0).ok().and_then(|bitmap| {
                let previous = SelectObject(memory, bitmap.into());
                let copied =
                    BitBlt(memory, 0, 0, width, height, Some(screen), area.left, area.top, SRCCOPY | CAPTUREBLT)
                        .is_ok();
                let _ = GdiFlush();
                let image = copied.then(|| {
                    let raw = std::slice::from_raw_parts(bits as *const u8, (width * height * 4) as usize);
                    let mut pixels = Vec::with_capacity(raw.len());
                    for chunk in raw.as_chunks::<4>().0 {
                        pixels.extend_from_slice(&[chunk[2], chunk[1], chunk[0], 255]);
                    }
                    image::RgbaImage::from_raw(width as u32, height as u32, pixels)
                });
                SelectObject(memory, previous);
                let _ = DeleteObject(bitmap.into());
                image.flatten()
            });
        let _ = DeleteDC(memory);
        ReleaseDC(None, screen);
        result
    }
}

pub fn screen_pixel(point: POINT) -> Option<[u8; 3]> {
    unsafe {
        let screen = GetDC(None);
        let color = GetPixel(screen, point.x, point.y);
        ReleaseDC(None, screen);
        (color.0 != CLR_INVALID).then_some([color.0 as u8, (color.0 >> 8) as u8, (color.0 >> 16) as u8])
    }
}

pub fn refresh_rate() -> Option<u32> {
    let mut mode = DEVMODEW { dmSize: size_of::<DEVMODEW>() as u16, ..Default::default() };
    unsafe { EnumDisplaySettingsW(PCWSTR::null(), ENUM_CURRENT_SETTINGS, &mut mode) }
        .as_bool()
        .then_some(mode.dmDisplayFrequency)
        .filter(|hz| *hz > 1)
}

pub fn monitor(point: POINT) -> (RECT, RECT) {
    unsafe {
        let handle = MonitorFromPoint(point, MONITOR_DEFAULTTOPRIMARY);
        let mut info = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
        let _ = GetMonitorInfoW(handle, &mut info);
        (info.rcMonitor, info.rcWork)
    }
}

pub fn on_desktop(point: POINT) -> bool {
    !unsafe { MonitorFromPoint(point, MONITOR_DEFAULTTONULL) }.is_invalid()
}

fn open_clipboard() -> bool {
    for _ in 0..40 {
        if unsafe { OpenClipboard(None) }.is_ok() {
            return true;
        }
        sleep(Duration::from_millis(50));
    }
    false
}

pub fn clear_clipboard() -> bool {
    if !open_clipboard() {
        return false;
    }
    let cleared = unsafe { EmptyClipboard() }.is_ok();
    let _ = unsafe { CloseClipboard() };
    cleared
}

pub fn clipboard_text() -> Option<String> {
    if !open_clipboard() {
        return None;
    }
    let text = unsafe {
        GetClipboardData(CF_UNICODETEXT.0 as u32).ok().and_then(|handle| {
            let global = HGLOBAL(handle.0);
            let pointer = GlobalLock(global) as *const u16;
            if pointer.is_null() {
                return None;
            }
            let mut length = 0;
            while *pointer.add(length) != 0 {
                length += 1;
            }
            let text = String::from_utf16_lossy(std::slice::from_raw_parts(pointer, length));
            let _ = GlobalUnlock(global);
            Some(text)
        })
    };
    let _ = unsafe { CloseClipboard() };
    text
}

pub fn clipboard_files() -> Option<Vec<PathBuf>> {
    if !open_clipboard() {
        return None;
    }
    let files = unsafe {
        GetClipboardData(15).ok().map(|handle| {
            let drop = HDROP(handle.0);
            let count = DragQueryFileW(drop, u32::MAX, None);
            (0..count)
                .map(|index| {
                    let length = DragQueryFileW(drop, index, None) as usize;
                    let mut buffer = vec![0u16; length + 1];
                    DragQueryFileW(drop, index, Some(&mut buffer));
                    PathBuf::from(OsString::from_wide(&buffer[..length]))
                })
                .collect()
        })
    };
    let _ = unsafe { CloseClipboard() };
    files
}

pub fn image_path(pid: u32) -> Option<PathBuf> {
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buffer = [0u16; 1024];
        let mut length = buffer.len() as u32;
        let queried = QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, PWSTR(buffer.as_mut_ptr()), &mut length);
        let _ = CloseHandle(handle);
        queried.ok()?;
        Some(PathBuf::from(OsString::from_wide(&buffer[..length as usize])))
    }
}

pub fn processes() -> Vec<(u32, String)> {
    let mut found = Vec::new();
    unsafe {
        let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
            return found;
        };
        let mut entry = PROCESSENTRY32W { dwSize: size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut ok = Process32FirstW(snapshot, &mut entry).is_ok();
        while ok {
            let length = entry.szExeFile.iter().position(|c| *c == 0).unwrap_or(entry.szExeFile.len());
            found.push((entry.th32ProcessID, String::from_utf16_lossy(&entry.szExeFile[..length])));
            ok = Process32NextW(snapshot, &mut entry).is_ok();
        }
        let _ = CloseHandle(snapshot);
    }
    found
}

pub fn processes_under(folder: &Path) -> Vec<u32> {
    let folder = folder.to_string_lossy().to_lowercase();
    processes()
        .into_iter()
        .filter_map(|(pid, _)| {
            let path = image_path(pid)?.to_string_lossy().to_lowercase();
            path.starts_with(&folder).then_some(pid)
        })
        .collect()
}

pub fn alive(pid: u32) -> bool {
    unsafe {
        let Ok(handle) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE, false, pid) else {
            return false;
        };
        let running = WaitForSingleObject(handle, 0) == WAIT_TIMEOUT;
        let _ = CloseHandle(handle);
        running
    }
}

pub fn terminate(pid: u32) {
    unsafe {
        if let Ok(handle) = OpenProcess(PROCESS_TERMINATE, false, pid) {
            let _ = TerminateProcess(handle, 1);
            let _ = CloseHandle(handle);
        }
    }
}

pub fn mutex_exists(name: &str) -> bool {
    match unsafe {
        windows::Win32::System::Threading::OpenMutexW(SYNCHRONIZATION_SYNCHRONIZE, false, &HSTRING::from(name))
    } {
        Ok(handle) => {
            let _ = unsafe { CloseHandle(handle) };
            true
        }
        Err(_) => false,
    }
}

pub fn find_class(class: &str) -> Option<HWND> {
    unsafe { FindWindowW(&HSTRING::from(class), PCWSTR::null()) }.ok().filter(|hwnd| !hwnd.is_invalid())
}

pub fn hotkey_free(modifiers: HOT_KEY_MODIFIERS, key: VIRTUAL_KEY) -> bool {
    const PROBE: i32 = 0x7E2E;
    match unsafe { RegisterHotKey(None, PROBE, modifiers, key.0 as u32) } {
        Ok(()) => {
            let _ = unsafe { UnregisterHotKey(None, PROBE) };
            true
        }
        Err(_) => false,
    }
}

pub fn input_desktop_available() -> bool {
    use windows::Win32::System::StationsAndDesktops::*;
    match unsafe { OpenInputDesktop(DESKTOP_CONTROL_FLAGS(0), false, DESKTOP_ACCESS_FLAGS(DESKTOP_SWITCHDESKTOP.0)) } {
        Ok(desktop) => {
            let _ = unsafe { CloseDesktop(desktop) };
            true
        }
        Err(_) => false,
    }
}

pub fn reg_read(key: &str, name: &str) -> Option<String> {
    let (key, name) = (HSTRING::from(key), HSTRING::from(name));
    let mut size = 0u32;
    unsafe { RegGetValueW(HKEY_CURRENT_USER, &key, &name, RRF_RT_REG_SZ, None, None, Some(&mut size)) }.ok().ok()?;
    let mut buffer = vec![0u16; size as usize / 2 + 1];
    let mut size = (buffer.len() * 2) as u32;
    unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            &key,
            &name,
            RRF_RT_REG_SZ,
            None,
            Some(buffer.as_mut_ptr() as _),
            Some(&mut size),
        )
    }
    .ok()
    .ok()?;
    let length = buffer.iter().position(|c| *c == 0).unwrap_or(buffer.len());
    Some(String::from_utf16_lossy(&buffer[..length]))
}

pub fn reg_read_machine(key: &str, name: &str) -> Option<String> {
    let (key, name) = (HSTRING::from(key), HSTRING::from(name));
    let mut buffer = vec![0u16; 256];
    let mut size = (buffer.len() * 2) as u32;
    unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            &key,
            &name,
            RRF_RT_REG_SZ,
            None,
            Some(buffer.as_mut_ptr() as _),
            Some(&mut size),
        )
    }
    .ok()
    .ok()?;
    let length = buffer.iter().position(|c| *c == 0).unwrap_or(buffer.len());
    Some(String::from_utf16_lossy(&buffer[..length]))
}

pub fn reg_dword(key: &str, name: &str) -> Option<u32> {
    let mut value = 0u32;
    let mut size = 4u32;
    unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            &HSTRING::from(key),
            &HSTRING::from(name),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut value as *mut u32 as _),
            Some(&mut size),
        )
    }
    .ok()
    .ok()?;
    Some(value)
}

pub fn reg_key_exists(key: &str) -> bool {
    let mut handle = HKEY::default();
    let opened = unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, &HSTRING::from(key), None, KEY_READ, &mut handle) };
    if opened == ERROR_SUCCESS {
        let _ = unsafe { RegCloseKey(handle) };
        return true;
    }
    false
}

pub fn reg_delete_value(key: &str, name: &str) {
    let _ = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, &HSTRING::from(key), &HSTRING::from(name)) };
}

pub fn reg_delete_tree(key: &str) {
    let _ = unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, &HSTRING::from(key)) };
    let _ = unsafe { RegDeleteKeyW(HKEY_CURRENT_USER, &HSTRING::from(key)) };
}

pub fn shell_verb(verb: &str, file: &Path) -> anyhow::Result<()> {
    let (verb, file) = (HSTRING::from(verb), HSTRING::from(file.as_os_str()));
    let mut info = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOASYNC | SEE_MASK_NOCLOSEPROCESS,
        lpVerb: PCWSTR(verb.as_ptr()),
        lpFile: PCWSTR(file.as_ptr()),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };
    unsafe { ShellExecuteExW(&mut info)? };
    if !info.hProcess.is_invalid() {
        unsafe {
            WaitForSingleObject(info.hProcess, 15_000);
            let _ = CloseHandle(info.hProcess);
        }
    }
    Ok(())
}

pub fn place(hwnd: HWND, rect: RECT) {
    unsafe {
        let _ = ShowWindow(hwnd, SW_RESTORE);
        let _ = SetWindowPos(
            hwnd,
            None,
            rect.left,
            rect.top,
            rect.right - rect.left,
            rect.bottom - rect.top,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}

pub fn set_topmost(hwnd: HWND) {
    unsafe {
        let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
    }
}

pub fn topmost(hwnd: HWND) -> bool {
    unsafe { GetWindowLongW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOPMOST.0 != 0 }
}

pub fn set_foreground(hwnd: HWND) -> bool {
    unsafe { SetForegroundWindow(hwnd).as_bool() }
}

pub fn post_char(hwnd: HWND, unit: u16) {
    post(hwnd, WM_CHAR, unit as usize, 1);
}

pub fn post_wheel(hwnd: HWND, screen: POINT, notches: i32) {
    let delta = (-notches * WHEEL_DELTA as i32) as i16 as u16 as usize;
    post(hwnd, WM_MOUSEWHEEL, delta << 16, pack(screen.x, screen.y));
}

pub fn close(hwnd: HWND) {
    post(hwnd, WM_CLOSE, 0, 0);
}
