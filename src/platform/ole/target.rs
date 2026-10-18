use super::{MAX_BYTES, ansi_string, bytes, format, global_bytes, png_format, registered, wide_string};
use super::{exporting, rejecting};
use crate::guard;
use crate::i18n::tr;
use crate::platform::window::client_point;
use crate::platform::{Event, Payload, send};
use crate::store::{self, ImageData, Scratch, Storage};
use crate::system::{Offer, Offered};
use anyhow::bail;
use std::cell::Cell;
use std::path::{Path, PathBuf};
use windows::Win32::Foundation::*;
use windows::Win32::System::Com::*;
use windows::Win32::System::Ole::*;
use windows::Win32::System::SystemServices::MODIFIERKEYS_FLAGS;
use windows::Win32::UI::Shell::*;
use windows::core::{Ref, implement, w};

const DESCRIPTOR_SIZE: usize = 592;
const FD_ATTRIBUTES: u32 = 0x4;
const FD_FILESIZE: u32 = 0x40;
const DIRECTORY: u32 = 0x10;

fn known_formats() -> [(u16, &'static str); 8] {
    [
        (CF_HDROP.0, "files"),
        (registered(w!("FileGroupDescriptorW")), "virtual files"),
        (png_format(), "PNG"),
        (CF_DIBV5.0, "DIBV5"),
        (CF_DIB.0, "DIB"),
        (registered(w!("UniformResourceLocatorW")), "URL"),
        (CF_UNICODETEXT.0, "Unicode text"),
        (CF_TEXT.0, "ANSI text"),
    ]
}

fn offered(data: &IDataObject) -> Vec<&'static str> {
    known_formats()
        .into_iter()
        .filter(|(cf, _)| unsafe { data.QueryGetData(&format(*cf, TYMED_HGLOBAL.0 as u32)) } == S_OK)
        .map(|(_, name)| name)
        .collect()
}

fn dropped_paths(data: &IDataObject) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    unsafe {
        let Ok(mut medium) = data.GetData(&format(CF_HDROP.0, TYMED_HGLOBAL.0 as u32)) else {
            return paths;
        };
        if medium.tymed != TYMED_HGLOBAL.0 as u32 {
            ReleaseStgMedium(&mut medium);
            return paths;
        }
        let drop = HDROP(medium.u.hGlobal.0);
        let count = DragQueryFileW(drop, u32::MAX, None);
        for index in 0..count.min(4096) {
            let length = DragQueryFileW(drop, index, None) as usize;
            let mut buffer = vec![0u16; length + 1];
            DragQueryFileW(drop, index, Some(&mut buffer));
            paths.push(PathBuf::from(String::from_utf16_lossy(&buffer[..length])));
        }
        ReleaseStgMedium(&mut medium);
    }
    paths
}

fn offer(data: &IDataObject, formats: &[&str]) -> Offer {
    let paths = dropped_paths(data);
    if !paths.is_empty() {
        return Offer::Files(paths.into_iter().map(|path| Offered { folder: path.is_dir(), path }).collect());
    }
    let has = |names: &[&str]| formats.iter().any(|format| names.contains(format));
    if has(&["virtual files"]) {
        Offer::Unknown
    } else if has(&["PNG", "DIBV5", "DIB"]) {
        Offer::Image
    } else if has(&["URL", "Unicode text", "ANSI text"]) {
        Offer::Text
    } else {
        Offer::Unknown
    }
}

fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    bytes.get(offset..offset + 4).and_then(|word| word.try_into().ok()).map_or(0, u32::from_le_bytes)
}

fn read_stream(stream: &IStream) -> anyhow::Result<Vec<u8>> {
    let mut content = Vec::new();
    let mut buffer = vec![0u8; 1 << 16];
    loop {
        let mut read = 0u32;
        let status = unsafe { stream.Read(buffer.as_mut_ptr() as _, buffer.len() as u32, Some(&mut read)) };
        status.ok()?;
        if read == 0 {
            break;
        }
        content.extend_from_slice(&buffer[..(read as usize).min(buffer.len())]);
        if content.len() > MAX_BYTES {
            bail!(tr().too_large);
        }
    }
    Ok(content)
}

fn virtual_contents(data: &IDataObject, index: usize) -> Option<Vec<u8>> {
    let mut fmt = format(registered(w!("FileContents")), (TYMED_ISTREAM.0 | TYMED_HGLOBAL.0) as u32);
    fmt.lindex = index as i32;
    unsafe {
        let mut medium = data.GetData(&fmt).ok()?;
        let content = if medium.tymed == TYMED_ISTREAM.0 as u32 {
            medium.u.pstm.as_ref().and_then(|stream| read_stream(stream).ok())
        } else if medium.tymed == TYMED_HGLOBAL.0 as u32 {
            global_bytes(medium.u.hGlobal)
        } else {
            None
        };
        ReleaseStgMedium(&mut medium);
        content
    }
}

fn virtual_files(data: &IDataObject, storage: &Storage) -> Scratch {
    let mut paths = Scratch::new(storage.clone());
    let Some(descriptors) = bytes(data, registered(w!("FileGroupDescriptorW"))) else {
        return paths;
    };
    if descriptors.len() < 4 {
        return paths;
    }
    let count = u32_at(&descriptors, 0) as usize;
    for index in 0..count.min(256) {
        let base = 4 + index * DESCRIPTOR_SIZE;
        if base + DESCRIPTOR_SIZE > descriptors.len() {
            break;
        }
        let flags = u32_at(&descriptors, base);
        let attributes = u32_at(&descriptors, base + 36);
        if flags & FD_ATTRIBUTES != 0 && attributes & DIRECTORY != 0 {
            continue;
        }
        let name = wide_string(&descriptors[base + 72..base + DESCRIPTOR_SIZE]);
        let name = Path::new(&name).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if name.is_empty() || name.to_lowercase().ends_with(".url") {
            continue;
        }
        let Some(mut content) = virtual_contents(data, index) else {
            log::warn!("Couldn't read the contents of the virtual file {name}");
            continue;
        };
        if flags & FD_FILESIZE != 0 {
            let size = (u32_at(&descriptors, base + 64) as u64) << 32 | u32_at(&descriptors, base + 68) as u64;
            content.truncate(size as usize);
        }
        match store::save_virtual(storage, &name, &content) {
            Ok(path) => paths.push(path),
            Err(error) => log::warn!("Couldn't save the virtual file {name}: {error:#}"),
        }
    }
    paths
}

pub fn read_data(data: &IDataObject, storage: &Storage) -> anyhow::Result<Payload> {
    let paths = dropped_paths(data);
    if !paths.is_empty() {
        return Ok(Payload::Paths(paths));
    }
    let stored = virtual_files(data, storage);
    if !stored.is_empty() {
        return Ok(Payload::Stored(stored));
    }
    if let Some(png) = bytes(data, png_format()) {
        return Ok(Payload::Image(ImageData::Png(png)));
    }
    for cf in [CF_DIBV5.0, CF_DIB.0] {
        if let Some(dib) = bytes(data, cf) {
            return Ok(Payload::Image(ImageData::Dib(dib)));
        }
    }
    let texts = [
        (registered(w!("UniformResourceLocatorW")), wide_string as fn(&[u8]) -> String),
        (CF_UNICODETEXT.0, wide_string),
        (CF_TEXT.0, ansi_string),
    ];
    for (cf, decode) in texts {
        if let Some(text) = bytes(data, cf).map(|b| decode(&b))
            && !text.trim().is_empty()
        {
            return Ok(Payload::Text(text));
        }
    }
    bail!(tr().unsupported)
}

#[implement(IDropTarget)]
pub(super) struct Target {
    pub(super) hwnd: HWND,
    pub(super) helper: Option<IDropTargetHelper>,
    pub(super) accepted: Cell<bool>,
    pub(super) pointer: Cell<Option<(i32, i32)>>,
    pub(super) storage: Storage,
}

impl Target {
    fn track(&self, pt: &POINTL) {
        let point = Some((pt.x, pt.y));
        if self.pointer.replace(point) != point {
            let (x, y) = client_point(pt.x, pt.y);
            send(Event::Pointer(x, y));
        }
    }
}

fn choose(accepted: bool, allowed: DROPEFFECT) -> DROPEFFECT {
    if !accepted || rejecting() {
        DROPEFFECT_NONE
    } else if allowed.0 & DROPEFFECT_COPY.0 != 0 {
        DROPEFFECT_COPY
    } else if allowed.0 & DROPEFFECT_LINK.0 != 0 {
        DROPEFFECT_LINK
    } else {
        DROPEFFECT_NONE
    }
}

fn point(pt: &POINTL) -> POINT {
    POINT { x: pt.x, y: pt.y }
}

fn refuse(effect: *mut DROPEFFECT) -> windows::core::Result<()> {
    if let Some(effect) = unsafe { effect.as_mut() } {
        *effect = DROPEFFECT_NONE;
    }
    Err(E_UNEXPECTED.into())
}

fn set_effect(effect: *mut DROPEFFECT, chosen: DROPEFFECT) {
    if let Some(effect) = unsafe { effect.as_mut() } {
        *effect = chosen;
    }
}

fn allowed(effect: *mut DROPEFFECT) -> DROPEFFECT {
    unsafe { effect.as_ref() }.copied().unwrap_or(DROPEFFECT_NONE)
}

fn abandon(effect: *mut DROPEFFECT) -> windows::core::Result<()> {
    send(Event::Hover(false));
    refuse(effect)
}

impl IDropTarget_Impl for Target_Impl {
    fn DragEnter(
        &self,
        object: Ref<IDataObject>,
        _: MODIFIERKEYS_FLAGS,
        pt: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> windows::core::Result<()> {
        guard::or_else(
            || abandon(effect),
            || {
                let exporting = exporting();
                let formats = object.as_ref().map(offered).unwrap_or_default();
                let accepted = !formats.is_empty();
                if exporting {
                    log::debug!("Own drag entered the window, only tiles take it");
                } else {
                    log::debug!("Drag entered the window: formats [{}], accepted: {accepted}", formats.join(", "));
                }
                self.accepted.set(accepted);
                let chosen = choose(accepted, allowed(effect));
                set_effect(effect, chosen);
                if let (Some(helper), Some(object)) = (&self.helper, object.as_ref()) {
                    let _ = unsafe { helper.DragEnter(self.hwnd, object, &point(pt), chosen) };
                }
                self.track(pt);
                if let (true, Some(object)) = (accepted, object.as_ref()) {
                    send(Event::Offer(offer(object, &formats)));
                }
                if !exporting {
                    send(Event::Hover(accepted));
                }
                Ok(())
            },
        )
    }

    fn DragOver(&self, _: MODIFIERKEYS_FLAGS, pt: &POINTL, effect: *mut DROPEFFECT) -> windows::core::Result<()> {
        guard::or_else(
            || refuse(effect),
            || {
                let chosen = choose(self.accepted.get(), allowed(effect));
                set_effect(effect, chosen);
                if let Some(helper) = &self.helper {
                    let _ = unsafe { helper.DragOver(&point(pt), chosen) };
                }
                self.track(pt);
                Ok(())
            },
        )
    }

    fn DragLeave(&self) -> windows::core::Result<()> {
        guard::or_else(
            || abandon(std::ptr::null_mut()),
            || {
                log::debug!("Drag left the window");
                if let Some(helper) = &self.helper {
                    let _ = unsafe { helper.DragLeave() };
                }
                self.pointer.set(None);
                send(Event::Hover(false));
                Ok(())
            },
        )
    }

    fn Drop(
        &self,
        object: Ref<IDataObject>,
        _: MODIFIERKEYS_FLAGS,
        pt: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> windows::core::Result<()> {
        guard::or_else(
            || {
                send(Event::Failure(tr().internal_error.into()));
                abandon(effect)
            },
            || {
                let chosen = choose(self.accepted.get(), allowed(effect));
                set_effect(effect, DROPEFFECT_NONE);
                if let (Some(helper), Some(object)) = (&self.helper, object.as_ref()) {
                    let _ = unsafe { helper.Drop(object, &point(pt), chosen) };
                }
                self.track(pt);
                self.pointer.set(None);
                match object.as_ref() {
                    Some(_) if chosen == DROPEFFECT_NONE => {
                        log::debug!("Drop refused: nothing to accept here")
                    }
                    Some(_) if exporting() => {
                        log::debug!("Own drag dropped back onto the window");
                        send(Event::DropBack);
                        set_effect(effect, chosen);
                    }
                    Some(object) => match read_data(object, &self.storage) {
                        Ok(payload) => {
                            log::debug!("Dropped onto the window: {}", payload.describe());
                            send(Event::Drop(payload));
                            set_effect(effect, chosen);
                        }
                        Err(error) => {
                            log::warn!("Couldn't read the dropped data: {error:#}");
                            send(Event::Failure(error.to_string()));
                        }
                    },
                    None => log::warn!("Drop arrived without data"),
                }
                send(Event::Hover(false));
                Ok(())
            },
        )
    }
}

#[cfg(test)]
mod tests;
