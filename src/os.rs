use std::fs::Metadata;
use std::net::Ipv4Addr;
use std::os::windows::fs::MetadataExt;
use windows::Win32::Foundation::{ERROR_BUFFER_OVERFLOW, ERROR_SUCCESS, FILETIME};
use windows::Win32::Globalization::{
    CP_ACP, CP_OEMCP, GetUserDefaultUILanguage, MULTI_BYTE_TO_WIDE_CHAR_FLAGS, MultiByteToWideChar, WideCharToMultiByte,
};
use windows::Win32::NetworkManagement::IpHelper::{
    GAA_FLAG_INCLUDE_GATEWAYS, GAA_FLAG_SKIP_ANYCAST, GAA_FLAG_SKIP_DNS_SERVER, GAA_FLAG_SKIP_MULTICAST,
    GetAdaptersAddresses, IF_TYPE_ETHERNET_CSMACD, IF_TYPE_IEEE80211, IP_ADAPTER_ADDRESSES_LH,
};
use windows::Win32::NetworkManagement::Ndis::IfOperStatusUp;
use windows::Win32::Networking::WinSock::{AF_INET, SOCKADDR_IN};
use windows::Win32::Storage::FileSystem::FileTimeToLocalFileTime;
use windows::Win32::System::WindowsProgramming::FileTimeToDosDateTime;
use windows::Win32::UI::Input::KeyboardAndMouse::{MAPVK_VK_TO_CHAR, MapVirtualKeyW};

pub fn layout_char(key: u16) -> Option<char> {
    let mapped = unsafe { MapVirtualKeyW(key as u32, MAPVK_VK_TO_CHAR) } & 0xFFFF;
    char::from_u32(mapped).filter(|c| *c != '\0' && !c.is_control())
}

pub fn ui_language() -> u16 {
    unsafe { GetUserDefaultUILanguage() }
}

pub fn ansi_string(bytes: &[u8]) -> String {
    decode(CP_ACP, bytes)
}

pub fn oem_string(bytes: &[u8]) -> String {
    decode(CP_OEMCP, bytes)
}

fn decode(page: u32, bytes: &[u8]) -> String {
    let bytes = &bytes[..bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len())];
    if bytes.is_empty() {
        return String::new();
    }
    let flags = MULTI_BYTE_TO_WIDE_CHAR_FLAGS(0);
    let length = unsafe { MultiByteToWideChar(page, flags, bytes, None) };
    let mut words = vec![0u16; length.max(0) as usize];
    let written = unsafe { MultiByteToWideChar(page, flags, bytes, Some(&mut words)) };
    String::from_utf16_lossy(&words[..(written.max(0) as usize).min(words.len())])
}

pub fn ansi_bytes(text: &str) -> Vec<u8> {
    let words: Vec<u16> = text.encode_utf16().collect();
    if words.is_empty() {
        return Vec::new();
    }
    let length = unsafe { WideCharToMultiByte(CP_ACP, 0, &words, None, None, None) };
    let mut bytes = vec![0u8; length.max(0) as usize];
    let written = unsafe { WideCharToMultiByte(CP_ACP, 0, &words, Some(&mut bytes), None, None) };
    bytes.truncate(written.max(0) as usize);
    bytes
}

pub fn local_dos_time(metadata: &Metadata) -> Option<(u16, u16)> {
    let raw = metadata.last_write_time();
    let utc = FILETIME { dwLowDateTime: raw as u32, dwHighDateTime: (raw >> 32) as u32 };
    let mut local = FILETIME::default();
    let (mut date, mut time) = (0, 0);
    let converted = unsafe {
        FileTimeToLocalFileTime(&utc, &mut local).is_ok() && FileTimeToDosDateTime(&local, &mut date, &mut time).is_ok()
    };
    converted.then_some((date, time))
}

const ADAPTERS_BUFFER: u32 = 16 * 1024;
const ADAPTERS_ATTEMPTS: usize = 3;

pub fn lan_addresses() -> Vec<Ipv4Addr> {
    let flags = GAA_FLAG_INCLUDE_GATEWAYS | GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST | GAA_FLAG_SKIP_DNS_SERVER;
    let mut size = ADAPTERS_BUFFER;
    for _ in 0..ADAPTERS_ATTEMPTS {
        let mut buffer = vec![0u64; (size as usize).div_ceil(8)];
        let first = buffer.as_mut_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>();
        let code = unsafe { GetAdaptersAddresses(AF_INET.0 as u32, flags, None, Some(first), &mut size) };
        if code == ERROR_SUCCESS.0 {
            return unsafe { lan_from(first) };
        }
        if code != ERROR_BUFFER_OVERFLOW.0 {
            log::warn!("Couldn't list network adapters: error {code}");
            break;
        }
    }
    Vec::new()
}

unsafe fn lan_from(mut adapter: *const IP_ADAPTER_ADDRESSES_LH) -> Vec<Ipv4Addr> {
    let mut found = Vec::new();
    while let Some(current) = unsafe { adapter.as_ref() } {
        adapter = current.Next;
        let physical = matches!(current.IfType, IF_TYPE_ETHERNET_CSMACD | IF_TYPE_IEEE80211);
        if !physical || current.OperStatus != IfOperStatusUp || current.FirstGatewayAddress.is_null() {
            continue;
        }
        let mut unicast = current.FirstUnicastAddress;
        while let Some(address) = unsafe { unicast.as_ref() } {
            unicast = address.Next;
            let Some(socket) = (unsafe { address.Address.lpSockaddr.cast::<SOCKADDR_IN>().as_ref() }) else {
                continue;
            };
            if socket.sin_family != AF_INET {
                continue;
            }
            let bytes = unsafe { socket.sin_addr.S_un.S_un_b };
            let ip = Ipv4Addr::new(bytes.s_b1, bytes.s_b2, bytes.s_b3, bytes.s_b4);
            if ip.is_private() {
                found.push((current.Ipv4Metric, ip));
            }
        }
    }
    found.sort_by_key(|(metric, _)| *metric);
    found.into_iter().map(|(_, ip)| ip).collect()
}
