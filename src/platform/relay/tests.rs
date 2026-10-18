use super::*;

fn copy_data(kind: usize, words: &[u16]) -> COPYDATASTRUCT {
    COPYDATASTRUCT { dwData: kind, cbData: (words.len() * 2) as u32, lpData: words.as_ptr() as _ }
}

fn encoded(paths: &[&str]) -> Vec<u16> {
    paths.iter().flat_map(|path| OsString::from(path).encode_wide().chain([0]).collect::<Vec<_>>()).collect()
}

fn receive(data: &COPYDATASTRUCT) -> Option<Vec<PathBuf>> {
    received(LPARAM(data as *const _ as isize))
}

#[test]
fn forwarded_paths_are_decoded() {
    let words = encoded(&[r"C:\Users\Имя\файл.txt", r"\\server\share\a b.pdf", r"D:\🙂.png"]);
    let paths = receive(&copy_data(PATHS_DATA, &words)).expect("paths");
    assert_eq!(
        paths,
        [
            PathBuf::from(r"C:\Users\Имя\файл.txt"),
            PathBuf::from(r"\\server\share\a b.pdf"),
            PathBuf::from(r"D:\🙂.png")
        ]
    );
}

#[test]
fn empty_segments_are_skipped() {
    let mut words = vec![0, 0];
    words.extend(encoded(&[r"C:\a.txt"]));
    words.extend([0, 0]);
    assert_eq!(receive(&copy_data(PATHS_DATA, &words)).expect("paths"), [PathBuf::from(r"C:\a.txt")]);
    assert_eq!(receive(&copy_data(PATHS_DATA, &[])).map(|paths| paths.len()), Some(0));
}

#[test]
fn foreign_or_empty_messages_are_ignored() {
    let words = encoded(&[r"C:\a.txt"]);
    assert!(receive(&copy_data(PATHS_DATA + 1, &words)).is_none());
    let null = COPYDATASTRUCT { dwData: PATHS_DATA, cbData: 10, lpData: std::ptr::null_mut() };
    assert!(receive(&null).is_none());
    assert!(received(LPARAM(0)).is_none());
}

#[test]
fn odd_byte_count_ignores_the_trailing_byte() {
    let words = encoded(&[r"C:\a"]);
    let mut data = copy_data(PATHS_DATA, &words);
    data.cbData += 1;
    assert_eq!(receive(&data).expect("paths"), [PathBuf::from(r"C:\a")]);
}

#[test]
fn wide_text_is_truncated_and_terminated() {
    let slot: [u16; 6] = wide("Traydex");
    assert_eq!(String::from_utf16_lossy(&slot[..5]), "Trayd");
    assert_eq!(slot[5], 0);
    let short: [u16; 8] = wide("Hi");
    assert_eq!(&short[..3], &[b'H' as u16, b'i' as u16, 0]);
    let _: [u16; 0] = wide("x");
}

#[test]
fn shortcuts_survive_message_packing() {
    let samples = [
        Shortcut::default(),
        Shortcut { ctrl: true, shift: true, key: VK_SPACE.0, ..Default::default() },
        Shortcut { alt: true, win: true, key: VK_F24.0, ..Default::default() },
        Shortcut { ctrl: true, shift: true, alt: true, win: true, key: 0xFFFF },
    ];
    for shortcut in samples {
        assert_eq!(unpack(pack(shortcut)), shortcut);
    }
}

#[test]
fn hotkey_ids_are_unique_and_nonzero() {
    let ids: std::collections::HashSet<i32> = Command::ALL.iter().map(|command| hotkey_id(*command)).collect();
    assert_eq!(ids.len(), Command::ALL.len());
    assert!(!ids.contains(&0));
}

#[test]
fn tray_tip_shows_toggle_shortcut() {
    HOTKEYS.with_borrow_mut(|hotkeys| hotkeys.clear());
    assert_eq!(tip(), "Traydex");
    let shortcut = Command::Toggle.default_shortcut();
    HOTKEYS.with_borrow_mut(|hotkeys| hotkeys.insert(Command::Toggle, shortcut));
    assert_eq!(tip(), format!("Traydex · {}", shortcut.text()));
    HOTKEYS.with_borrow_mut(|hotkeys| hotkeys.clear());
}
