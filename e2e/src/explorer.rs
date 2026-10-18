use crate::{input, wait, win};
use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;
use std::time::Duration;
use windows::Win32::{
    Foundation::{HWND, POINT, RECT},
    System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance},
    UI::Accessibility::*,
};

pub const CLASS: &str = "CabinetWClass";

pub struct Explorer {
    pub hwnd: HWND,
}

fn matches(hwnd: HWND, folder: &Path) -> bool {
    let title = win::title(hwnd);
    let name = folder.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    win::class(hwnd) == CLASS && (title == name || Path::new(&title) == folder)
}

pub fn windows_for(folder: &Path) -> Vec<HWND> {
    win::windows().into_iter().filter(|hwnd| matches(*hwnd, folder)).collect()
}

pub fn open(folder: &Path, rect: RECT) -> Result<Explorer> {
    let before = windows_for(folder);
    Command::new("explorer.exe").arg(folder).spawn().context("couldn't start Explorer")?;
    let hwnd = wait("Explorer window with the test folder", Duration::from_secs(15), || {
        windows_for(folder).into_iter().find(|hwnd| !before.contains(hwnd))
    })?;
    input::allow_window(hwnd);
    win::place(hwnd, rect);
    crate::pause(700)?;
    Ok(Explorer { hwnd })
}

fn automation() -> Result<IUIAutomation> {
    Ok(unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)? })
}

fn elements(hwnd: HWND) -> Result<Vec<IUIAutomationElement>> {
    let automation = automation()?;
    unsafe {
        let root = automation.ElementFromHandle(hwnd)?;
        let all = root.FindAll(TreeScope_Descendants, &automation.CreateTrueCondition()?)?;
        Ok((0..all.Length()?).filter_map(|index| all.GetElement(index).ok()).collect())
    }
}

impl Explorer {
    pub fn item(&self, name: &str) -> Result<RECT> {
        let stem = Path::new(name).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        wait(&format!("item \"{name}\" in Explorer"), Duration::from_secs(10), || {
            elements(self.hwnd).ok()?.into_iter().find_map(|element| unsafe {
                let kind = element.CurrentControlType().ok()?;
                let label = element.CurrentName().ok()?.to_string();
                (kind == UIA_ListItemControlTypeId && (label == name || label == stem))
                    .then(|| element.CurrentBoundingRectangle().ok())
                    .flatten()
            })
        })
    }

    pub fn empty_point(&self) -> Result<POINT> {
        let view = wait("file area in Explorer", Duration::from_secs(10), || {
            elements(self.hwnd).ok()?.into_iter().find_map(|element| unsafe {
                (element.CurrentClassName().ok()? == "UIItemsView")
                    .then(|| element.CurrentBoundingRectangle().ok())
                    .flatten()
            })
        })?;
        Ok(POINT { x: view.right - (view.right - view.left) / 4, y: view.bottom - (view.bottom - view.top) / 4 })
    }

    pub fn center(rect: RECT) -> POINT {
        POINT { x: rect.left + (rect.right - rect.left).min(120) / 2, y: (rect.top + rect.bottom) / 2 }
    }
}

impl Drop for Explorer {
    fn drop(&mut self) {
        win::close(self.hwnd);
        input::forget_window(self.hwnd);
        for _ in 0..60 {
            if !win::exists(self.hwnd) {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}
