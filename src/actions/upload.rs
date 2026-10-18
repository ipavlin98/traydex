use super::{CONNECT_TIMEOUT, Outcome, Progress, Upload, base, encode, extension, last_line};
use crate::i18n::{fill, tr};
use crate::store;
use crate::system::Services;
use anyhow::{Context, Result, anyhow, bail};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const ERROR_PREVIEW: usize = 160;
const UPLOAD_LIMIT: u64 = 1024 * 1024 * 1024;
const CURL_RESOLVE: i32 = 6;
const CURL_CONNECT: i32 = 7;
const CURL_TIMEOUT: i32 = 28;
const GOOGLE_LENS: &str = "https://lens.google.com/v3/upload";
const YANDEX_IMAGES: &str = "https://yandex.ru/images/search?rpt=imageview";
const YANDEX_QUERY: &str =
    "&format=json&request=%7B%22blocks%22%3A%5B%7B%22block%22%3A%22b-page_type_search-by-image__link%22%7D%5D%7D";
const BING_IMAGES: &str = "https://www.bing.com/images/search?view=detailv2&iss=sbiupload&FORM=SBIHMP";
const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub(super) struct Accepts {
    pub(super) kind: fn(&Path) -> bool,
    pub(super) limit: u64,
    pub(super) wrong: &'static str,
}

impl Accepts {
    pub(super) fn files() -> Self {
        Self { kind: |path| path.is_file(), limit: UPLOAD_LIMIT, wrong: tr().needs_files }
    }
}

pub(super) fn publish(
    files: &[PathBuf],
    progress: &Progress,
    accepts: Accepts,
    send: impl Fn(&Path) -> Result<String>,
) -> Result<Outcome> {
    let suitable: Vec<&PathBuf> = files.iter().filter(|file| file.is_file() && (accepts.kind)(file)).collect();
    if suitable.is_empty() {
        bail!(accepts.wrong);
    }
    let mut links = Vec::new();
    let mut skipped = files.len() - suitable.len();
    let mut failure = None;
    for (done, file) in suitable.iter().enumerate() {
        if !progress.step(done, suitable.len()) {
            break;
        }
        let size = fs::metadata(file).map(|m| m.len()).unwrap_or(0);
        let sent = if size > accepts.limit { Err(anyhow!(fill(tr().too_big, base(file)))) } else { send(file) };
        match sent {
            Ok(link) => links.push(link),
            Err(error) => {
                log::warn!("Upload of {} failed: {error:#}", file.display());
                skipped += 1;
                failure.get_or_insert(error);
            }
        }
    }
    if links.is_empty() {
        return Err(failure.unwrap_or_else(|| anyhow!(accepts.wrong)));
    }
    let mut message = tr().link_copied.to_string();
    if skipped > 0 {
        message = format!("{message} · {}", fill(tr().skipped, skipped));
    }
    Ok(Outcome { message, created: Vec::new(), clipboard: Some(links.join("\r\n")), show: None })
}

pub fn curl_failure(code: Option<i32>, stderr: &[u8]) -> String {
    match code {
        Some(CURL_RESOLVE | CURL_CONNECT) => tr().no_connection.into(),
        Some(CURL_TIMEOUT) => tr().timed_out.into(),
        _ => last_line(stderr),
    }
}

pub fn curl_exe() -> PathBuf {
    let system = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
    Path::new(&system).join("System32").join("curl.exe")
}

pub(super) fn curl() -> Command {
    let mut command = Command::new(curl_exe());
    command.args(["-sS", "--connect-timeout", CONNECT_TIMEOUT, "-w", "\n%{http_code}"]);
    command
}

pub(super) fn exchange(
    command: &mut Command,
    body: Vec<u8>,
    progress: &Progress,
    system: &dyn Services,
) -> Result<(String, String)> {
    let output = system.output(command, Some(body), progress)?;
    if !output.status.success() {
        bail!(curl_failure(output.status.code(), &output.stderr));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let (body, status) = text.rsplit_once('\n').unwrap_or_default();
    Ok((body.to_string(), status.trim().to_string()))
}

fn check(status: &str, body: &str) -> Result<()> {
    match status {
        status if status.starts_with('2') => Ok(()),
        "401" | "403" => bail!(tr().key_rejected),
        status => {
            let preview: String = body.chars().take(ERROR_PREVIEW).collect();
            bail!("HTTP {status}: {preview}");
        }
    }
}

fn request(command: &mut Command, body: Vec<u8>, progress: &Progress, system: &dyn Services) -> Result<String> {
    let (body, status) = exchange(command, body, progress, system)?;
    check(&status, &body)?;
    Ok(body)
}

pub(super) fn base64(data: &[u8]) -> String {
    let mut text = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let bits = chunk.iter().enumerate().fold(0u32, |bits, (i, &byte)| bits | u32::from(byte) << (16 - 8 * i));
        for i in 0..4 {
            text.push(if i <= chunk.len() { char::from(BASE64[(bits >> (18 - 6 * i) & 63) as usize]) } else { '=' });
        }
    }
    text
}

fn lens_page(file: &Path, image: &[u8], system: &dyn Services) -> Result<String> {
    let page = system.temp_dir().join("traydex-lens.html");
    let html = format!(
        r#"<!doctype html>
<meta charset="utf-8">
<title>Google Lens</title>
<form method="post" enctype="multipart/form-data" action="{GOOGLE_LENS}"><input type="file" name="encoded_image"></form>
<script>
const bytes = Uint8Array.from(atob("{}"), c => c.charCodeAt(0));
const files = new DataTransfer();
files.items.add(new File([bytes], "{}", {{ type: "{}" }}));
const form = document.forms[0];
form.elements[0].files = files.files;
form.submit();
</script>
"#,
        base64(image),
        ascii_name(file),
        mime(file),
    );
    fs::write(&page, html)?;
    Ok(page.to_string_lossy().into_owned())
}

pub(super) fn find_image(engine: &str, file: &Path, progress: &Progress, system: &dyn Services) -> Result<String> {
    let host = store::host(engine);
    let image = fs::read(file)?;
    let (field, body, url) = if host.contains("yandex") {
        let field = format!("upfile=@-;filename={}", ascii_name(file));
        (field, image, format!("{YANDEX_IMAGES}{YANDEX_QUERY}"))
    } else if host.contains("bing.") {
        ("imageBin=<-".to_string(), base64(&image).into_bytes(), BING_IMAGES.to_string())
    } else {
        return lens_page(file, &image, system);
    };
    let mut command = curl();
    command.args(["-w", "\n%{redirect_url}\n%{http_code}", "-F"]).arg(field).arg(url);
    let (text, status) = exchange(&mut command, body, progress, system)?;
    let (body, redirect) = text.rsplit_once('\n').unwrap_or_default();
    if status.starts_with('3') && redirect.starts_with("https://") {
        return Ok(redirect.to_string());
    }
    check(&status, body)?;
    let value: serde_json::Value = serde_json::from_str(body).context(tr().no_link)?;
    let id = value.pointer("/blocks/0/params/cbirId").and_then(|id| id.as_str()).context(tr().no_link)?;
    Ok(format!("{YANDEX_IMAGES}&cbir_id={}", encode(id)))
}

pub(super) fn ascii_name(file: &Path) -> String {
    base(file)
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') { c } else { '_' })
        .collect()
}

pub(super) fn mime(file: &Path) -> &'static str {
    match extension(file).as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "avif" => "image/avif",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "css" => "text/css",
        "js" => "text/javascript",
        "json" => "application/json",
        "mp4" => "video/mp4",
        "mov" => "video/quicktime",
        "webm" => "video/webm",
        "mp3" => "audio/mpeg",
        "pdf" => "application/pdf",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

impl Upload {
    pub(super) fn ready(&self) -> bool {
        !self.url.trim().is_empty() && !self.field.trim().is_empty()
    }

    pub(super) fn send(&self, file: &Path, progress: &Progress, system: &dyn Services) -> Result<String> {
        let mut command = curl();
        command.arg("-F").arg(format!("{}=@-;filename={}", self.field.trim(), ascii_name(file)));
        for (key, value) in &self.form {
            command.arg("--form-string").arg(format!("{key}={value}"));
        }
        if !self.header.trim().is_empty() {
            command.arg("-H").arg(self.header.trim());
        }
        let body = request(command.arg(self.url.trim()), fs::read(file)?, progress, system)?;
        if self.link.trim().is_empty() {
            return Ok(body.trim().to_string());
        }
        let value: serde_json::Value = serde_json::from_str(&body)?;
        value.pointer(self.link.trim()).and_then(|link| link.as_str()).map(str::to_string).context(tr().no_link)
    }
}

#[cfg(test)]
mod tests;
