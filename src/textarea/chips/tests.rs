use super::*;

fn id(raw: &str) -> ItemId {
    serde_json::from_value(serde_json::Value::String(raw.to_owned())).expect("an id is a plain string")
}

fn stored() -> String {
    format!("Отправить {} и {} сегодня", mention::token(&id("1"), "договор.pdf"), mention::token(&id("2"), "счёт"))
}

#[test]
fn stored_mentions_become_single_characters_and_come_back_unchanged() {
    let mut refs = Refs::default();
    let edit = refs.encode(&stored());
    assert_eq!(edit.chars().filter(|c| is_chip(*c)).count(), 2);
    assert!(edit.starts_with("Отправить ") && edit.ends_with(" сегодня"));
    assert_eq!(refs.decode(&edit), stored());
    assert_eq!(refs.plain(&edit), "Отправить @договор.pdf и @счёт сегодня");
}

#[test]
fn the_same_card_mentioned_twice_shares_one_reference() {
    let mut refs = Refs::default();
    let twice = format!("{a} {a}", a = mention::token(&id("7"), "a"));
    let edit = refs.encode(&twice);
    let chips: Vec<char> = edit.chars().filter(|c| is_chip(*c)).collect();
    assert_eq!(chips.len(), 2);
    assert_eq!(chips[0], chips[1]);
}

#[test]
fn private_use_characters_typed_or_stored_by_others_are_dropped() {
    let mut refs = Refs::default();
    assert_eq!(refs.encode("a\u{E000}b"), "ab");
    assert_eq!(strip("x\u{E123}y"), "xy");
}

#[test]
fn display_offsets_map_both_ways_and_snap_out_of_chips() {
    let mut refs = Refs::default();
    let edit = refs.encode(&stored());
    let display = Display::build(&edit, &refs, &HashMap::new());
    let first = display.spans[0].display.clone();
    let chip_at = display.spans[0].at;
    assert!(display.text[display.spans[0].label.clone()].contains("договор.pdf"));
    assert_eq!(display.to_display(chip_at), first.start);
    assert_eq!(display.to_display(chip_at + 3), first.end);
    assert_eq!(display.to_edit(first.start + 1), chip_at);
    assert_eq!(display.to_edit(first.end - 1), chip_at + 3);
    let tail_edit = edit.len() - " сегодня".len();
    let tail_display = display.text.len() - " сегодня".len();
    assert_eq!(display.to_display(tail_edit), tail_display);
    assert_eq!(display.to_edit(tail_display), tail_edit);
    assert_eq!(display.to_display(3), 3);
}

#[test]
fn chips_show_the_current_name_from_the_shelf() {
    let mut refs = Refs::default();
    let edit = refs.encode(&stored());
    let looks = HashMap::from([(id("1"), Chip { label: "Договор с клиентом".into(), icon: "file", missing: false })]);
    let display = Display::build(&edit, &refs, &looks);
    assert_eq!(&display.text[display.spans[0].label.clone()], "Договор\u{00A0}с\u{00A0}клиентом");
}

#[test]
fn long_names_are_shortened_inside_the_chip() {
    let mut refs = Refs::default();
    let edit = refs.encode(&mention::token(&id("1"), &"очень длинное имя файла ".repeat(4)));
    let display = Display::build(&edit, &refs, &HashMap::new());
    let label = &display.text[display.spans[0].label.clone()];
    assert!(label.chars().count() <= LABEL_LIMIT && label.ends_with('…'), "{label}");
}

#[test]
fn a_trigger_starts_a_query_only_at_the_start_of_a_word() {
    assert_eq!(query("@", 1), Some((0, "")));
    assert_eq!(query("см. \\дог", "см. \\дог".len()), Some(("см. ".len(), "дог")));
    assert_eq!(query("C:\\Users", 8), None, "a path is not a mention");
    assert_eq!(query("см. @дог", "см. @дог".len()), Some(("см. ".len(), "дог")));
    assert_eq!(query("(@a", 3), Some((1, "a")));
    assert_eq!(query("ivan@mail", 9), None);
    assert_eq!(query("@two words", 10), None);
    assert_eq!(query("@a b", 4), None);
    assert_eq!(query("@x", 0), None);
    let long = format!("@{}", "a".repeat(60));
    assert_eq!(query(&long, long.len()), None);
}
