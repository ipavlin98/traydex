use crate::profile::{self, Profile};
use windows::{
    Win32::{Foundation::*, System::Registry::*},
    core::HSTRING,
};

const RUN: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
pub const AUTOSTART_FLAG: &str = "--autostart";
pub const SEND_FLAG: &str = "--send";

fn menus(profile: &Profile) -> [String; 2] {
    let name = profile.registry_name();
    [format!("Software\\Classes\\*\\shell\\{name}"), format!("Software\\Classes\\Directory\\shell\\{name}")]
}

fn autostart_command(profile: &Profile, exe: &str) -> String {
    profile.command(exe, AUTOSTART_FLAG)
}

fn menu_command(profile: &Profile, exe: &str) -> String {
    profile.command(exe, &format!("{SEND_FLAG} \"%1\""))
}

fn exe() -> anyhow::Result<String> {
    Ok(std::env::current_exe()?.display().to_string())
}

fn read(key: &str, name: &str) -> Option<String> {
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

fn write(key: &str, name: &str, value: &str) -> anyhow::Result<bool> {
    if read(key, name).as_deref() == Some(value) {
        return Ok(false);
    }
    let data: Vec<u16> = value.encode_utf16().chain([0]).collect();
    unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            &HSTRING::from(key),
            &HSTRING::from(name),
            REG_SZ.0,
            Some(data.as_ptr() as _),
            (data.len() * 2) as u32,
        )
        .ok()?;
    }
    Ok(true)
}

fn removed(status: WIN32_ERROR) -> anyhow::Result<bool> {
    if status == ERROR_FILE_NOT_FOUND {
        return Ok(false);
    }
    status.ok()?;
    Ok(true)
}

pub fn set_autostart(enabled: bool) -> anyhow::Result<()> {
    let profile = profile::current();
    let name = profile.autostart_name();
    let exe = exe()?;
    let command = autostart_command(profile, &exe);
    if enabled {
        if write(RUN, name, &command)? {
            log::debug!("Autostart registered for {exe}");
        }
        return Ok(());
    }
    if read(RUN, name).is_some_and(|value| !value.eq_ignore_ascii_case(&command)) {
        return Ok(());
    }
    let status = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, &HSTRING::from(RUN), &HSTRING::from(name)) };
    if removed(status)? {
        log::debug!("Autostart removed");
    }
    Ok(())
}

fn delete_menus() -> anyhow::Result<bool> {
    let mut changed = false;
    for key in menus(profile::current()) {
        changed |= removed(unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, &HSTRING::from(key)) })?;
    }
    Ok(changed)
}

fn write_menus(label: &str, exe: &str) -> anyhow::Result<bool> {
    let profile = profile::current();
    let mut changed = false;
    for key in menus(profile) {
        changed |= write(&key, "MUIVerb", label)?;
        changed |= write(&key, "Icon", &format!("\"{exe}\",0"))?;
        changed |= write(&key, "MultiSelectModel", "Player")?;
        changed |= write(&format!("{key}\\command"), "", &menu_command(profile, exe))?;
    }
    Ok(changed)
}

pub fn set_context_menu(enabled: bool, label: &str) -> anyhow::Result<()> {
    let result = if enabled {
        exe().and_then(|exe| {
            let changed = write_menus(label, &exe);
            if let Ok(true) = changed {
                log::debug!("Context menu registered for {exe}");
            }
            changed
        })
    } else {
        delete_menus().inspect(|changed| {
            if *changed {
                log::debug!("Context menu removed");
            }
        })
    };
    if enabled
        && result.is_err()
        && let Err(error) = delete_menus()
    {
        log::warn!("Couldn't roll back a partial context menu: {error:#}");
    }
    result.map(|_| ())
}

#[cfg(test)]
mod tests;
