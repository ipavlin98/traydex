use super::buffer::utf8_offset;
use super::words::{word_end, word_start};
use super::{TextArea, clean, word_around};
use crate::input::Changed;
use gpui::{
    ClipboardItem, Context, Entity, EntityInputHandler, IntoElement, Modifiers, MouseButton, Render, TestAppContext,
    VisualTestContext, Window, div, point, prelude::*, px,
};
use std::cell::Cell;
use std::rc::Rc;

#[test]
fn utf16_offsets_map_to_char_boundaries() {
    let text = "aб😀c";
    assert_eq!(utf8_offset(text, 0), 0);
    assert_eq!(utf8_offset(text, 1), 1);
    assert_eq!(utf8_offset(text, 2), 3);
    assert_eq!(utf8_offset(text, 3), 7);
    assert_eq!(utf8_offset(text, 4), 7);
    assert_eq!(utf8_offset(text, 5), 8);
    assert_eq!(utf8_offset(text, 99), text.len());
    assert_eq!(utf8_offset("", 3), 0);
}

struct Host {
    area: Entity<TextArea>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w(px(300.)).h(px(120.)).child(self.area.clone())
    }
}

fn open<'a>(cx: &'a mut TestAppContext, text: &str) -> (Entity<TextArea>, &'a mut VisualTestContext) {
    let text = text.to_string();
    let (host, cx) =
        cx.add_window_view(move |_, cx| Host { area: cx.new(|cx| TextArea::new(&text, "Пусто", cx)) });
    let area = cx.update(|_, cx| host.read(cx).area.clone());
    cx.update(|window, cx| area.update(cx, |input, cx| input.focus(window, cx)));
    cx.run_until_parked();
    (area, cx)
}

fn text(area: &Entity<TextArea>, cx: &mut VisualTestContext) -> String {
    cx.update(|_, cx| area.read(cx).text().to_string())
}

fn selected(area: &Entity<TextArea>, cx: &mut VisualTestContext) -> String {
    area.update_in(cx, |area, window, cx| {
        let range = area.selected_text_range(false, window, cx).expect("selection").range;
        let mut adjusted = None;
        area.text_for_range(range, &mut adjusted, window, cx).expect("text")
    })
}

#[test]
fn words_stop_at_spaces_punctuation_and_line_breaks() {
    let text = "Съешь ещё,  этих\nбулок";
    assert_eq!(word_start(text, text.len()), text.find("булок").expect("word"));
    assert_eq!(word_start(text, text.find("булок").expect("word")), text.find('\n').expect("break"));
    assert_eq!(word_start(text, text.find("  этих").expect("gap") + 2), text.find(',').expect("comma"));
    assert_eq!(word_end(text, 0), "Съешь".len());
    assert_eq!(word_end(text, "Съешь".len()), "Съешь ещё".len());
    assert_eq!(word_end(text, text.find("этих").expect("word") + "этих".len()), text.find('\n').expect("break") + 1);
    assert_eq!(word_start("", 0), 0);
    assert_eq!(word_end("", 0), 0);
    assert_eq!(&text[word_around(text, 2)], "Съешь");
    assert_eq!(&text[word_around(text, text.find(',').expect("comma"))], ",");
}

#[test]
fn pasted_text_keeps_lines_but_drops_carriage_returns_and_controls() {
    assert_eq!(clean("a\r\nb\rc\u{7}\td"), "a\nb\nc\td");
}

#[gpui::test]
fn typing_and_enter_build_multiline_text(cx: &mut TestAppContext) {
    let (area, cx) = open(cx, "");
    let changes = Rc::new(Cell::new(0));
    let counter = changes.clone();
    cx.update(|_, cx| cx.subscribe(&area, move |_, _: &Changed, _| counter.set(counter.get() + 1)).detach());
    cx.simulate_input("Привет");
    cx.simulate_keystrokes("enter");
    cx.simulate_input("мир");
    assert_eq!(text(&area, cx), "Привет\nмир");
    assert_eq!(changes.get(), 10);
}

#[gpui::test]
fn backspace_and_delete_remove_whole_characters_and_words(cx: &mut TestAppContext) {
    let (area, cx) = open(cx, "Съешь ещё этих");
    cx.simulate_keystrokes("ctrl-end backspace");
    assert_eq!(text(&area, cx), "Съешь ещё эти");
    cx.simulate_keystrokes("ctrl-backspace");
    assert_eq!(text(&area, cx), "Съешь ещё ");
    cx.simulate_keystrokes("ctrl-home delete");
    assert_eq!(text(&area, cx), "ъешь ещё ");
    cx.simulate_keystrokes("ctrl-delete");
    assert_eq!(text(&area, cx), " ещё ");
}

#[gpui::test]
fn undo_and_redo_step_through_words(cx: &mut TestAppContext) {
    let (area, cx) = open(cx, "");
    cx.simulate_input("один два");
    cx.simulate_keystrokes("ctrl-z");
    assert_eq!(text(&area, cx), "один ");
    cx.simulate_keystrokes("ctrl-z ctrl-z");
    assert_eq!(text(&area, cx), "");
    cx.simulate_keystrokes("ctrl-z");
    assert_eq!(text(&area, cx), "");
    cx.simulate_keystrokes("ctrl-y");
    assert_eq!(text(&area, cx), "один");
    cx.simulate_keystrokes("ctrl-shift-z ctrl-shift-z");
    assert_eq!(text(&area, cx), "один два");
    cx.simulate_keystrokes("ctrl-z");
    cx.simulate_input("!");
    cx.simulate_keystrokes("ctrl-y");
    assert_eq!(text(&area, cx), "один !");
}

#[gpui::test]
fn arrows_move_between_lines_and_keep_the_column(cx: &mut TestAppContext) {
    let (area, cx) = open(cx, "abcdef\nab\nabcdef");
    cx.simulate_keystrokes("right right right right down");
    cx.simulate_input("X");
    assert_eq!(text(&area, cx), "abcdef\nabX\nabcdef");
    cx.simulate_keystrokes("left down");
    cx.simulate_input("Y");
    assert_eq!(text(&area, cx), "abcdef\nabX\nabYcdef");
    cx.simulate_keystrokes("down");
    cx.simulate_input("Z");
    assert_eq!(text(&area, cx), "abcdef\nabX\nabYcdefZ");
    cx.simulate_keystrokes("ctrl-home up");
    cx.simulate_input("W");
    assert_eq!(text(&area, cx), "Wabcdef\nabX\nabYcdefZ");
    cx.simulate_keystrokes("ctrl-home right right right right down down");
    cx.simulate_input("Q");
    assert_eq!(text(&area, cx), "Wabcdef\nabX\nabYcQdefZ");
}

#[gpui::test]
fn shift_selection_is_copied_cut_and_replaced_by_paste(cx: &mut TestAppContext) {
    let (area, cx) = open(cx, "первая\nвторая");
    cx.simulate_keystrokes("shift-end");
    assert_eq!(selected(&area, cx), "первая");
    cx.simulate_keystrokes("shift-down ctrl-c");
    assert_eq!(cx.read_from_clipboard().and_then(|item| item.text()).as_deref(), Some("первая\nвторая"));
    cx.simulate_keystrokes("ctrl-x");
    assert_eq!(text(&area, cx), "");
    cx.write_to_clipboard(ClipboardItem::new_string("раз\r\nдва".into()));
    cx.simulate_keystrokes("ctrl-v");
    assert_eq!(text(&area, cx), "раз\nдва");
    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_input("всё");
    assert_eq!(text(&area, cx), "всё");
}

#[gpui::test]
fn clicks_place_the_cursor_and_double_click_selects_a_word(cx: &mut TestAppContext) {
    let (area, cx) = open(cx, "один два\nтри");
    cx.simulate_click(point(px(200.), px(60.)), Modifiers::default());
    cx.simulate_input("!");
    assert_eq!(text(&area, cx), "один два\nтри!");
    let word = point(px(16.), px(17.));
    cx.simulate_event(gpui::MouseDownEvent {
        button: MouseButton::Left,
        position: word,
        modifiers: Modifiers::default(),
        click_count: 2,
        first_mouse: false,
    });
    cx.simulate_event(gpui::MouseUpEvent {
        button: MouseButton::Left,
        position: word,
        modifiers: Modifiers::default(),
        click_count: 2,
    });
    assert_eq!(selected(&area, cx), "один");
}

fn id(raw: &str) -> crate::store::ItemId {
    serde_json::from_value(serde_json::Value::String(raw.to_owned())).expect("an id is a plain string")
}

fn token(raw: &str, name: &str) -> String {
    crate::store::mention::token(&id(raw), name)
}

#[derive(Debug, PartialEq)]
enum Seen {
    Asked(Option<String>),
    Step(isize),
    Choose,
    Hover(Option<String>),
    Open(String, bool),
    Press(String),
    Paste,
}

fn watch(area: &Entity<TextArea>, cx: &mut VisualTestContext) -> Rc<std::cell::RefCell<Vec<Seen>>> {
    let seen = Rc::new(std::cell::RefCell::new(Vec::new()));
    let log = seen.clone();
    cx.update(|_, cx| {
        cx.subscribe(area, move |_, event: &super::AreaEvent, _| {
            log.borrow_mut().push(match event {
                super::AreaEvent::Suggest(query) => Seen::Asked(query.clone()),
                super::AreaEvent::Anchored => return,
                super::AreaEvent::Step(delta) => Seen::Step(*delta),
                super::AreaEvent::Choose => Seen::Choose,
                super::AreaEvent::Hover(peek) => Seen::Hover(peek.as_ref().map(|peek| peek.id.to_string())),
                super::AreaEvent::Open { id, reveal } => Seen::Open(id.to_string(), *reveal),
                super::AreaEvent::Press { id, .. } => Seen::Press(id.to_string()),
                super::AreaEvent::Paste => Seen::Paste,
            })
        })
        .detach()
    });
    seen
}

#[gpui::test]
fn a_mention_moves_and_erases_as_one_character(cx: &mut TestAppContext) {
    let mention = token("5", "отчёт.pdf");
    let (area, cx) = open(cx, &format!("a {mention} b"));
    let seen = watch(&area, cx);
    assert_eq!(text(&area, cx), format!("a {mention} b"));
    cx.simulate_keystrokes("ctrl-home right right right");
    cx.simulate_input("X");
    assert_eq!(text(&area, cx), format!("a {mention}X b"));
    cx.simulate_keystrokes("backspace backspace");
    assert_eq!(text(&area, cx), "a \\ b", "backspace turns the mention back into the trigger");
    assert_eq!(seen.borrow().last(), Some(&Seen::Asked(Some(String::new()))), "and opens the list again");
    cx.simulate_keystrokes("backspace");
    assert_eq!(text(&area, cx), "a  b");
    cx.simulate_keystrokes("ctrl-z ctrl-z");
    assert_eq!(text(&area, cx), format!("a {mention} b"));
}

#[gpui::test]
fn at_sign_asks_for_a_card_and_choosing_puts_a_mention_in_its_place(cx: &mut TestAppContext) {
    let (area, cx) = open(cx, "");
    let seen = watch(&area, cx);
    cx.simulate_input("ivan@mail см. @отч");
    assert_eq!(seen.borrow().last(), Some(&Seen::Asked(Some("отч".into()))));
    assert!(!seen.borrow().contains(&Seen::Asked(Some("mail".into()))), "an address is not a mention");
    area.update(cx, |area, _| area.set_suggesting(true));
    cx.simulate_keystrokes("down alt-j alt-k up enter");
    assert_eq!(text(&area, cx), "ivan@mail см. @отч", "keys go to the list while it is open");
    assert!(seen.borrow().ends_with(&[Seen::Step(1), Seen::Step(1), Seen::Step(-1), Seen::Step(-1), Seen::Choose]));
    area.update(cx, |area, cx| area.choose(&id("5"), "отчёт.pdf", cx));
    assert_eq!(text(&area, cx), format!("ivan@mail см. {} ", token("5", "отчёт.pdf")));
    assert_eq!(seen.borrow().last(), Some(&Seen::Asked(None)));
}

#[gpui::test]
fn escape_closes_the_list_and_keeps_what_was_typed(cx: &mut TestAppContext) {
    let (area, cx) = open(cx, "");
    let seen = watch(&area, cx);
    cx.simulate_input("@ab");
    area.update(cx, |area, _| area.set_suggesting(true));
    cx.simulate_keystrokes("escape");
    assert_eq!(seen.borrow().last(), Some(&Seen::Asked(None)));
    area.update(cx, |area, _| area.set_suggesting(false));
    cx.simulate_input("c");
    assert_eq!(text(&area, cx), "@abc");
    assert_eq!(seen.borrow().last(), Some(&Seen::Asked(None)), "the same @ does not ask again");
    cx.simulate_input(" @");
    assert_eq!(seen.borrow().last(), Some(&Seen::Asked(Some(String::new()))));
}

#[gpui::test]
fn copied_mentions_paste_back_as_mentions_and_read_as_names_elsewhere(cx: &mut TestAppContext) {
    let mention = token("5", "отчёт.pdf");
    let (area, cx) = open(cx, &format!("x {mention} y"));
    cx.simulate_keystrokes("ctrl-a ctrl-c");
    let copied = cx.update(|_, cx| cx.read_from_clipboard()).and_then(|item| item.text());
    assert_eq!(copied.as_deref(), Some("x @отчёт.pdf y"));
    cx.simulate_keystrokes("ctrl-end ctrl-v");
    assert_eq!(text(&area, cx), format!("x {mention} yx {mention} y"));
    cx.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string("a\u{E000}b".into())));
    cx.simulate_keystrokes("ctrl-v");
    assert!(text(&area, cx).ends_with(" yab"));
}

#[gpui::test]
fn clipboard_without_text_is_left_to_the_shelf(cx: &mut TestAppContext) {
    let (area, cx) = open(cx, "");
    let seen = watch(&area, cx);
    let image = gpui::Image::from_bytes(gpui::ImageFormat::Png, vec![1, 2, 3]);
    cx.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_image(&image)));
    cx.simulate_keystrokes("ctrl-v");
    assert_eq!(seen.borrow().last(), Some(&Seen::Paste));
}

#[gpui::test]
fn clicking_a_mention_opens_it_instead_of_moving_the_caret(cx: &mut TestAppContext) {
    let (area, cx) = open(cx, &format!("see {} now", token("5", "отчёт.pdf")));
    let seen = watch(&area, cx);
    cx.run_until_parked();
    let chip =
        cx.update(|_, cx| area.read(cx).layout.as_ref().and_then(|layout| layout.chips.first()).map(|c| c.bounds));
    let center = chip.expect("the mention is laid out").center();
    let cursor = cx.update(|_, cx| area.read(cx).cursor());
    cx.simulate_mouse_move(center, None, Modifiers::none());
    assert!(seen.borrow().contains(&Seen::Hover(Some("5".into()))));
    cx.simulate_click(center, Modifiers::none());
    assert!(seen.borrow().ends_with(&[Seen::Press("5".into()), Seen::Open("5".into(), false)]));
    assert_eq!(cx.update(|_, cx| area.read(cx).cursor()), cursor);
    cx.simulate_click(center, Modifiers::control());
    assert_eq!(seen.borrow().last(), Some(&Seen::Open("5".into(), true)));
    let left = point(center.x, center.y + px(200.));
    cx.simulate_mouse_move(left, None, Modifiers::none());
    assert_eq!(seen.borrow().last(), Some(&Seen::Hover(None)));
}

#[gpui::test]
fn a_mention_at_the_start_of_a_line_lines_up_with_the_text_and_stays_on_its_line(cx: &mut TestAppContext) {
    let (area, cx) = open(cx, &format!("строка\n{}", token("5", "отчёт.pdf")));
    cx.run_until_parked();
    let (chip, origin, line) = cx.update(|_, cx| {
        let layout = area.read(cx).layout.as_ref().expect("laid out");
        (layout.chips.first().map(|chip| chip.bounds).expect("the mention"), layout.origin(), layout.line_height())
    });
    assert_eq!(chip.left(), origin.x, "the chip starts where the text starts");
    assert!(chip.top() >= origin.y + line, "{chip:?} stays below the first line");
    assert!(chip.size.height <= line, "{chip:?} fits its line of {line:?}");
}
