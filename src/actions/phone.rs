use super::upload::{ascii_name, mime};
use super::{Outcome, Show, base, encode, extension, textual};
use crate::i18n::tr;
use crate::store::human_size;
use crate::system::{Server, Services};
use anyhow::{Result, anyhow, bail};
use image::{GrayImage, ImageFormat, Luma};
use qrcodegen::{QrCode, QrCodeEcc};
use std::collections::hash_map::RandomState;
use std::fs::{self, File};
use std::hash::{BuildHasher, Hasher};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

const HEAD_LIMIT: usize = 8 * 1024;
const QR_FILE_LIMIT: u64 = 8 * 1024;
const QR_MODULE: u32 = 10;
const QR_QUIET: u32 = 4;
const IO_TIMEOUT: Duration = Duration::from_secs(30);
const WAKE_TIMEOUT: Duration = Duration::from_millis(500);
const ROUTE_PROBE: (Ipv4Addr, u16) = (Ipv4Addr::new(8, 8, 8, 8), 80);
const VIEWABLE: [&str; 5] = ["image/", "video/", "audio/", "text/plain", "application/pdf"];
const NOT_FOUND: &str = "404 Not Found";
const PAGE: &str = "<!doctype html><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>Traydex</title>\
<style>body{margin:0;padding:28px 24px;font:17px system-ui,sans-serif;background:#fff;color:#171717}\
div{margin:0 0 10px;padding:14px 16px;border:1px solid #d4d4d4;border-radius:10px}\
a{display:block;color:inherit;text-decoration:none;overflow-wrap:anywhere}\
a,p,button{-webkit-user-select:none;user-select:none}\
small,p{display:block;margin:4px 0 0;color:#5f5f5f}p{margin:0 0 16px;font-size:15px}\
img{display:block;width:100%;max-height:60vh;object-fit:contain;margin:0 0 10px;border-radius:6px;background:#fafafa;-webkit-touch-callout:default}\
button{display:block;width:100%;margin:0 0 12px;padding:13px 16px;border:1px solid #7b61ff;border-radius:10px;background:none;color:#6247e8;font:500 17px system-ui,sans-serif}\
@media(prefers-color-scheme:dark){body{background:#000;color:#ededed}div{border-color:#333}small,p{color:#a1a1a1}img{background:#0a0a0a}button{border-color:#b4a3ff;color:#c5b8ff}}</style>";
const DOWNLOAD_ALL: &str = "<script>document.querySelector('button').onclick=async()=>{for(const link of[...document.links]){\
const save=document.createElement('a');save.href=link.href+'?download';save.download='';document.body.append(save);save.click();save.remove();\
await new Promise(done=>setTimeout(done,700))}}</script>";

struct Running {
    stopped: Arc<AtomicBool>,
    address: SocketAddr,
}

impl Drop for Running {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Relaxed);
        let _ = TcpStream::connect_timeout(&self.address, WAKE_TIMEOUT);
    }
}

pub fn serve(files: Vec<PathBuf>) -> Result<Server> {
    let Some(host) = local_address() else {
        bail!(tr().no_network);
    };
    let listener = TcpListener::bind((Ipv4Addr::UNSPECIFIED, 0))?;
    let port = listener.local_addr()?.port();
    let shared = Arc::new(Shared::new(files.clone()));
    let url = format!("http://{host}:{port}{}", shared.root());
    let stopped = Arc::new(AtomicBool::new(false));
    let flag = stopped.clone();
    thread::Builder::new().name("traydex-share".into()).spawn(move || listen(listener, &shared, &flag))?;
    log::info!("Sharing {} file(s) on {host}:{port}", files.len());
    Ok(Server::new(url, files, Running { stopped, address: SocketAddr::from((Ipv4Addr::LOCALHOST, port)) }))
}

pub(super) fn share(files: &[PathBuf], system: &dyn Services) -> Result<Outcome> {
    let files: Vec<PathBuf> = files.iter().filter(|file| file.is_file()).cloned().collect();
    if files.is_empty() {
        bail!(tr().phone_needs_files);
    }
    Ok(Outcome::showing(Show::Share(system.serve(files)?)))
}

pub(super) fn qr(files: &[PathBuf]) -> Result<Outcome> {
    let mut parts = Vec::new();
    for file in files {
        let kind = extension(file);
        if !file.is_file() || !textual(file) {
            continue;
        }
        if fs::metadata(file)?.len() > QR_FILE_LIMIT {
            bail!(tr().qr_too_long);
        }
        let text = String::from_utf8_lossy(&fs::read(file)?).into_owned();
        let text = if kind == "url" {
            text.lines().find_map(|line| line.trim().strip_prefix("URL=")).unwrap_or_default().to_string()
        } else {
            text
        };
        parts.push(text.trim().to_string());
    }
    let text = parts.join("\n").trim().to_string();
    if text.is_empty() {
        bail!(tr().qr_needs_text);
    }
    QrCode::encode_text(&text, QrCodeEcc::Medium).map_err(|_| anyhow!(tr().qr_too_long))?;
    Ok(Outcome::showing(Show::Qr(text)))
}

pub fn qr_png(code: &QrCode) -> Result<Vec<u8>> {
    let side = (code.size() as u32 + QR_QUIET * 2) * QR_MODULE;
    let image = GrayImage::from_fn(side, side, |x, y| {
        let cell = |value: u32| (value / QR_MODULE) as i32 - QR_QUIET as i32;
        Luma([if code.get_module(cell(x), cell(y)) { 0 } else { 255 }])
    });
    let mut png = io::Cursor::new(Vec::new());
    image.write_to(&mut png, ImageFormat::Png)?;
    Ok(png.into_inner())
}

fn local_address() -> Option<Ipv4Addr> {
    crate::os::lan_addresses().into_iter().next().or_else(routed_address)
}

fn routed_address() -> Option<Ipv4Addr> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    socket.connect(ROUTE_PROBE).ok()?;
    match socket.local_addr().ok()?.ip() {
        IpAddr::V4(ip) if !ip.is_loopback() && !ip.is_unspecified() => Some(ip),
        _ => None,
    }
}

fn listen(listener: TcpListener, shared: &Arc<Shared>, stopped: &AtomicBool) {
    for stream in listener.incoming() {
        if stopped.load(Ordering::Relaxed) {
            break;
        }
        match stream {
            Ok(stream) => {
                let shared = shared.clone();
                let spawned =
                    thread::Builder::new().name("traydex-share-client".into()).spawn(move || answer(stream, &shared));
                if let Err(error) = spawned {
                    log::warn!("Couldn't serve a phone request: {error}");
                }
            }
            Err(error) => log::debug!("Phone connection failed: {error}"),
        }
    }
    log::info!("Sharing stopped");
}

fn answer(mut stream: TcpStream, shared: &Shared) {
    let _ = stream.set_read_timeout(Some(IO_TIMEOUT));
    let _ = stream.set_write_timeout(Some(IO_TIMEOUT));
    if let Err(error) = shared.respond(&mut stream) {
        log::debug!("Phone transfer interrupted: {error}");
    }
}

fn token() -> String {
    let state = RandomState::new();
    let word = |salt: u64| {
        let mut hasher = state.build_hasher();
        hasher.write_u64(salt);
        hasher.finish()
    };
    format!("{:016x}{:08x}", word(0), word(1) as u32)
}

#[derive(Debug, PartialEq)]
enum Span {
    Whole,
    Part(u64, u64),
    Unsatisfiable,
}

fn span(header: &str, size: u64) -> Span {
    let Some((first, last)) = header.trim().strip_prefix("bytes=").and_then(|spec| spec.split_once('-')) else {
        return Span::Whole;
    };
    if last.contains(',') {
        return Span::Whole;
    }
    let (first, last) = (first.trim(), last.trim());
    if first.is_empty() {
        return match last.parse::<u64>() {
            Ok(0) => Span::Unsatisfiable,
            Ok(_) if size == 0 => Span::Unsatisfiable,
            Ok(count) => Span::Part(size.saturating_sub(count), size),
            Err(_) => Span::Whole,
        };
    }
    let Ok(start) = first.parse::<u64>() else {
        return Span::Whole;
    };
    let end = if last.is_empty() {
        size
    } else {
        match last.parse::<u64>() {
            Ok(last) if last >= start => last.saturating_add(1).min(size),
            _ => return Span::Whole,
        }
    };
    if start >= size { Span::Unsatisfiable } else { Span::Part(start, end) }
}

fn read_head(stream: &mut impl Read) -> io::Result<Option<String>> {
    let mut head = Vec::new();
    let mut chunk = [0; 1024];
    while !head.windows(4).any(|window| window == b"\r\n\r\n") {
        if head.len() > HEAD_LIMIT {
            return Ok(None);
        }
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            return Ok(None);
        }
        head.extend_from_slice(&chunk[..read]);
    }
    Ok(String::from_utf8(head).ok())
}

fn reply(out: &mut impl Write, status: &str, headers: &[(&str, String)], length: u64) -> io::Result<()> {
    let mut text = format!(
        "HTTP/1.1 {status}\r\nContent-Length: {length}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n"
    );
    for (name, value) in headers {
        text.push_str(&format!("{name}: {value}\r\n"));
    }
    text.push_str("\r\n");
    out.write_all(text.as_bytes())
}

fn page(out: &mut impl Write, status: &str, content_type: &str, body: &str, head: bool) -> io::Result<()> {
    reply(out, status, &[("Content-Type", content_type.into())], body.len() as u64)?;
    if !head {
        out.write_all(body.as_bytes())?;
    }
    out.flush()
}

fn refuse(out: &mut impl Write, status: &str, head: bool) -> io::Result<()> {
    page(out, status, "text/plain; charset=utf-8", status, head)
}

fn send_file(out: &mut impl Write, file: &Path, range: Option<&str>, download: bool, head: bool) -> io::Result<()> {
    let Ok(mut source) = File::open(file) else {
        return refuse(out, NOT_FOUND, head);
    };
    let size = source.metadata()?.len();
    let kind = mime(file);
    let viewable = !download && VIEWABLE.iter().any(|viewable| kind.starts_with(viewable));
    let disposition = if viewable { "inline" } else { "attachment" };
    let mut headers = vec![
        ("Content-Type", kind.to_string()),
        (
            "Content-Disposition",
            format!("{disposition}; filename=\"{}\"; filename*=UTF-8''{}", ascii_name(file), encode(&base(file))),
        ),
        ("Accept-Ranges", "bytes".to_string()),
    ];
    let (start, end) = match range.map_or(Span::Whole, |range| span(range, size)) {
        Span::Whole => {
            reply(out, "200 OK", &headers, size)?;
            (0, size)
        }
        Span::Part(start, end) => {
            headers.push(("Content-Range", format!("bytes {start}-{}/{size}", end - 1)));
            reply(out, "206 Partial Content", &headers, end - start)?;
            (start, end)
        }
        Span::Unsatisfiable => {
            reply(out, "416 Range Not Satisfiable", &[("Content-Range", format!("bytes */{size}"))], 0)?;
            return out.flush();
        }
    };
    if !head {
        source.seek(SeekFrom::Start(start))?;
        io::copy(&mut source.take(end - start), out)?;
    }
    out.flush()
}

pub(super) struct Shared {
    token: String,
    files: Vec<PathBuf>,
}

impl Shared {
    pub(super) fn new(files: Vec<PathBuf>) -> Self {
        Self { token: token(), files }
    }

    pub(super) fn root(&self) -> String {
        format!("/{}", self.token)
    }

    fn index(&self, out: &mut impl Write, head: bool) -> io::Result<()> {
        let photo = |file: &PathBuf| mime(file).starts_with("image/");
        let links: String = self
            .files
            .iter()
            .enumerate()
            .map(|(index, file)| {
                let name = base(file);
                let href = format!("{}/{index}/{}", self.root(), encode(&name));
                let preview = if photo(file) { format!("<img src=\"{href}\" alt=\"\">") } else { String::new() };
                let size = fs::metadata(file).map(|meta| human_size(meta.len())).unwrap_or_default();
                format!(
                    "<div>{preview}<a href=\"{href}\">{}<small>{size}</small></a></div>",
                    name.replace('&', "&amp;")
                )
            })
            .collect();
        let hint =
            if self.files.iter().any(photo) { format!("<p>{}</p>", tr().phone_photo_hint) } else { String::new() };
        let body = format!("{PAGE}<button>{}</button>{hint}{links}{DOWNLOAD_ALL}", tr().phone_download_all);
        page(out, "200 OK", "text/html; charset=utf-8", &body, head)
    }

    pub(super) fn respond(&self, stream: &mut (impl Read + Write)) -> io::Result<()> {
        let Some(request) = read_head(stream)? else {
            return refuse(stream, "400 Bad Request", false);
        };
        let mut lines = request.split("\r\n");
        let mut words = lines.next().unwrap_or_default().split(' ');
        let (method, target) = (words.next().unwrap_or_default(), words.next().unwrap_or_default());
        let head = method == "HEAD";
        if method != "GET" && !head {
            return refuse(stream, "405 Method Not Allowed", head);
        }
        let range = lines.find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.trim().eq_ignore_ascii_case("range").then_some(value)
        });
        let target = target.split('#').next().unwrap_or_default();
        let (path, query) = target.split_once('?').unwrap_or((target, ""));
        let download = query.split('&').any(|pair| pair == "download");
        let rest = match path.strip_prefix(&self.root()) {
            Some(rest) if rest.is_empty() || rest.starts_with('/') => rest.trim_start_matches('/'),
            _ => return refuse(stream, NOT_FOUND, head),
        };
        if rest.is_empty() {
            return self.index(stream, head);
        }
        let chosen = rest
            .split('/')
            .next()
            .and_then(|index| index.parse::<usize>().ok())
            .and_then(|index| self.files.get(index));
        match chosen {
            Some(file) => send_file(stream, file, range, download, head),
            None => refuse(stream, NOT_FOUND, head),
        }
    }
}

#[cfg(test)]
mod tests;
