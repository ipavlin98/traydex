use super::window::panel;
use crate::{
    i18n::{fill, tr},
    store::{Item, Kind},
};
use anyhow::bail;
use std::{
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
};
use windows::{
    Win32::{
        System::Com::*,
        UI::{
            Shell::{Common::COMDLG_FILTERSPEC, *},
            WindowsAndMessaging::SW_SHOWNORMAL,
        },
    },
    core::{HSTRING, PCWSTR, PWSTR, w},
};

pub fn open(item: &Item) {
    let target = if item.kind == Kind::Link {
        item.text.as_deref().unwrap_or_default().trim().to_owned()
    } else {
        item.path.to_string_lossy().into_owned()
    };
    shell_open(&target);
}

pub fn reveal(item: &Item) {
    if item.kind == Kind::Link {
        return open(item);
    }
    unsafe {
        let list = ILCreateFromPathW(&HSTRING::from(item.path.as_path()));
        if list.is_null() {
            return;
        }
        let _ = SHOpenFolderAndSelectItems(list, None, 0);
        ILFree(Some(list));
    }
}

pub fn shell_open(target: &str) {
    unsafe {
        ShellExecuteW(Some(panel()), w!("open"), &HSTRING::from(target), None, None, SW_SHOWNORMAL);
    }
}

unsafe fn take(text: PWSTR) -> Option<String> {
    let value = unsafe { text.to_string() }.ok();
    unsafe { CoTaskMemFree(Some(text.0 as _)) };
    value
}

pub fn pick(title: &str, folders: bool) -> anyhow::Result<Option<PathBuf>> {
    unsafe {
        let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)?;
        let mut options = FOS_FORCEFILESYSTEM | FOS_PATHMUSTEXIST | FOS_NOCHANGEDIR;
        if folders {
            options |= FOS_PICKFOLDERS;
        } else {
            let name = HSTRING::from(tr().programs_filter);
            dialog.SetFileTypes(&[COMDLG_FILTERSPEC {
                pszName: PCWSTR(name.as_ptr()),
                pszSpec: w!("*.exe;*.bat;*.cmd;*.ps1"),
            }])?;
        }
        dialog.SetOptions(options)?;
        dialog.SetTitle(&HSTRING::from(title))?;
        if dialog.Show(Some(panel())).is_err() {
            return Ok(None);
        }
        let name = dialog.GetResult()?.GetDisplayName(SIGDN_FILESYSPATH)?;
        Ok(take(name).map(PathBuf::from))
    }
}

pub fn pick_files(title: &str) -> anyhow::Result<Vec<PathBuf>> {
    unsafe {
        let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)?;
        dialog.SetOptions(
            FOS_FORCEFILESYSTEM | FOS_PATHMUSTEXIST | FOS_FILEMUSTEXIST | FOS_NOCHANGEDIR | FOS_ALLOWMULTISELECT,
        )?;
        dialog.SetTitle(&HSTRING::from(title))?;
        if dialog.Show(Some(panel())).is_err() {
            return Ok(Vec::new());
        }
        let results = dialog.GetResults()?;
        let mut paths = Vec::new();
        for index in 0..results.GetCount()? {
            let name = results.GetItemAt(index)?.GetDisplayName(SIGDN_FILESYSPATH)?;
            paths.extend(take(name).map(PathBuf::from));
        }
        Ok(paths)
    }
}

pub fn display_name(path: &Path) -> String {
    let shown = unsafe {
        SHCreateItemFromParsingName::<_, _, IShellItem>(&HSTRING::from(path), None::<&IBindCtx>)
            .and_then(|item| item.GetDisplayName(SIGDN_NORMALDISPLAY))
    };
    shown
        .ok()
        .and_then(|name| unsafe { take(name) })
        .unwrap_or_else(|| path.file_name().unwrap_or(path.as_os_str()).to_string_lossy().into_owned())
}

pub fn desktop() -> Option<PathBuf> {
    let path = unsafe { SHGetKnownFolderPath(&FOLDERID_Desktop, KF_FLAG_DEFAULT, None) };
    path.ok().and_then(|p| unsafe { take(p) }).map(PathBuf::from)
}

const SAME_FILE: i32 = 0x71;
const CANCELLED: i32 = 0x75;
const DESTINATION_SUBTREE: i32 = 0x76;
const DESTINATION_SAME_TREE: i32 = 0x7D;
const ERROR_CANCELLED: i32 = 1223;

pub fn transfer(files: &[PathBuf], folder: &Path, copy: bool) -> anyhow::Result<()> {
    let from: Vec<u16> = files.iter().flat_map(|f| f.as_os_str().encode_wide().chain([0])).chain([0]).collect();
    let to: Vec<u16> = folder.as_os_str().encode_wide().chain([0, 0]).collect();
    let mut operation = SHFILEOPSTRUCTW {
        wFunc: if copy { FO_COPY } else { FO_MOVE },
        pFrom: PCWSTR(from.as_ptr()),
        pTo: PCWSTR(to.as_ptr()),
        fFlags: (FOF_RENAMEONCOLLISION | FOF_NOCONFIRMMKDIR).0 as u16,
        ..Default::default()
    };
    let code = unsafe { SHFileOperationW(&mut operation) };
    if operation.fAnyOperationsAborted.as_bool() {
        bail!(tr().operation_cancelled);
    }
    match code {
        0 => {}
        SAME_FILE => bail!(tr().already_there),
        DESTINATION_SUBTREE | DESTINATION_SAME_TREE => bail!(tr().inside_itself),
        CANCELLED | ERROR_CANCELLED => bail!(tr().operation_cancelled),
        code => bail!(fill(tr().file_operation_failed, format!("0x{code:X}"))),
    }
    Ok(())
}
