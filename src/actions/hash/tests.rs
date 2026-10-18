use super::*;
use crate::actions::{Action, Kind, Outcome, run};
use crate::store::Payload;
use crate::store::Storage;
use crate::testing::{Call, Fake, Sandbox, output};
use std::io;

const ABC_SHA256: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
const ABC_MD5: &str = "900150983cd24fb0d6963f7d28e17f72";
const EMPTY_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
const EMPTY_MD5: &str = "d41d8cd98f00b204e9800998ecf8427e";

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

    fn hash(&self, algorithm: Algorithm, key: &str, files: &[&PathBuf]) -> Result<Outcome> {
        let kind = Kind::Hash { algorithm, virustotal: key.into() };
        let payload = Payload::Paths(files.iter().map(|file| (*file).clone()).collect());
        run(&Action::new(kind), payload, &Progress::default(), &self.storage, &self.fake)
    }

    fn check(&self, outcome: &Outcome) -> Vec<Verdict> {
        match &outcome.show {
            Some(Show::Hashes(digests, Some(lookup))) => lookup.verdicts(digests, &Progress::default(), &self.fake),
            _ => panic!("expected a VirusTotal lookup"),
        }
    }

    fn respond(&self, status: &str, body: &str) {
        self.fake.state().outputs.push_back(Ok(output(0, &format!("{body}\n{status}"), "")));
    }

    fn lookups(&self) -> Vec<crate::testing::Invocation> {
        self.fake
            .calls()
            .into_iter()
            .filter_map(|call| match call {
                Call::Output(invocation) => Some(invocation),
                _ => None,
            })
            .collect()
    }
}

fn digests(outcome: &Outcome) -> &[Digest] {
    match &outcome.show {
        Some(Show::Hashes(digests, _)) => digests,
        _ => panic!("expected hashes"),
    }
}

fn stats(malicious: u64, suspicious: u64, undetected: u64, harmless: u64) -> String {
    format!(
        r#"{{"data":{{"attributes":{{"last_analysis_stats":{{"malicious":{malicious},"suspicious":{suspicious},"undetected":{undetected},"harmless":{harmless},"timeout":3,"type-unsupported":9}}}}}}}}"#
    )
}

#[test]
fn hashes_match_reference_values_and_copy_the_chosen_one() {
    let bench = Bench::new();
    let file = bench.sandbox.file("папка с пробелами/файл abc.txt", "abc");
    let empty = bench.sandbox.file("empty.bin", "");
    let outcome = bench.hash(Algorithm::Sha256, "", &[&file]).expect("hash");
    assert_eq!(outcome.clipboard.as_deref(), Some(ABC_SHA256));
    assert_eq!(digests(&outcome)[0].md5, ABC_MD5);
    assert_eq!(digests(&outcome)[0].verdict, None);
    assert_eq!(outcome.message, fill(tr().hash_copied, "SHA-256"));
    let outcome = bench.hash(Algorithm::Md5, "", &[&empty]).expect("hash");
    assert_eq!(outcome.clipboard.as_deref(), Some(EMPTY_MD5));
    assert_eq!(digests(&outcome)[0].sha256, EMPTY_SHA256);
    assert!(bench.lookups().is_empty());
}

#[test]
fn several_files_are_listed_like_checksum_files() {
    let bench = Bench::new();
    let first = bench.sandbox.file("a b.txt", "abc");
    let second = bench.sandbox.file("пусто.txt", "");
    let folder = bench.sandbox.folder("Folder");
    let outcome = bench.hash(Algorithm::Sha256, "", &[&first, &folder, &second]).expect("hash");
    let expected = format!("{ABC_SHA256}  a b.txt\r\n{EMPTY_SHA256}  пусто.txt");
    assert_eq!(outcome.clipboard.as_deref(), Some(expected.as_str()));
    assert_eq!(outcome.message, fill(tr().hashes_copied, 2));
    let failure = bench.hash(Algorithm::Sha256, "", &[&folder]).err().expect("folders only");
    assert_eq!(failure.to_string(), tr().needs_file_input);
}

#[test]
fn virustotal_key_travels_through_stdin_not_arguments() {
    let bench = Bench::new();
    let file = bench.sandbox.file("setup.exe", "abc");
    bench.respond("200", &stats(0, 0, 60, 10));
    let outcome = bench.hash(Algorithm::Sha256, "  SECRET-KEY \r\n", &[&file]).expect("hash");
    bench.check(&outcome);
    let lookups = bench.lookups();
    assert_eq!(lookups.len(), 1);
    let args = lookups[0].args().join(" ");
    assert!(!args.contains("SECRET-KEY"), "{args}");
    assert!(args.ends_with(&format!("https://www.virustotal.com/api/v3/files/{ABC_SHA256}")), "{args}");
    assert_eq!(lookups[0].input.as_deref(), Some(&b"x-apikey: SECRET-KEY\n"[..]));
}

#[test]
fn virustotal_answers_become_verdicts() {
    assert_eq!(verdict("200", &stats(0, 0, 60, 10)), Verdict::Clean { total: 70 });
    assert_eq!(verdict("200", &stats(3, 2, 55, 10)), Verdict::Flagged { detected: 5, total: 70 });
    assert_eq!(verdict("200", r#"{"data":{"attributes":{}}}"#), Verdict::Unknown);
    assert_eq!(verdict("200", "not json"), Verdict::Unknown);
    assert_eq!(verdict("200", &stats(0, 0, 0, 0)), Verdict::Unknown);
    assert_eq!(verdict("404", r#"{"error":{"code":"NotFoundError"}}"#), Verdict::Unknown);
    assert_eq!(verdict("401", ""), Verdict::Failed(tr().key_rejected.into()));
    assert_eq!(verdict("403", ""), Verdict::Failed(tr().key_rejected.into()));
    assert_eq!(verdict("429", ""), Verdict::Failed(tr().vt_quota.into()));
    assert_eq!(verdict("500", "oops"), Verdict::Failed("HTTP 500".into()));
    assert!(Verdict::Clean { total: 70 }.summary().contains("0/70"));
    assert!(Verdict::Flagged { detected: 5, total: 70 }.summary().contains("5/70"));
}

#[test]
fn hashes_come_at_once_and_the_verdict_follows() {
    let bench = Bench::new();
    let file = bench.sandbox.file("a.txt", "abc");
    bench.respond("200", &stats(4, 0, 50, 0));
    let outcome = bench.hash(Algorithm::Sha256, "key", &[&file]).expect("hash");
    assert!(bench.lookups().is_empty(), "the hash page must not wait for the network");
    assert_eq!(digests(&outcome)[0].verdict, Some(Verdict::Checking));
    assert_eq!(outcome.message, fill(tr().hash_copied, "SHA-256"));
    assert_eq!(bench.check(&outcome), [Verdict::Flagged { detected: 4, total: 54 }]);
    assert_eq!(digests(&outcome)[0].report(), format!("https://www.virustotal.com/gui/file/{ABC_SHA256}"));
}

#[test]
fn quota_stops_further_lookups() {
    let bench = Bench::new();
    let files: Vec<PathBuf> = (0..3).map(|n| bench.sandbox.file(&format!("{n}.txt"), n.to_string())).collect();
    bench.respond("200", &stats(0, 0, 1, 0));
    bench.respond("429", r#"{"error":{"code":"QuotaExceededError"}}"#);
    let outcome = bench.hash(Algorithm::Sha256, "key", &files.iter().collect::<Vec<_>>()).expect("hash");
    let verdicts = bench.check(&outcome);
    assert_eq!(bench.lookups().len(), 2);
    let quota = Verdict::Failed(tr().vt_quota.into());
    assert_eq!(verdicts, [Verdict::Clean { total: 1 }, quota.clone(), quota]);
}

#[test]
fn network_failures_keep_the_hash() {
    let bench = Bench::new();
    let file = bench.sandbox.file("a.txt", "abc");
    bench.fake.state().outputs.push_back(Ok(output(6, "", "curl: (6) Could not resolve host")));
    let outcome = bench.hash(Algorithm::Sha256, "key", &[&file]).expect("hash");
    assert_eq!(outcome.clipboard.as_deref(), Some(ABC_SHA256));
    assert_eq!(bench.check(&outcome), [Verdict::Failed(tr().no_connection.into())]);
    bench.fake.state().outputs.push_back(Err(io::ErrorKind::NotFound.into()));
    let outcome = bench.hash(Algorithm::Sha256, "key", &[&file]).expect("hash");
    assert!(matches!(bench.check(&outcome)[..], [Verdict::Failed(_)]));
}

#[test]
fn hashing_one_large_file_reports_progress_by_bytes() {
    let bench = Bench::new();
    let image = bench.sandbox.file("disk.iso", vec![2u8; 3 * CHUNK + 5]);
    let progress = Progress::default();
    let kind = Kind::Hash { algorithm: Algorithm::Sha256, virustotal: String::new() };
    run(&Action::new(kind), Payload::Paths(vec![image]), &progress, &bench.storage, &bench.fake).expect("hash");
    assert_eq!(progress.get().fraction, Some(1.));
}
