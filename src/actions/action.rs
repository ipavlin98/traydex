use super::{Custom, Fetch, Tool, base, compress, convert, normalized, ocr, offered, pdf, textual, video};
use crate::i18n::{Strings, fill, tr};
use crate::store;
use crate::system::Offer;
use image::ImageFormat;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

pub const ENGINES: [&str; 5] = [
    "https://www.google.com/search?q=%s",
    "https://yandex.ru/search/?text=%s",
    "https://www.bing.com/search?q=%s",
    "https://duckduckgo.com/?q=%s",
    "https://www.youtube.com/results?search_query=%s",
];

pub fn engine_name(url: &str) -> Option<&'static str> {
    Some(match ENGINES.iter().position(|engine| *engine == url.trim())? {
        0 => "Google",
        1 => tr().yandex,
        2 => "Bing",
        3 => "DuckDuckGo",
        _ => "YouTube",
    })
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Action {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub icon_only: bool,
    #[serde(flatten)]
    pub kind: Kind,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Kind {
    Move {
        folder: PathBuf,
    },
    Copy {
        folder: PathBuf,
    },
    Zip,
    Jpeg,
    Png,
    Gif,
    Imgur {
        client_id: String,
    },
    CopyPath,
    Phone,
    Upload(Upload),
    Run {
        program: PathBuf,
    },
    Search {
        url: String,
    },
    DataUri,
    Hash {
        #[serde(default)]
        algorithm: Algorithm,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        virustotal: String,
    },
    Ocr {
        #[serde(default, skip_serializing_if = "String::is_empty")]
        language: String,
    },
    Compress {
        #[serde(default)]
        format: Compression,
    },
    Pdf,
    Unpack,
    Rename,
    Qr,
    Ytdlp(Fetch),
    Custom(Custom),
    Cancel,
}

#[derive(Clone, Copy, Default, PartialEq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Algorithm {
    #[default]
    Sha256,
    Md5,
}

impl Algorithm {
    pub fn label(self) -> &'static str {
        match self {
            Algorithm::Sha256 => "SHA-256",
            Algorithm::Md5 => "MD5",
        }
    }
}

#[derive(Clone, Copy, Default, PartialEq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Compression {
    #[default]
    Jpeg,
    Webp,
}

impl Compression {
    pub fn label(self) -> &'static str {
        match self {
            Compression::Jpeg => "JPEG",
            Compression::Webp => "WebP",
        }
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Upload {
    pub url: String,
    pub field: String,
    #[serde(default)]
    pub header: String,
    #[serde(default)]
    pub form: BTreeMap<String, String>,
    #[serde(default)]
    pub link: String,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Field {
    Name,
    Description,
    ClientId,
    Url,
    File,
    Header,
    Link,
    SearchUrl,
    ApiKey,
}

impl Field {
    pub fn label(self) -> &'static str {
        match self {
            Field::Name => tr().field_name,
            Field::Description => tr().field_description,
            Field::ClientId => "Client-ID",
            Field::Url => tr().field_url,
            Field::File => tr().field_file,
            Field::Header => tr().field_header,
            Field::Link => tr().field_link,
            Field::SearchUrl => tr().field_search_url,
            Field::ApiKey => tr().virustotal_key,
        }
    }

    pub fn placeholder(self) -> &'static str {
        match self {
            Field::Name | Field::Description => "",
            Field::ClientId => tr().client_id_placeholder,
            Field::Url => "https://example.com/upload",
            Field::File => "file",
            Field::Header | Field::Link => tr().optional,
            Field::SearchUrl => "https://example.com/search?q=%s",
            Field::ApiKey => tr().virustotal_placeholder,
        }
    }

    pub fn secret(self) -> bool {
        self == Field::ApiKey
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Single {
    Zip,
    Jpeg,
    Png,
    Gif,
    CopyPath,
    Phone,
    Imgur,
    DataUri,
    Hash,
    Ocr,
    Compress,
    Pdf,
    Unpack,
    Rename,
    Qr,
    Ytdlp,
    Cancel,
}

#[derive(Clone, PartialEq, Debug)]
pub enum Slot {
    Single(Single),
    Folder(String),
    Program(String),
    Search(String),
    Upload(String),
    Custom(String),
}

#[derive(Clone, Copy)]
enum Takes {
    Anything,
    Files(fn(&Path) -> bool),
    Disk,
}

const ANY_FILE: Takes = Takes::Files(|_| true);

impl Takes {
    fn admits(self, offer: &Offer) -> bool {
        match self {
            Takes::Anything => true,
            Takes::Files(kind) => offered(offer, |path, folder| !folder && kind(path)),
            Takes::Disk => matches!(offer, Offer::Files(_) | Offer::Unknown),
        }
    }
}

struct Spec {
    icon: &'static str,
    verb: fn(&Strings) -> &'static str,
    activity: fn(&Strings) -> &'static str,
    titled: bool,
    tools: &'static [Tool],
    fields: &'static [Field],
    takes: Takes,
}

const BASIC_FIELDS: &[Field] = &[Field::Name, Field::Description];

impl Spec {
    fn new(icon: &'static str, verb: fn(&Strings) -> &'static str, activity: fn(&Strings) -> &'static str) -> Self {
        Self { icon, verb, activity, titled: false, tools: &[], fields: BASIC_FIELDS, takes: Takes::Anything }
    }

    fn takes(self, takes: Takes) -> Self {
        Self { takes, ..self }
    }

    fn titled(self) -> Self {
        Self { titled: true, ..self }
    }

    fn tools(self, tools: &'static [Tool]) -> Self {
        Self { tools, ..self }
    }

    fn fields(self, fields: &'static [Field]) -> Self {
        Self { fields, ..self }
    }
}

impl Kind {
    fn spec(&self) -> Spec {
        match self {
            Kind::Move { .. } => Spec::new("folder", |s| s.verb_move, |s| s.job_move).titled(),
            Kind::Copy { .. } => Spec::new("folder", |s| s.verb_copy, |s| s.job_copy).titled(),
            Kind::Zip => Spec::new("file-archive", |s| s.verb_zip, |s| s.job_zip),
            Kind::Jpeg => Spec::new("image", |s| s.verb_convert, |s| s.job_convert)
                .takes(Takes::Files(|path| convert::convertible(path, ImageFormat::Jpeg))),
            Kind::Png => Spec::new("image", |s| s.verb_convert, |s| s.job_convert)
                .takes(Takes::Files(|path| convert::convertible(path, ImageFormat::Png))),
            Kind::Gif => Spec::new("film", |s| s.verb_gif, |s| s.job_gif)
                .tools(&[Tool::Ffmpeg])
                .takes(Takes::Files(video::is_video)),
            Kind::CopyPath => Spec::new("link", |s| s.verb_path, |s| s.job_path).takes(Takes::Disk),
            Kind::Phone => Spec::new("smartphone", |s| s.verb_phone, |s| s.job_phone).titled().takes(ANY_FILE),
            Kind::Imgur { .. } => Spec::new("upload", |s| s.verb_upload, |s| s.job_upload)
                .titled()
                .fields(&[Field::Name, Field::Description, Field::ClientId])
                .takes(Takes::Files(store::is_image)),
            Kind::Upload(_) => Spec::new("server", |s| s.verb_upload, |s| s.job_upload)
                .titled()
                .fields(&[Field::Name, Field::Description, Field::Url, Field::File, Field::Header, Field::Link])
                .takes(ANY_FILE),
            Kind::Run { .. } => Spec::new("terminal", |s| s.verb_run, |s| s.job_run).titled(),
            Kind::Search { url } => Spec::new("search", |s| s.verb_search, |s| s.job_search).titled().fields(
                if engine_name(url).is_none() {
                    &[Field::Name, Field::Description, Field::SearchUrl]
                } else {
                    BASIC_FIELDS
                },
            ),
            Kind::DataUri => Spec::new("code", |s| s.verb_base64, |s| s.job_base64).titled().takes(ANY_FILE),
            Kind::Hash { .. } => Spec::new("hash", |s| s.verb_hash, |s| s.job_hash)
                .fields(&[Field::Name, Field::Description, Field::ApiKey])
                .takes(ANY_FILE),
            Kind::Ocr { .. } => {
                Spec::new("scan-text", |s| s.verb_ocr, |s| s.job_ocr).takes(Takes::Files(ocr::readable))
            }
            Kind::Compress { .. } => {
                Spec::new("shrink", |s| s.verb_compress, |s| s.job_compress).takes(Takes::Files(compress::compressible))
            }
            Kind::Pdf => Spec::new("file-text", |s| s.verb_pdf, |s| s.job_pdf).takes(Takes::Files(pdf::printable)),
            Kind::Unpack => {
                Spec::new("unpack", |s| s.verb_unpack, |s| s.job_unpack).takes(Takes::Files(store::is_archive))
            }
            Kind::Rename => Spec::new("rename", |s| s.verb_rename, |s| s.job_rename).takes(Takes::Disk),
            Kind::Qr => Spec::new("qr", |s| s.verb_qr, |s| s.job_qr).takes(Takes::Files(textual)),
            Kind::Ytdlp(_) => Spec::new("video", |s| s.verb_ytdlp, |s| s.job_ytdlp)
                .tools(&[Tool::YtDlp, Tool::Ffmpeg, Tool::Deno])
                .takes(Takes::Files(textual)),
            Kind::Custom(_) => Spec::new("puzzle", |s| s.verb_run, |s| s.job_plugin).titled(),
            Kind::Cancel => Spec::new("close", |s| s.drop_cancel_hint, |s| s.drop_cancel),
        }
    }
}

impl Action {
    pub fn new(kind: Kind) -> Self {
        Self { name: String::new(), description: String::new(), icon_only: false, kind }
    }

    pub fn default_title(&self) -> String {
        match &self.kind {
            Kind::Move { folder } | Kind::Copy { folder } => base(folder),
            Kind::Run { program } => program.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
            Kind::Zip => fill(tr().to_format, "ZIP"),
            Kind::Jpeg => fill(tr().to_format, "JPEG"),
            Kind::Png => fill(tr().to_format, "PNG"),
            Kind::Gif => fill(tr().to_format, "GIF"),
            Kind::CopyPath => tr().path_title.into(),
            Kind::Phone => tr().add_phone.into(),
            Kind::Imgur { .. } => fill(tr().to_service, "Imgur"),
            Kind::Upload(upload) if !upload.url.trim().is_empty() => fill(tr().to_service, store::host(&upload.url)),
            Kind::Upload(_) => tr().add_upload.into(),
            Kind::Search { url } => match engine_name(url) {
                Some(name) => name.into(),
                None if !url.trim().is_empty() => store::host(url).into(),
                None => tr().add_search.into(),
            },
            Kind::DataUri => "Base64".into(),
            Kind::Hash { .. } => tr().add_hash.into(),
            Kind::Ocr { .. } => tr().add_ocr.into(),
            Kind::Compress { .. } => tr().add_compress.into(),
            Kind::Pdf => fill(tr().to_format, "PDF"),
            Kind::Unpack => tr().add_unpack.into(),
            Kind::Rename => tr().add_rename.into(),
            Kind::Qr => tr().add_qr.into(),
            Kind::Ytdlp(_) => tr().add_ytdlp.into(),
            Kind::Custom(custom) => custom.plugin().map_or_else(|| custom.id.clone(), |plugin| plugin.name().into()),
            Kind::Cancel => tr().drop_cancel.into(),
        }
    }

    pub fn title(&self) -> String {
        if self.name.trim().is_empty() { self.default_title() } else { self.name.clone() }
    }

    pub fn description(&self) -> String {
        if self.description.trim().is_empty() { self.verb().into() } else { self.description.clone() }
    }

    pub fn verb(&self) -> &'static str {
        (self.kind.spec().verb)(tr())
    }

    pub fn activity(&self) -> &'static str {
        (self.kind.spec().activity)(tr())
    }

    pub fn job_label(&self) -> String {
        if self.kind.spec().titled || !self.name.trim().is_empty() {
            format!("{}… · {}", self.activity(), self.title())
        } else {
            format!("{}…", self.activity())
        }
    }

    pub fn detail(&self) -> String {
        let target = match &self.kind {
            Kind::Move { folder } | Kind::Copy { folder } => folder.display().to_string(),
            Kind::Run { program } => program.display().to_string(),
            Kind::Upload(upload) => store::host(&upload.url).into(),
            Kind::Search { url } => store::host(url).into(),
            Kind::Hash { algorithm, virustotal } if virustotal.trim().is_empty() => algorithm.label().into(),
            Kind::Hash { algorithm, .. } => format!("{} · VirusTotal", algorithm.label()),
            Kind::Compress { format } => format.label().into(),
            Kind::Custom(custom) => match custom.plugin() {
                None => return fill(tr().plugin_missing, &custom.id),
                Some(plugin) if plugin.description().is_empty() => return self.verb().into(),
                Some(plugin) => return plugin.description().into(),
            },
            Kind::Zip
            | Kind::Jpeg
            | Kind::Png
            | Kind::Gif
            | Kind::Imgur { .. }
            | Kind::CopyPath
            | Kind::Phone
            | Kind::DataUri
            | Kind::Ocr { .. }
            | Kind::Pdf
            | Kind::Unpack
            | Kind::Rename
            | Kind::Qr
            | Kind::Ytdlp(_) => return self.verb().into(),
            Kind::Cancel => return tr().cancel_tile_note.into(),
        };
        format!("{} · {target}", self.verb())
    }

    pub fn icon(&self) -> &'static str {
        self.kind.spec().icon
    }

    pub fn tools(&self) -> &'static [Tool] {
        self.kind.spec().tools
    }

    pub fn ready(&self) -> bool {
        match &self.kind {
            Kind::Imgur { client_id } => !client_id.trim().is_empty(),
            Kind::Upload(upload) => upload.ready(),
            Kind::Search { url } => !url.trim().is_empty(),
            Kind::Custom(custom) => custom.ready(),
            Kind::Move { .. }
            | Kind::Copy { .. }
            | Kind::Zip
            | Kind::Jpeg
            | Kind::Png
            | Kind::Gif
            | Kind::CopyPath
            | Kind::Phone
            | Kind::Run { .. }
            | Kind::DataUri
            | Kind::Hash { .. }
            | Kind::Ocr { .. }
            | Kind::Compress { .. }
            | Kind::Pdf
            | Kind::Unpack
            | Kind::Rename
            | Kind::Qr
            | Kind::Ytdlp(_)
            | Kind::Cancel => true,
        }
    }

    pub fn fields(&self) -> &'static [Field] {
        self.kind.spec().fields
    }

    pub fn text_mut(&mut self, field: Field) -> Option<&mut String> {
        Some(match (&mut self.kind, field) {
            (_, Field::Name) => &mut self.name,
            (_, Field::Description) => &mut self.description,
            (Kind::Imgur { client_id }, Field::ClientId) => client_id,
            (Kind::Upload(upload), Field::Url) => &mut upload.url,
            (Kind::Upload(upload), Field::File) => &mut upload.field,
            (Kind::Upload(upload), Field::Header) => &mut upload.header,
            (Kind::Upload(upload), Field::Link) => &mut upload.link,
            (Kind::Search { url }, Field::SearchUrl) => url,
            (Kind::Hash { virustotal, .. }, Field::ApiKey) => virustotal,
            _ => return None,
        })
    }

    pub fn slot(&self) -> Slot {
        match &self.kind {
            Kind::Move { folder } | Kind::Copy { folder } => Slot::Folder(normalized(folder)),
            Kind::Run { program } => Slot::Program(normalized(program)),
            Kind::Zip => Slot::Single(Single::Zip),
            Kind::Jpeg => Slot::Single(Single::Jpeg),
            Kind::Png => Slot::Single(Single::Png),
            Kind::Gif => Slot::Single(Single::Gif),
            Kind::CopyPath => Slot::Single(Single::CopyPath),
            Kind::Phone => Slot::Single(Single::Phone),
            Kind::Imgur { .. } => Slot::Single(Single::Imgur),
            Kind::Upload(upload) => Slot::Upload(upload.url.trim().to_lowercase()),
            Kind::Search { url } => Slot::Search(url.trim().to_lowercase()),
            Kind::DataUri => Slot::Single(Single::DataUri),
            Kind::Hash { .. } => Slot::Single(Single::Hash),
            Kind::Ocr { .. } => Slot::Single(Single::Ocr),
            Kind::Compress { .. } => Slot::Single(Single::Compress),
            Kind::Pdf => Slot::Single(Single::Pdf),
            Kind::Unpack => Slot::Single(Single::Unpack),
            Kind::Rename => Slot::Single(Single::Rename),
            Kind::Qr => Slot::Single(Single::Qr),
            Kind::Ytdlp(_) => Slot::Single(Single::Ytdlp),
            Kind::Custom(custom) => Slot::Custom(custom.id.clone()),
            Kind::Cancel => Slot::Single(Single::Cancel),
        }
    }

    pub fn cancels(&self) -> bool {
        matches!(self.kind, Kind::Cancel)
    }

    pub fn path(&self) -> Option<&Path> {
        match &self.kind {
            Kind::Move { folder } | Kind::Copy { folder } => Some(folder),
            Kind::Run { program } => Some(program),
            _ => None,
        }
    }

    pub fn path_mut(&mut self) -> Option<&mut PathBuf> {
        match &mut self.kind {
            Kind::Move { folder } | Kind::Copy { folder } => Some(folder),
            Kind::Run { program } => Some(program),
            _ => None,
        }
    }

    pub fn picture(&self) -> Option<Arc<str>> {
        let Kind::Custom(custom) = &self.kind else {
            return None;
        };
        custom.plugin()?.icon()
    }

    pub fn accepts(&self, offer: &Offer) -> bool {
        let Kind::Custom(custom) = &self.kind else {
            return self.kind.spec().takes.admits(offer);
        };
        custom.plugin().is_none_or(|plugin| plugin.accepts().admits_offer(offer))
    }

    pub(super) fn needs_disk(&self) -> bool {
        matches!(self.kind.spec().takes, Takes::Disk)
    }

    pub fn timeout(&self) -> Option<Duration> {
        let Kind::Custom(custom) = &self.kind else {
            return None;
        };
        custom.plugin().map(|plugin| plugin.timeout())
    }

    pub fn text(&self, field: Field) -> String {
        self.clone().text_mut(field).map(std::mem::take).unwrap_or_default()
    }
}
