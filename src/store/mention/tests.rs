use super::*;

fn id(raw: &str) -> ItemId {
    ItemId(raw.to_owned())
}

#[test]
fn tokens_read_back_with_their_place_name_and_card() {
    let text = format!("Отправить {} и {}.", token(&id("17"), "договор.pdf"), token(&id("42"), "Счёт за май"));
    let found = mentions(&text);
    assert_eq!(found.len(), 2);
    assert_eq!((found[0].name, found[0].id.clone()), ("договор.pdf", id("17")));
    assert_eq!((found[1].name, found[1].id.clone()), ("Счёт за май", id("42")));
    assert_eq!(&text[found[0].range.clone()], token(&id("17"), "договор.pdf"));
    assert!(text[found[1].range.end..].starts_with('.'));
}

#[test]
fn names_that_would_break_the_token_are_made_safe() {
    let text = token(&id("5"), "report [final]\nv2");
    let found = mentions(&text);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].name, "report (final) v2");
}

#[test]
fn plain_text_shows_mentions_as_names() {
    let text = format!("См. {}, потом почта ivan@mail.ru", token(&id("9"), "photo.png"));
    assert_eq!(plain(&text), "См. @photo.png, потом почта ivan@mail.ru");
    assert!(matches!(plain("no mentions"), Cow::Borrowed(_)));
}

#[test]
fn look_alike_text_typed_by_hand_is_not_a_mention() {
    for text in [
        "@[name](https://example.com)",
        "@[name](traydex:)",
        "@[name](traydex:12 34)",
        "@[na\nme](traydex:1)",
        "@[name](traydex:1",
        "@[[x](traydex:1)",
    ] {
        assert!(mentions(text).is_empty(), "{text:?}");
        assert_eq!(plain(text), text);
    }
}

#[test]
fn a_broken_token_does_not_hide_the_next_one() {
    let good = token(&id("3"), "b");
    let text = format!("@[a](traydex: {good}");
    let found = mentions(&text);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].id, id("3"));
}
