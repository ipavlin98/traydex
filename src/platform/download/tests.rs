use super::*;

#[test]
fn percent_reads_last_curl_progress_value() {
    assert_eq!(percent(b"###                12.5%"), Some(0.125));
    assert_eq!(percent(b"\r##  3.0%\r#####  47.9%"), Some(0.479));
    assert_eq!(percent(b"100.0%"), Some(1.));
    assert_eq!(percent(b"no progress"), None);
    assert_eq!(percent(b"%"), None);
    assert_eq!(percent(b"1.2.3%"), None);
}
