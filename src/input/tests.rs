use super::{Changed, Submitted, TextInput, caret_bounds, pixel_bounds};
use gpui::{
    Bounds, ClipboardItem, Entity, EntityInputHandler, Modifiers, MouseButton, MouseDownEvent, MouseUpEvent, Pixels,
    TestAppContext, VisualTestContext, point, px,
};
use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;

struct Events {
    changed: Rc<RefCell<usize>>,
    submitted: Rc<RefCell<usize>>,
}

fn open<'a>(
    cx: &'a mut TestAppContext,
    text: &str,
    secret: bool,
) -> (Entity<TextInput>, &'a mut VisualTestContext, Events) {
    let text = text.to_string();
    let (input, cx) = cx.add_window_view(move |_, cx| TextInput::new(text, "Placeholder", secret, cx));
    let events = Events { changed: Rc::default(), submitted: Rc::default() };
    let (changed, submitted) = (events.changed.clone(), events.submitted.clone());
    cx.update(|window, cx| {
        cx.subscribe(&input, move |_, _: &Changed, _| *changed.borrow_mut() += 1).detach();
        cx.subscribe(&input, move |_, _: &Submitted, _| *submitted.borrow_mut() += 1).detach();
        input.read(cx).focus(window);
    });
    cx.run_until_parked();
    (input, cx, events)
}

fn text(input: &Entity<TextInput>, cx: &mut VisualTestContext) -> String {
    cx.update(|_, cx| input.read(cx).text().to_string())
}

fn selection(input: &Entity<TextInput>, cx: &mut VisualTestContext) -> (std::ops::Range<usize>, bool) {
    input.update_in(cx, |input, window, cx| {
        let selection = input.selected_text_range(false, window, cx).expect("selection");
        (selection.range, selection.reversed)
    })
}

#[gpui::test]
fn starts_with_cursor_at_end_and_focus(cx: &mut TestAppContext) {
    let (input, cx, _) = open(cx, "hello", false);
    assert_eq!(selection(&input, cx), (5..5, false));
    assert!(cx.update(|window, cx| input.read(cx).focused(window)));
}

#[gpui::test]
fn typing_inserts_at_cursor_and_emits_changes(cx: &mut TestAppContext) {
    let (input, cx, events) = open(cx, "", false);
    cx.simulate_input("abc");
    assert_eq!(text(&input, cx), "abc");
    assert_eq!(*events.changed.borrow(), 3);
    cx.simulate_keystrokes("left left");
    cx.simulate_input("X");
    assert_eq!(text(&input, cx), "aXbc");
    assert_eq!(selection(&input, cx), (2..2, false));
}

#[gpui::test]
fn backspace_and_delete_remove_whole_characters(cx: &mut TestAppContext) {
    let (input, cx, _) = open(cx, "a😀бc", false);
    cx.simulate_keystrokes("left backspace");
    assert_eq!(text(&input, cx), "a😀c");
    cx.simulate_keystrokes("home delete");
    assert_eq!(text(&input, cx), "😀c");
    cx.simulate_keystrokes("delete");
    assert_eq!(text(&input, cx), "c");
    assert_eq!(selection(&input, cx), (0..0, false));
}

#[gpui::test]
fn editing_at_boundaries_is_harmless(cx: &mut TestAppContext) {
    let (input, cx, _) = open(cx, "ab", false);
    cx.simulate_keystrokes("end delete right");
    assert_eq!(selection(&input, cx), (2..2, false));
    cx.simulate_keystrokes("home backspace left");
    assert_eq!(text(&input, cx), "ab");
    assert_eq!(selection(&input, cx), (0..0, false));
}

#[gpui::test]
fn shift_arrows_extend_selection_in_both_directions(cx: &mut TestAppContext) {
    let (input, cx, _) = open(cx, "hello", false);
    cx.simulate_keystrokes("shift-left shift-left");
    assert_eq!(selection(&input, cx), (3..5, true));
    cx.simulate_keystrokes("shift-right shift-right shift-right");
    assert_eq!(selection(&input, cx).0, 5..5);
    cx.simulate_keystrokes("home shift-right shift-right");
    assert_eq!(selection(&input, cx), (0..2, false));
    cx.simulate_keystrokes("shift-end");
    assert_eq!(selection(&input, cx), (0..5, false));
    cx.simulate_keystrokes("shift-home");
    assert_eq!(selection(&input, cx), (0..0, false));
}

#[gpui::test]
fn arrows_collapse_selection_to_its_edge(cx: &mut TestAppContext) {
    let (input, cx, _) = open(cx, "hello", false);
    cx.simulate_keystrokes("ctrl-a left");
    assert_eq!(selection(&input, cx), (0..0, false));
    cx.simulate_keystrokes("ctrl-a right");
    assert_eq!(selection(&input, cx), (5..5, false));
}

#[gpui::test]
fn ctrl_arrows_and_ctrl_erase_work_by_words(cx: &mut TestAppContext) {
    let (input, cx, _) = open(cx, "привет мир тест", false);
    cx.simulate_keystrokes("ctrl-left");
    assert_eq!(selection(&input, cx).0, 11..11, "the cursor lands at the start of the last word");
    cx.simulate_keystrokes("ctrl-backspace");
    assert_eq!(text(&input, cx), "привет тест");
    cx.simulate_keystrokes("home ctrl-right");
    assert_eq!(selection(&input, cx).0, 6..6, "the cursor lands at the end of the first word");
    cx.simulate_keystrokes("ctrl-shift-right");
    assert_eq!(selection(&input, cx).0, 6..11);
}

#[gpui::test]
fn secret_field_moves_by_characters_even_with_ctrl(cx: &mut TestAppContext) {
    let (input, cx, _) = open(cx, "ab cd", true);
    cx.simulate_keystrokes("ctrl-left");
    assert_eq!(selection(&input, cx).0, 4..4, "word boundaries of a password stay hidden");
    cx.simulate_keystrokes("ctrl-backspace");
    assert_eq!(text(&input, cx), "ab d");
}

#[gpui::test]
fn typing_replaces_selection(cx: &mut TestAppContext) {
    let (input, cx, _) = open(cx, "hello world", false);
    cx.simulate_keystrokes("shift-left shift-left shift-left shift-left shift-left");
    cx.simulate_input("there");
    assert_eq!(text(&input, cx), "hello there");
    cx.simulate_keystrokes("ctrl-a backspace");
    assert_eq!(text(&input, cx), "");
}

#[gpui::test]
fn selection_offsets_are_reported_in_utf16(cx: &mut TestAppContext) {
    let (input, cx, _) = open(cx, "😀б", false);
    assert_eq!(selection(&input, cx), (3..3, false));
    cx.simulate_keystrokes("shift-left");
    assert_eq!(selection(&input, cx), (2..3, true));
    let fragment = input.update_in(cx, |input, window, cx| {
        let mut adjusted = None;
        let text = input.text_for_range(1..3, &mut adjusted, window, cx);
        (text, adjusted)
    });
    assert_eq!(fragment, (Some("б".into()), Some(2..3)));
}

#[gpui::test]
fn clipboard_copy_cut_and_paste(cx: &mut TestAppContext) {
    let (input, cx, _) = open(cx, "hello world", false);
    cx.simulate_keystrokes("ctrl-a ctrl-c");
    assert_eq!(cx.read_from_clipboard().and_then(|item| item.text()), Some("hello world".into()));
    cx.simulate_keystrokes("ctrl-a ctrl-x");
    assert_eq!(text(&input, cx), "");
    cx.write_to_clipboard(ClipboardItem::new_string("  pasted\n".into()));
    cx.simulate_keystrokes("ctrl-v");
    assert_eq!(text(&input, cx), "pasted");
}

#[gpui::test]
fn copy_without_selection_keeps_clipboard(cx: &mut TestAppContext) {
    let (_, cx, _) = open(cx, "hello", false);
    cx.write_to_clipboard(ClipboardItem::new_string("previous".into()));
    cx.simulate_keystrokes("ctrl-c ctrl-x");
    assert_eq!(cx.read_from_clipboard().and_then(|item| item.text()), Some("previous".into()));
}

#[gpui::test]
fn secret_text_is_never_copied(cx: &mut TestAppContext) {
    let (input, cx, _) = open(cx, "s3cret", true);
    cx.write_to_clipboard(ClipboardItem::new_string("previous".into()));
    cx.simulate_keystrokes("ctrl-a ctrl-c ctrl-x");
    assert_eq!(cx.read_from_clipboard().and_then(|item| item.text()), Some("previous".into()));
    assert_eq!(text(&input, cx), "s3cret");
}

#[gpui::test]
fn control_characters_are_stripped_from_input(cx: &mut TestAppContext) {
    let (input, cx, _) = open(cx, "", false);
    input.update_in(cx, |input, window, cx| input.replace_text_in_range(None, "a\tb\r\nc\u{7}", window, cx));
    assert_eq!(text(&input, cx), "abc");
}

#[gpui::test]
fn enter_submits_unless_composing(cx: &mut TestAppContext) {
    let (input, cx, events) = open(cx, "", false);
    cx.simulate_keystrokes("enter");
    assert_eq!(*events.submitted.borrow(), 1);
    input.update_in(cx, |input, window, cx| input.replace_and_mark_text_in_range(None, "に", None, window, cx));
    cx.simulate_keystrokes("enter");
    assert_eq!(*events.submitted.borrow(), 1);
    input.update_in(cx, |input, window, cx| input.unmark_text(window, cx));
    cx.simulate_keystrokes("enter");
    assert_eq!(*events.submitted.borrow(), 2);
}

#[gpui::test]
fn composition_replaces_marked_range(cx: &mut TestAppContext) {
    let (input, cx, _) = open(cx, "ab", false);
    let marked = input.update_in(cx, |input, window, cx| {
        input.replace_and_mark_text_in_range(None, "かな", Some(1..1), window, cx);
        input.marked_text_range(window, cx)
    });
    assert_eq!(marked, Some(2..4));
    assert_eq!(selection(&input, cx), (3..3, false));
    input.update_in(cx, |input, window, cx| input.replace_text_in_range(None, "仮名", window, cx));
    assert_eq!(text(&input, cx), "ab仮名");
    let marked = input.update_in(cx, |input, window, cx| input.marked_text_range(window, cx));
    assert_eq!(marked, None);
}

#[gpui::test]
fn empty_composition_clears_marked_text(cx: &mut TestAppContext) {
    let (input, cx, _) = open(cx, "ab", false);
    let marked = input.update_in(cx, |input, window, cx| {
        input.replace_and_mark_text_in_range(None, "x", None, window, cx);
        input.replace_and_mark_text_in_range(None, "", None, window, cx);
        input.marked_text_range(window, cx)
    });
    assert_eq!(marked, None);
    assert_eq!(text(&input, cx), "ab");
}

#[gpui::test]
fn explicit_utf16_range_replaces_exact_characters(cx: &mut TestAppContext) {
    let (input, cx, _) = open(cx, "a😀b", false);
    input.update_in(cx, |input, window, cx| input.replace_text_in_range(Some(1..3), "-", window, cx));
    assert_eq!(text(&input, cx), "a-b");
    assert_eq!(selection(&input, cx), (2..2, false));
    input.update_in(cx, |input, window, cx| {
        input.replace_text_in_range(Some(Range { start: 3, end: 1 }), "+", window, cx)
    });
    assert_eq!(text(&input, cx), "a+");
}

#[gpui::test]
fn set_text_resets_selection_without_change_event(cx: &mut TestAppContext) {
    let (input, cx, events) = open(cx, "hello", false);
    cx.simulate_keystrokes("ctrl-a");
    input.update(cx, |input, cx| input.set_text("bye".into(), cx));
    assert_eq!(text(&input, cx), "bye");
    assert_eq!(selection(&input, cx), (3..3, false));
    assert_eq!(*events.changed.borrow(), 0);
}

#[gpui::test]
fn unhandled_keys_propagate(cx: &mut TestAppContext) {
    let (input, cx, events) = open(cx, "hello", false);
    cx.simulate_keystrokes("up down pageup f5");
    assert_eq!(text(&input, cx), "hello");
    assert_eq!(selection(&input, cx), (5..5, false));
    assert_eq!(*events.changed.borrow(), 0);
}

#[gpui::test]
fn double_click_selects_a_word_and_triple_click_everything(cx: &mut TestAppContext) {
    let (input, cx, _) = open(cx, "один два", false);
    let at = point(px(12.), px(14.));
    for clicks in [2, 3] {
        let modifiers = Modifiers::default();
        cx.simulate_event(MouseDownEvent {
            button: MouseButton::Left,
            position: at,
            modifiers,
            click_count: clicks,
            first_mouse: false,
        });
        cx.simulate_event(MouseUpEvent { button: MouseButton::Left, position: at, modifiers, click_count: clicks });
        let expected = if clicks == 2 { 0..4 } else { 0..8 };
        assert_eq!(selection(&input, cx), (expected, false));
    }
}

#[gpui::test]
fn losing_focus_scrolls_a_long_text_back_to_its_start(cx: &mut TestAppContext) {
    let (input, cx, _) = open(cx, &"слово ".repeat(300), false);
    assert!(cx.update(|_, cx| input.read(cx).scroll) > px(0.), "the focused input shows the caret at the end");
    cx.update(|window, _| window.blur());
    cx.run_until_parked();
    assert_eq!(cx.update(|_, cx| input.read(cx).scroll), px(0.));
}

const SCALES: [f32; 6] = [1., 1.1, 1.25, 1.5, 1.75, 2.];

fn on_grid(bounds: Bounds<Pixels>, scale: f32) -> bool {
    [bounds.left(), bounds.top(), bounds.right(), bounds.bottom()].into_iter().all(|edge| {
        let device = f32::from(edge) * scale;
        (device - device.round()).abs() < 1e-3
    })
}

#[test]
fn caret_is_a_crisp_line_of_the_same_width_anywhere() {
    for scale in SCALES {
        let widths: Vec<f32> = [3.1, 40.55, 41.37, 200.9]
            .into_iter()
            .map(|x| {
                let bounds = caret_bounds(point(px(x), px(7.3)), px(14.), scale);
                assert!(on_grid(bounds, scale), "caret {bounds:?} at {scale} is off the pixel grid");
                (f32::from(bounds.size.width) * scale).round()
            })
            .collect();
        assert!(widths.iter().all(|width| *width >= 1. && *width == widths[0]), "{widths:?} at {scale}");
    }
}

#[test]
fn selection_rows_meet_without_gaps_or_overlaps() {
    for scale in SCALES {
        let row = |index: f32| {
            let top = px(5.3) + px(20.) * index;
            pixel_bounds(Bounds::from_corners(point(px(10.4), top), point(px(77.7), top + px(20.))), scale)
        };
        for index in 0..5 {
            let (upper, lower) = (row(index as f32), row(index as f32 + 1.));
            assert!(on_grid(upper, scale), "{upper:?} at {scale}");
            assert_eq!(upper.bottom(), lower.top(), "rows {index} and {} at {scale}", index + 1);
        }
    }
}
