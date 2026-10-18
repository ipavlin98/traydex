use super::*;
use crate::actions::{Action, Kind, run};
use crate::store::Payload;
use crate::testing::{Fake, Sandbox};
use sevenz_rust2::encoder_options::AesEncoderOptions;
use sevenz_rust2::{ArchiveEntry, ArchiveWriter};
use std::collections::BTreeMap;
use zip::write::SimpleFileOptions;
use zip::{AesMode, ZipWriter};

const PLAIN_RAR: &[u8] = include_bytes!("plain.rar");
const LOCKED_RAR: &[u8] = include_bytes!("locked.rar");
const HIDDEN_RAR: &[u8] = include_bytes!("hidden.rar");
const RAR_PASSWORD: &str = "secret";
const PASSWORD: &str = "пароль 123";

struct Bench {
    sandbox: Sandbox,
    fake: Fake,
    storage: Storage,
}

impl Bench {
    fn new() -> Self {
        let sandbox = Sandbox::new();
        let fake = Fake::new(&sandbox);
        let storage = sandbox.store().storage().clone();
        Self { sandbox, fake, storage }
    }

    fn run(&self, files: &[&PathBuf]) -> Result<Outcome> {
        let payload = Payload::Paths(files.iter().map(|file| (*file).clone()).collect());
        run(&Action::new(Kind::Unpack), payload, &Progress::default(), &self.storage, &self.fake)
    }

    fn unlock(&self, archives: &[&PathBuf], password: &str) -> Unlocked {
        let archives: Vec<PathBuf> = archives.iter().map(|archive| (*archive).clone()).collect();
        unlock(&archives, password, &Progress::default(), &self.storage).expect("unlock")
    }

    fn zip(&self, name: &str, entries: &[(&str, &str)], password: Option<&str>) -> PathBuf {
        let path = self.sandbox.join(name);
        let mut writer = ZipWriter::new(fs::File::create(&path).expect("zip"));
        for (entry, text) in entries {
            let options = SimpleFileOptions::default();
            let options = match password {
                Some(password) => options.with_aes_encryption(AesMode::Aes256, password),
                None => options,
            };
            writer.start_file(*entry, options).expect("entry");
            writer.write_all(text.as_bytes()).expect("write");
        }
        writer.finish().expect("finish");
        path
    }

    fn seven(&self, name: &str, entries: &[(&str, &str)], password: Option<&str>, hidden: bool) -> PathBuf {
        let path = self.sandbox.join(name);
        let mut writer = ArchiveWriter::create(&path).expect("7z");
        if let Some(password) = password {
            writer.set_content_methods(vec![
                AesEncoderOptions::new(Password::from(password)).into(),
                EncoderMethod::LZMA2.into(),
            ]);
            writer.set_encrypt_header(hidden);
        }
        for (entry, text) in entries {
            writer.push_archive_entry(ArchiveEntry::new_file(entry), Some(text.as_bytes())).expect("entry");
        }
        writer.finish().expect("finish");
        path
    }

    fn top(&self) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(self.sandbox.path())
            .expect("sandbox")
            .map(|entry| entry.expect("entry").file_name().to_string_lossy().into_owned())
            .filter(|name| name != "root" && name != "temp")
            .collect();
        names.sort();
        names
    }
}

fn tree(folder: &Path) -> BTreeMap<String, String> {
    let mut files = BTreeMap::new();
    let mut pending = vec![folder.to_path_buf()];
    while let Some(path) = pending.pop() {
        for entry in fs::read_dir(&path).expect("folder") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                pending.push(path);
            } else {
                let name = path.strip_prefix(folder).expect("inside").to_string_lossy().replace('\\', "/");
                files.insert(name, fs::read_to_string(&path).expect("text"));
            }
        }
    }
    files
}

fn expected(entries: &[(&str, &str)]) -> BTreeMap<String, String> {
    entries.iter().map(|(name, text)| (name.to_string(), text.to_string())).collect()
}

fn locked(outcome: &Outcome) -> Vec<PathBuf> {
    match &outcome.show {
        Some(Show::Password(archives)) => archives.clone(),
        _ => Vec::new(),
    }
}

const FILES: [(&str, &str); 3] = [("Отчёт.txt", "квартал"), ("Папка/b.txt", "b"), ("Папка/Вложенная/c.txt", "c")];

#[test]
fn archive_unpacks_into_a_folder_named_after_it_next_to_it() {
    let bench = Bench::new();
    let archive = bench.zip("Фото с отпуска.zip", &FILES, None);
    let outcome = bench.run(&[&archive]).expect("unpack");
    let folder = bench.sandbox.join("Фото с отпуска");
    assert_eq!(outcome.created, std::slice::from_ref(&folder));
    assert_eq!(tree(&folder), expected(&FILES));
    assert_eq!(outcome.message, fill(tr().unpacked, 1));
    assert!(locked(&outcome).is_empty());
    assert!(archive.is_file(), "the archive stays");
}

#[test]
fn single_top_level_entry_is_not_wrapped_in_another_folder() {
    let bench = Bench::new();
    let taken = bench.sandbox.file("docs/old.txt", "old");
    let nested = bench.zip("docs.zip", &[("docs/a.txt", "a"), ("docs/sub/b.txt", "b")], None);
    let outcome = bench.run(&[&nested]).expect("folder");
    let folder = bench.sandbox.join("docs (2)");
    assert_eq!(outcome.created, std::slice::from_ref(&folder));
    assert_eq!(tree(&folder), expected(&[("a.txt", "a"), ("sub/b.txt", "b")]));
    assert_eq!(fs::read_to_string(&taken).expect("untouched"), "old");
    let lonely = bench.zip("report.zip", &[("report.pdf", "%PDF")], None);
    let outcome = bench.run(&[&lonely]).expect("file");
    assert_eq!(outcome.created, [bench.sandbox.join("report.pdf")]);
    assert_eq!(fs::read_to_string(bench.sandbox.join("report.pdf")).expect("file"), "%PDF");
    assert_eq!(bench.top(), ["docs", "docs (2)", "docs.zip", "report.pdf", "report.zip"], "no staging folders remain");
}

#[test]
fn entries_cannot_escape_the_target_folder() {
    let bench = Bench::new();
    let names = [
        ("../evil.txt", "1"),
        ("..\\evil.txt", "2"),
        ("a/../../evil.txt", "3"),
        ("C:/Windows/x.txt", "4"),
        ("ok.txt", "5"),
    ];
    let archive = bench.zip("trap.zip", &names, None);
    let outcome = bench.run(&[&archive]).expect("unpack");
    let folder = bench.sandbox.join("trap");
    assert_eq!(outcome.created, std::slice::from_ref(&folder));
    assert_eq!(tree(&folder), expected(&[("C_/Windows/x.txt", "4"), ("ok.txt", "5")]));
    assert_eq!(bench.top(), ["trap", "trap.zip"]);
}

#[test]
fn password_protected_zip_waits_for_the_right_password() {
    let bench = Bench::new();
    let archive = bench.zip("secret.zip", &FILES, Some(PASSWORD));
    let outcome = bench.run(&[&archive]).expect("asks");
    assert_eq!(locked(&outcome), std::slice::from_ref(&archive));
    assert!(outcome.created.is_empty() && outcome.message.is_empty());
    assert_eq!(bench.top(), ["secret.zip"], "nothing is written before the password");
    let wrong = bench.unlock(&[&archive], "пароль");
    assert_eq!((wrong.created.len(), wrong.locked), (0, vec![archive.clone()]));
    assert_eq!(bench.top(), ["secret.zip"], "a wrong password leaves nothing behind");
    let right = bench.unlock(&[&archive], PASSWORD);
    assert_eq!(right.created, [bench.sandbox.join("secret")]);
    assert!(right.locked.is_empty());
    assert_eq!(tree(&right.created[0]), expected(&FILES));
}

#[test]
fn password_protected_7z_waits_for_the_right_password_with_and_without_hidden_names() {
    for hidden in [false, true] {
        let bench = Bench::new();
        let archive = bench.seven("архив.7z", &FILES, Some(PASSWORD), hidden);
        let outcome = bench.run(&[&archive]).expect("asks");
        assert_eq!(locked(&outcome), std::slice::from_ref(&archive), "hidden names: {hidden}");
        assert_eq!(bench.unlock(&[&archive], "wrong").locked, std::slice::from_ref(&archive), "hidden names: {hidden}");
        assert_eq!(bench.top(), ["архив.7z"], "hidden names: {hidden}");
        let right = bench.unlock(&[&archive], PASSWORD);
        assert_eq!(right.created, [bench.sandbox.join("архив")], "hidden names: {hidden}");
        assert_eq!(tree(&right.created[0]), expected(&FILES), "hidden names: {hidden}");
    }
}

#[test]
fn plain_7z_unpacks() {
    let bench = Bench::new();
    let archive = bench.seven("plain.7z", &FILES, None, false);
    let outcome = bench.run(&[&archive]).expect("unpack");
    assert_eq!(tree(&outcome.created[0]), expected(&FILES));
}

#[test]
fn rar_unpacks_and_asks_for_passwords_of_data_and_names() {
    let bench = Bench::new();
    let contents = expected(&[("Привет.txt", "привет"), ("inner/b.txt", "b")]);
    let plain = bench.sandbox.file("plain.rar", PLAIN_RAR);
    let outcome = bench.run(&[&plain]).expect("plain");
    assert_eq!(outcome.created, [bench.sandbox.join("docs")]);
    assert_eq!(tree(&outcome.created[0]), contents);
    for (name, bytes) in [("locked.rar", LOCKED_RAR), ("hidden.rar", HIDDEN_RAR)] {
        let archive = bench.sandbox.file(name, bytes);
        assert_eq!(locked(&bench.run(&[&archive]).expect("asks")), std::slice::from_ref(&archive), "{name}");
        assert_eq!(bench.unlock(&[&archive], "wrong").locked, std::slice::from_ref(&archive), "{name}");
        let right = bench.unlock(&[&archive], RAR_PASSWORD);
        assert_eq!(tree(&right.created[0]), contents, "{name}");
        fs::remove_dir_all(&right.created[0]).expect("clean");
    }
    assert_eq!(bench.top(), ["docs", "hidden.rar", "locked.rar", "plain.rar"]);
}

#[test]
fn compressed_tarballs_and_single_compressed_files_unpack() {
    let bench = Bench::new();
    let mut builder = tar::Builder::new(Vec::new());
    for (name, text) in FILES {
        let mut header = tar::Header::new_gnu();
        header.set_size(text.len() as u64);
        header.set_mode(0o644);
        builder.append_data(&mut header, name, text.as_bytes()).expect("entry");
    }
    let tarball = builder.into_inner().expect("tar");
    let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gzip.write_all(&tarball).expect("gzip");
    let gz = bench.sandbox.file("bundle.tar.gz", gzip.finish().expect("gzip"));
    let zst = bench.sandbox.file("bundle.tzst", zstd::encode_all(tarball.as_slice(), 0).expect("zstd"));
    let plain = bench.sandbox.file("bundle.tar", &tarball);
    for (archive, folder) in [(&gz, "bundle"), (&zst, "bundle (2)"), (&plain, "bundle (3)")] {
        let outcome = bench.run(&[archive]).expect("tarball");
        assert_eq!(outcome.created, [bench.sandbox.join(folder)]);
        assert_eq!(tree(&outcome.created[0]), expected(&FILES), "{folder}");
    }
    let mut bzip = bzip2::write::BzEncoder::new(Vec::new(), bzip2::Compression::default());
    bzip.write_all("строка".as_bytes()).expect("bzip2");
    let single = bench.sandbox.file("notes.txt.bz2", bzip.finish().expect("bzip2"));
    let outcome = bench.run(&[&single]).expect("single");
    assert_eq!(outcome.created, [bench.sandbox.join("notes.txt")]);
    assert_eq!(fs::read_to_string(&outcome.created[0]).expect("text"), "строка");
}

#[test]
fn files_that_are_not_archives_are_skipped_or_refused() {
    let bench = Bench::new();
    let text = bench.sandbox.file("a.txt", "просто текст");
    let renamed = bench.sandbox.file("fake.zip", "not a zip");
    let error = bench.run(&[&text, &renamed]).err().expect("refused").to_string();
    assert_eq!(error, tr().unpack_needs);
    let archive = bench.zip("real.zip", &FILES, None);
    let outcome = bench.run(&[&text, &archive]).expect("partly");
    assert_eq!(outcome.message, format!("{} · {}", fill(tr().unpacked, 1), fill(tr().skipped, 1)));
}

#[test]
fn plain_and_locked_archives_dropped_together() {
    let bench = Bench::new();
    let open = bench.zip("open.zip", &FILES, None);
    let closed = bench.zip("closed.zip", &FILES, Some(PASSWORD));
    let outcome = bench.run(&[&open, &closed]).expect("mixed");
    assert_eq!(outcome.created, [bench.sandbox.join("open")]);
    assert_eq!(locked(&outcome), [closed]);
    assert_eq!(outcome.message, fill(tr().unpacked, 1));
}
