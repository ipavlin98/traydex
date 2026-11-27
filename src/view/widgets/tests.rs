use super::{HINT_PADDING, HINT_WIDTH, Hint, Tip, hint, optical_lift, searched};
use crate::search::Query;
use crate::shelf::Regions;
use crate::theme::{Theme, ThemeMode};
use gpui::{
    Context, IntoElement, Modifiers, Render, TestAppContext, VisualTestContext, Window, WindowAppearance, div, point,
    prelude::*, px,
};
use std::time::Duration;

const LONG: &str =
    "Покажет QR-код: наведите камеру телефона, и файл откроется по Wi‑Fi, даже если строка очень длинная";

struct Anchor {
    theme: Theme,
}

impl Render for Anchor {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(div().id("anchor").size(px(100.)).tooltip(hint(self.theme, LONG)))
    }
}

fn shown(cx: &mut TestAppContext) -> &mut VisualTestContext {
    let theme = Theme::resolve(ThemeMode::Dark, WindowAppearance::Dark);
    let (_, cx) = cx.add_window_view(move |_, _| Anchor { theme });
    cx.run_until_parked();
    cx.simulate_mouse_move(point(px(20.), px(30.)), None, Modifiers::default());
    cx.executor().advance_clock(Duration::from_millis(600));
    cx.run_until_parked();
    cx
}

#[gpui::test]
fn wrapped_tooltip_hugs_its_longest_line(cx: &mut TestAppContext) {
    let cx = shown(cx);
    let frame = cx.debug_bounds("hint").expect("tooltip");
    let rows: Vec<_> =
        (0..).map_while(|n| cx.debug_bounds(Box::leak(format!("hint:0:{n}").into_boxed_str()))).collect();
    assert!(rows.len() >= 2, "{rows:?}");
    let right = rows.iter().map(|row| f32::from(row.right())).fold(0., f32::max);
    let gap = f32::from(frame.right()) - right;
    assert!(gap <= HINT_PADDING + 2.5, "text ends at {right}, frame {frame:?} leaves {gap}px");
}

#[gpui::test]
fn shown_tooltip_grows_to_fit_wrapped_lines(cx: &mut TestAppContext) {
    let cx = shown(cx);
    let frame = cx.debug_bounds("hint").expect("tooltip");
    assert!(f32::from(frame.size.width) <= HINT_WIDTH + 0.5, "{frame:?}");
    let line = cx.debug_bounds("hint:0").expect("line");
    assert!(f32::from(line.size.height) >= 24., "wrapped text needs two lines, got {line:?}");
    assert!(line.bottom() <= frame.bottom(), "{line:?} leaves {frame:?}");
}

#[gpui::test]
fn long_hint_lines_wrap_inside_its_box(cx: &mut TestAppContext) {
    let theme = Theme::resolve(ThemeMode::Dark, WindowAppearance::Dark);
    let text = LONG;
    let (_, cx) = cx.add_window_view(move |_, _| Hint {
        tip: Tip::keys(text, &["Ctrl", "C"]).line(Tip::keys(text, &["Ctrl", "Enter"])),
        theme,
    });
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    let frame = cx.debug_bounds("hint").expect("hint");
    assert!(f32::from(frame.size.width) <= HINT_WIDTH + 0.5, "{frame:?}");
    let first = cx.debug_bounds("hint:0").expect("first line");
    for part in ["hint:0", "hint:1"] {
        let bounds = cx.debug_bounds(part).expect(part);
        let inside =
            bounds.left() >= frame.left() && bounds.right() <= frame.right() && bounds.bottom() <= frame.bottom();
        assert!(inside, "{part} {bounds:?} leaves {frame:?}");
    }
    let second = cx.debug_bounds("hint:1").expect("second line");
    assert!(second.top() >= first.bottom(), "second line {second:?} overlaps the first {first:?}");
}

#[test]
fn page_search_keeps_only_the_best_grade_of_matches_on_the_page() {
    let regions = Regions::default();
    let entries = [
        ("В PDF", "Склеивает картинки и текстовые файлы в один PDF"),
        ("Распознать текст", "Копирует текст со скриншотов, фото и сканов PDF"),
    ];
    let shown = |text: &str| {
        let query = Query::parse(text);
        searched(query.as_ref(), &regions, |finder| {
            entries.map(|(title, detail)| finder.locate(title, detail).is_some())
        })
    };
    assert_eq!(shown("в pdf"), [true, false], "a stray letter inside a word loses to a word match");
    assert_eq!(shown("текст"), [true, true]);
    assert_eq!(shown("канов"), [false, true], "with no word match, fragments are found");
    assert_eq!(shown("канов зеро"), [false, false]);
}

#[test]
fn text_block_is_centered_between_the_capitals_and_the_last_baseline() {
    assert_eq!(optical_lift(4., 30., 32.), 1., "ink sits low in its line boxes, so the block moves up");
    assert_eq!(optical_lift(2., 26., 32.), -2., "ink sits high, so the block moves down");
    assert_eq!(optical_lift(3., 29., 32.), 0., "ink already centered");
}
