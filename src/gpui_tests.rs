use gpui::{
    AbsoluteLength, AnyView, AppContext, Bounds, Context, DevicePixels, Entity, Hitbox, HitboxBehavior, HoverFade,
    HoverTransition, Hsla, InteractiveElement, IntoElement, Modifiers, MouseButton, MouseExitEvent, ParentElement,
    Pixels, Point, Render, SharedString, StatefulInteractiveElement, Style, Styled, TestAppContext, TextRun,
    Visibility, VisualTestContext, Window, blend_hover, canvas, div, font, point, prelude::FluentBuilder, px, relative,
    size,
};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

type Hitboxes = Rc<RefCell<Vec<Hitbox>>>;

struct Boxes {
    bounds: Vec<Bounds<Pixels>>,
    inserted: Hitboxes,
}

impl Render for Boxes {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let bounds = self.bounds.clone();
        let inserted = self.inserted.clone();
        div().size_full().child(
            canvas(
                move |_, window, _| {
                    bounds
                        .into_iter()
                        .map(|bounds| window.insert_hitbox(bounds, HitboxBehavior::Normal))
                        .collect::<Vec<_>>()
                },
                move |_, hitboxes, _, _| *inserted.borrow_mut() = hitboxes,
            )
            .size_full(),
        )
    }
}

fn boxes(cx: &mut TestAppContext, bounds: Vec<Bounds<Pixels>>) -> (Hitboxes, &mut VisualTestContext) {
    let inserted = Hitboxes::default();
    let shared = inserted.clone();
    let (_, cx) = cx.add_window_view(move |_, _| Boxes { bounds, inserted: shared });
    cx.run_until_parked();
    (inserted, cx)
}

fn hovered(hitboxes: &Hitboxes, cx: &mut VisualTestContext) -> Vec<bool> {
    cx.update(|window, _| hitboxes.borrow().iter().map(|hitbox| hitbox.is_hovered(window)).collect())
}

fn rect(x: f32, y: f32, width: f32, height: f32) -> Bounds<Pixels> {
    Bounds::new(point(px(x), px(y)), size(px(width), px(height)))
}

fn exit(cx: &mut VisualTestContext, position: Point<Pixels>) {
    cx.simulate_event(MouseExitEvent { position, pressed_button: None, modifiers: Modifiers::default() });
}

#[gpui::test]
fn hit_test_follows_device_pixels_at_fractional_edges(cx: &mut TestAppContext) {
    let (hitboxes, cx) = boxes(cx, vec![rect(0., 0., 10.3, 10.), rect(10.3, 0., 10., 10.)]);
    assert_eq!(cx.update(|window, _| window.scale_factor()), 2.);
    cx.simulate_mouse_move(point(px(10.2), px(5.)), None, Modifiers::default());
    assert_eq!(hovered(&hitboxes, cx), [true, false]);
    cx.simulate_mouse_move(point(px(10.26), px(5.)), None, Modifiers::default());
    assert_eq!(hovered(&hitboxes, cx), [false, true]);
    cx.simulate_mouse_move(point(px(20.24), px(5.)), None, Modifiers::default());
    assert_eq!(hovered(&hitboxes, cx), [false, true]);
    cx.simulate_mouse_move(point(px(20.26), px(5.)), None, Modifiers::default());
    assert_eq!(hovered(&hitboxes, cx), [false, false]);
    cx.simulate_mouse_move(point(px(5.), px(9.76)), None, Modifiers::default());
    assert_eq!(hovered(&hitboxes, cx), [false, false]);
}

#[gpui::test]
fn pointer_leaving_the_window_clears_hover(cx: &mut TestAppContext) {
    let (hitboxes, cx) = boxes(cx, vec![rect(0., 0., 100., 100.)]);
    cx.simulate_mouse_move(point(px(50.), px(50.)), None, Modifiers::default());
    assert_eq!(hovered(&hitboxes, cx), [true]);
    exit(cx, point(px(50.), px(50.)));
    assert_eq!(hovered(&hitboxes, cx), [false]);
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    assert_eq!(hovered(&hitboxes, cx), [false]);
    cx.simulate_mouse_move(point(px(60.), px(60.)), None, Modifiers::default());
    assert_eq!(hovered(&hitboxes, cx), [true]);
}

#[derive(Default)]
struct Probe {
    built: Cell<usize>,
    origin: Cell<Option<Point<Pixels>>>,
}

impl Probe {
    fn shown(&self, cx: &mut VisualTestContext) -> bool {
        self.origin.set(None);
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
        self.origin.get().is_some()
    }
}

struct Tip {
    probe: Rc<Probe>,
}

impl Render for Tip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let probe = self.probe.clone();
        div()
            .size(px(40.))
            .child(canvas(move |bounds, _, _| probe.origin.set(Some(bounds.origin)), |_, _, _, _| {}).size_full())
    }
}

struct Source {
    probe: Rc<Probe>,
    covered: bool,
    shifted: bool,
}

impl Render for Source {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let probe = self.probe.clone();
        let left = if self.shifted { 300. } else { 0. };
        div()
            .size_full()
            .relative()
            .child(div().id("source").absolute().left(px(left)).top(px(0.)).size(px(100.)).tooltip(move |_, cx| {
                probe.built.set(probe.built.get() + 1);
                let probe = probe.clone();
                AnyView::from(cx.new(|_| Tip { probe }))
            }))
            .children(self.covered.then(|| div().id("cover").absolute().size(px(200.)).occlude()))
    }
}

fn source(cx: &mut TestAppContext) -> (Entity<Source>, Rc<Probe>, &mut VisualTestContext) {
    let probe = Rc::new(Probe::default());
    let shared = probe.clone();
    let (view, cx) = cx.add_window_view(move |_, _| Source { probe: shared, covered: false, shifted: false });
    cx.run_until_parked();
    (view, probe, cx)
}

fn show_tooltip(cx: &mut VisualTestContext, position: Point<Pixels>) {
    cx.simulate_mouse_move(position, None, Modifiers::default());
    cx.executor().advance_clock(Duration::from_millis(600));
    cx.run_until_parked();
}

#[gpui::test]
fn tooltip_opens_below_the_cursor(cx: &mut TestAppContext) {
    let (_, probe, cx) = source(cx);
    show_tooltip(cx, point(px(20.), px(30.)));
    assert!(probe.shown(cx));
    assert_eq!(probe.built.get(), 1);
    assert_eq!(probe.origin.get(), Some(point(px(20.), px(52.))));
}

#[gpui::test]
fn tooltip_keeps_a_safe_margin_from_the_left_edge(cx: &mut TestAppContext) {
    let (_, probe, cx) = source(cx);
    show_tooltip(cx, point(px(5.), px(30.)));
    assert!(probe.shown(cx));
    assert_eq!(probe.origin.get(), Some(point(px(15.), px(52.))));
}

fn tooltip_left_in_narrow_window(cx: &mut TestAppContext, x: f32) -> Option<Pixels> {
    let (_, probe, cx) = source(cx);
    cx.simulate_resize(size(px(100.), px(100.)));
    cx.run_until_parked();
    show_tooltip(cx, point(px(x), px(20.)));
    assert!(probe.shown(cx));
    probe.origin.get().map(|origin| origin.x)
}

#[gpui::test]
fn tooltip_keeps_a_safe_margin_from_the_right_edge(cx: &mut TestAppContext) {
    assert_eq!(tooltip_left_in_narrow_window(cx, 50.), Some(px(45.)));
    assert_eq!(tooltip_left_in_narrow_window(cx, 80.), Some(px(39.)));
}

#[gpui::test]
fn tooltip_hides_when_pointer_leaves_the_window(cx: &mut TestAppContext) {
    let (_, probe, cx) = source(cx);
    show_tooltip(cx, point(px(20.), px(30.)));
    assert!(probe.shown(cx));
    exit(cx, point(px(20.), px(30.)));
    cx.run_until_parked();
    assert!(!probe.shown(cx));
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
    assert_eq!(probe.built.get(), 1);
}

#[gpui::test]
fn tooltip_hides_when_its_element_gets_covered(cx: &mut TestAppContext) {
    let (view, probe, cx) = source(cx);
    show_tooltip(cx, point(px(20.), px(30.)));
    assert!(probe.shown(cx));
    view.update(cx, |source, cx| {
        source.covered = true;
        cx.notify();
    });
    cx.run_until_parked();
    assert!(!probe.shown(cx));
}

#[gpui::test]
fn tooltip_hides_when_its_element_moves_away(cx: &mut TestAppContext) {
    let (view, probe, cx) = source(cx);
    show_tooltip(cx, point(px(20.), px(30.)));
    assert!(probe.shown(cx));
    view.update(cx, |source, cx| {
        source.shifted = true;
        cx.notify();
    });
    cx.run_until_parked();
    assert!(!probe.shown(cx));
}

struct Selected {
    shown: bool,
}

impl Render for Selected {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().children(self.shown.then(|| div().debug_selector(|| "marked".into()).size(px(10.))))
    }
}

#[gpui::test]
fn debug_bounds_forget_elements_that_are_no_longer_drawn(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, _| Selected { shown: true });
    cx.run_until_parked();
    assert_eq!(cx.debug_bounds("marked"), Some(rect(0., 0., 10., 10.)));
    view.update(cx, |view, cx| {
        view.shown = false;
        cx.notify();
    });
    cx.run_until_parked();
    assert_eq!(cx.debug_bounds("marked"), None);
}

fn wrapped(cx: &mut VisualTestContext, text: &str, chars: f32) -> Vec<String> {
    cx.update(|window, _| {
        let run = TextRun {
            len: text.len(),
            font: font("Segoe UI"),
            color: Hsla::default(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let size = px(10.);
        let em = window.text_system().layout_line("m", size, &[TextRun { len: 1, ..run.clone() }], None).width;
        let lines = window
            .text_system()
            .shape_text(SharedString::from(text.to_string()), size, &[run], Some(em * chars + em / 2.), None)
            .expect("shaped");
        let mut rows = Vec::new();
        for line in &lines {
            let mut start = 0;
            let breaks = line.wrap_boundaries().iter().map(|wrap| line.runs()[wrap.run_ix].glyphs[wrap.glyph_ix].index);
            for end in breaks.chain([line.text.len()]) {
                rows.push(line.text[start..end].trim_end().to_string());
                start = end;
            }
        }
        rows
    })
}

#[gpui::test]
fn text_wraps_at_grammatical_line_breaks(cx: &mut TestAppContext) {
    let (_, cx) = boxes(cx, Vec::new());
    assert_eq!(wrapped(cx, "один (два) три", 9.), ["один", "(два) три"]);
    assert_eq!(wrapped(cx, "Перетащите в окно", 12.), ["Перетащите", "в окно"]);
    assert_eq!(wrapped(cx, "слово — дело", 8.), ["слово —", "дело"]);
    assert_eq!(wrapped(cx, "Скачано 40 МБ", 10.), ["Скачано", "40 МБ"]);
    assert_eq!(wrapped(cx, "फ़ाइलें यहाँ खींचें", 8.), ["फ़ाइलें", "यहाँ", "खींचें"]);
    assert_eq!(wrapped(cx, "你好，世界", 2.), ["你", "好，", "世界"]);
}

#[test]
fn hover_fade_reverses_from_where_it_was() {
    let duration = Duration::from_millis(160);
    let start = Instant::now() + Duration::from_secs(1);
    let mut fade = HoverFade::settled(false, start);
    let first = fade.advance(true, start, duration);
    assert!(first > 0., "the first frame already moves: {first}");
    let halfway = fade.advance(true, start + Duration::from_millis(64), duration);
    assert!(halfway > 0.5 && halfway < 1., "{halfway}");
    let turned = fade.advance(false, start + Duration::from_millis(64), duration);
    assert!(turned < halfway && turned > 0.5, "{halfway} → {turned}");
    let back = fade.advance(false, start + Duration::from_millis(96), duration);
    assert!(back < turned && back > 0., "{turned} → {back}");
    assert_eq!(fade.advance(false, start + Duration::from_millis(300), duration), 0.);
    assert_eq!(HoverFade::settled(true, start).advance(true, start, duration), 1.);
}

#[test]
fn hover_fades_out_as_quickly_as_it_fades_in() {
    let duration = Duration::from_millis(160);
    let start = Instant::now() + Duration::from_secs(1);
    let mut resting = Style::default();
    resting.text.color = Some(gpui::black());
    let mut hovered = resting.clone();
    hovered.text.color = Some(gpui::white());
    let lightness = |progress: f32| blend_hover(&resting, &hovered, progress).text.color.expect("text color").l;
    let mut entering = HoverFade::settled(false, start);
    entering.advance(true, start, duration);
    let mut leaving = HoverFade::settled(true, start);
    leaving.advance(false, start, duration);
    let later = start + Duration::from_millis(60);
    let shown = lightness(entering.advance(true, later, duration));
    let gone = 1. - lightness(leaving.advance(false, later, duration));
    assert!(shown > 0.5, "entering decelerates into the hovered style: {shown}");
    assert!((shown - gone).abs() < 1e-2, "leaving must not linger: shown {shown}, gone {gone}");
}

#[test]
fn hover_fade_keeps_animating_from_the_first_frame_of_a_change() {
    let duration = Duration::from_millis(160);
    let start = Instant::now() + Duration::from_secs(1);
    let mut entering = HoverFade::settled(false, start);
    assert!(!entering.animating(start, duration));
    assert!(entering.advance(true, start, duration) < 1.);
    assert!(entering.animating(start, duration));
    assert!(!entering.animating(start + duration, duration));
    let mut leaving = HoverFade::settled(true, start);
    assert!(leaving.advance(false, start, duration) > 0.);
    assert!(leaving.animating(start, duration));
    assert!(!leaving.animating(start + duration, duration));
}

#[test]
fn blended_hover_mixes_colors_and_fades_revealed_elements() {
    let mut resting = Style::default();
    resting.text.color = Some(gpui::black());
    resting.visibility = Visibility::Hidden;
    let mut hovered = resting.clone();
    hovered.text.color = Some(gpui::white());
    hovered.visibility = Visibility::Visible;
    let mixed = blend_hover(&resting, &hovered, 0.5);
    let lightness = mixed.text.color.expect("text color").l;
    assert!((lightness - 0.5).abs() < 1e-2, "{lightness}");
    assert_eq!(mixed.visibility, Visibility::Visible);
    let opacity = mixed.opacity.expect("partial opacity");
    assert!((opacity - 0.5).abs() < 1e-2, "{opacity}");
    assert_eq!(blend_hover(&resting, &hovered, 0.).visibility, Visibility::Hidden);
    assert_eq!(blend_hover(&resting, &hovered, 1.).text.color, Some(gpui::white()));
}

#[test]
fn blended_hover_keeps_the_hue_of_fully_transparent_colors() {
    let mut resting = Style::default();
    resting.text.color = Some(gpui::white().opacity(0.));
    let mut hovered = resting.clone();
    hovered.text.color = Some(gpui::hsla(0., 0., 0.5, 0.));
    let mixed = blend_hover(&resting, &hovered, 0.3).text.color.expect("text color");
    assert!(mixed.l > 0.5 && mixed.l < 1., "a transparent stop must not turn black: {mixed:?}");
}

#[test]
fn revealed_panels_with_a_background_switch_whole_instead_of_ghosting() {
    let resting =
        Style { background: Some(gpui::white().into()), visibility: Visibility::Hidden, ..Default::default() };
    let mut hovered = resting.clone();
    hovered.visibility = Visibility::Visible;
    for progress in [0.05, 0.1, 0.3, 0.6, 0.9] {
        let mixed = blend_hover(&resting, &hovered, progress);
        assert!(
            mixed.opacity.is_none_or(|opacity| opacity >= 1.),
            "a panel with its own background is never half transparent: {progress} → {:?}",
            mixed.opacity
        );
    }
    assert_eq!(blend_hover(&resting, &hovered, 0.9).visibility, Visibility::Visible);
}

struct Revealed {
    hoverable: bool,
}

impl Render for Revealed {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(
            div()
                .id("target")
                .size(px(10.))
                .cursor_pointer()
                .when(self.hoverable, |d| d.invisible().hover(|s| s.visible()))
                .child(div().debug_selector(|| "inner".into()).size_full()),
        )
    }
}

fn revealed_after_leaving(cx: &mut TestAppContext, transition: Option<Duration>) -> bool {
    let (view, cx) = cx.add_window_view(|_, _| Revealed { hoverable: false });
    cx.update(|_, cx| cx.set_global(HoverTransition { duration: transition }));
    cx.run_until_parked();
    cx.simulate_mouse_move(point(px(5.), px(5.)), None, Modifiers::default());
    view.update(cx, |view, cx| {
        view.hoverable = true;
        cx.notify();
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("inner").is_some(), "the hovered element is revealed");
    cx.simulate_mouse_move(point(px(50.), px(50.)), None, Modifiers::default());
    cx.run_until_parked();
    cx.debug_bounds("inner").is_some()
}

#[gpui::test]
fn hover_transition_keeps_the_hover_style_while_fading_out(cx: &mut TestAppContext) {
    assert!(revealed_after_leaving(cx, Some(Duration::from_secs(60))));
}

#[gpui::test]
fn hover_without_transition_switches_instantly(cx: &mut TestAppContext) {
    assert!(!revealed_after_leaving(cx, None));
}

struct Grows;

impl Render for Grows {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .child(div().id("target").debug_selector(|| "target".into()).size(px(10.)).hover(|s| s.size(px(20.))))
    }
}

fn hover_and_redraw<V: 'static>(view: &Entity<V>, cx: &mut VisualTestContext) {
    cx.simulate_mouse_move(point(px(5.), px(5.)), None, Modifiers::default());
    cx.run_until_parked();
    view.update(cx, |_, cx| cx.notify());
    cx.run_until_parked();
}

#[gpui::test]
fn hover_never_changes_the_size(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, _| Grows);
    cx.run_until_parked();
    hover_and_redraw(&view, cx);
    let bounds = cx.debug_bounds("target").expect("the target is painted");
    assert_eq!(bounds.size.width, px(10.));
}

const RESTING_TEXT: Hsla = Hsla { h: 0., s: 0., l: 0.4, a: 1. };
const HOVER_TEXT: Hsla = Hsla { h: 0., s: 0., l: 0.9, a: 1. };

struct Labelled {
    seen: Rc<Cell<Option<(Hsla, AbsoluteLength)>>>,
}

impl Render for Labelled {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let seen = self.seen.clone();
        div().size_full().child(
            div()
                .id("label")
                .size(px(40.))
                .text_size(px(12.))
                .text_color(RESTING_TEXT)
                .hover(|s| s.text_color(HOVER_TEXT))
                .child(
                    canvas(
                        move |_, window, _| {
                            let text = window.text_style();
                            seen.set(Some((text.color, text.font_size)));
                        },
                        |_, _, _, _| {},
                    )
                    .size_full(),
                ),
        )
    }
}

#[gpui::test]
fn hover_text_color_reaches_layout_and_keeps_the_text_size(cx: &mut TestAppContext) {
    let seen = Rc::new(Cell::new(None));
    let (view, cx) = cx.add_window_view(|_, _| Labelled { seen: seen.clone() });
    cx.run_until_parked();
    hover_and_redraw(&view, cx);
    let (color, size) = seen.get().expect("the label is laid out");
    assert_eq!(color, HOVER_TEXT);
    assert_eq!(size, px(12.).into());
}

struct Reordered {
    swapped: bool,
}

impl Render for Reordered {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let names = if self.swapped { ["b", "a"] } else { ["a", "b"] };
        div().size_full().flex().flex_col().children(names.map(|name| {
            div()
                .id(name)
                .size(px(10.))
                .invisible()
                .hover(|s| s.visible())
                .child(div().debug_selector(move || format!("{name}-inner")).size_full())
        }))
    }
}

#[gpui::test]
fn hover_transition_does_not_follow_an_element_moved_from_under_the_pointer(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, _| Reordered { swapped: false });
    cx.update(|_, cx| cx.set_global(HoverTransition { duration: Some(Duration::from_secs(60)) }));
    cx.run_until_parked();
    cx.simulate_mouse_move(point(px(5.), px(5.)), None, Modifiers::default());
    cx.run_until_parked();
    view.update(cx, |_, cx| cx.notify());
    cx.run_until_parked();
    assert!(cx.debug_bounds("a-inner").is_some(), "the element under the pointer is revealed");
    view.update(cx, |view, cx| {
        view.swapped = true;
        cx.notify();
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("a-inner").is_none(), "the moved element does not fade out at its new place");
    assert!(cx.debug_bounds("b-inner").is_some(), "the element that came under the pointer is revealed");
}

#[derive(Clone, Copy, PartialEq, Debug)]
struct Lifted(usize);

struct Ghost;

impl Render for Ghost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size(px(10.))
    }
}

struct Draggables;

impl Render for Draggables {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().flex().flex_col().children((0..2).map(|index| {
            div().id(("draggable", index)).size(px(40.)).on_drag(Lifted(index), |_, _, _, cx| cx.new(|_| Ghost))
        }))
    }
}

#[gpui::test]
fn active_drag_value_reports_the_dragged_value_until_release(cx: &mut TestAppContext) {
    let (_, cx) = cx.add_window_view(|_, _| Draggables);
    cx.run_until_parked();
    let lifted = |cx: &mut VisualTestContext| cx.update(|_, cx| cx.active_drag_value::<Lifted>().copied());
    assert_eq!(lifted(cx), None);
    cx.simulate_mouse_down(point(px(20.), px(60.)), MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(point(px(20.), px(20.)), MouseButton::Left, Modifiers::default());
    cx.run_until_parked();
    assert_eq!(lifted(cx), Some(Lifted(1)), "the value of the pressed element, not of the one under the pointer");
    assert!(cx.update(|_, cx| cx.active_drag_value::<usize>().is_none()), "another type is not reported");
    cx.simulate_mouse_up(point(px(20.), px(20.)), MouseButton::Left, Modifiers::default());
    cx.run_until_parked();
    assert_eq!(lifted(cx), None);
}

struct Capped {
    layout: Rc<RefCell<Option<gpui::TextLayout>>>,
}

impl Render for Capped {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let text = gpui::StyledText::new("Уменьшить изображения до нужного размера");
        *self.layout.borrow_mut() = Some(text.layout().clone());
        div().size_full().flex().child(div().max_w(px(60.)).truncate().child(text))
    }
}

#[gpui::test]
fn truncated_text_capped_by_max_width_ends_with_an_ellipsis(cx: &mut TestAppContext) {
    let layout = Rc::new(RefCell::new(None));
    let shared = layout.clone();
    let (_, cx) = cx.add_window_view(move |_, _| Capped { layout: shared });
    cx.run_until_parked();
    let layout = layout.borrow().clone().expect("rendered");
    let shown = layout.text();
    assert!(shown.ends_with('…'), "the text is cut with an ellipsis, not clipped mid-letter: {shown:?}");
    assert!(layout.bounds().size.width <= px(60.), "the laid out text fits its box: {:?}", layout.bounds());
}

struct Remeasured {
    shown: Rc<RefCell<Vec<String>>>,
}

impl Render for Remeasured {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let shown = self.shown.clone();
        canvas(
            move |_, window, cx| {
                let text = gpui::StyledText::new("Уменьшить изображения до нужного размера");
                let layout = text.layout().clone();
                let mut element = div().max_w(px(60.)).truncate().child(text).into_any_element();
                let id = element.request_layout(window, cx);
                for width in [gpui::AvailableSpace::Definite(px(60.)), gpui::AvailableSpace::MaxContent] {
                    window.compute_layout(id, size(width, gpui::AvailableSpace::MaxContent), cx);
                    shown.borrow_mut().push(layout.text());
                }
            },
            |_, _, _, _| {},
        )
        .size_full()
    }
}

#[gpui::test]
fn truncated_text_measured_again_keeps_its_runs_matching_the_text(cx: &mut TestAppContext) {
    let shown = Rc::new(RefCell::new(Vec::new()));
    let shared = shown.clone();
    let (_, cx) = cx.add_window_view(move |_, _| Remeasured { shown: shared });
    cx.run_until_parked();
    let shown = shown.borrow();
    assert!(shown.first().is_some_and(|text| text.ends_with('…')), "the narrow pass truncates: {shown:?}");
    assert_eq!(shown.len(), 2, "both passes ran without a panic: {shown:?}");
}

#[gpui::test]
fn truncation_never_leaves_a_space_or_punctuation_before_the_ellipsis(cx: &mut TestAppContext) {
    let text = "Прошлый запуск не удался, ошибка: файл «отчёт 2026.pdf» — занят";
    let style = font("Segoe UI");
    cx.update(|cx| {
        let mut wrapper = cx.text_system().line_wrapper(style.clone(), px(12.));
        for width in 1..400 {
            let runs = vec![TextRun {
                len: text.len(),
                font: style.clone(),
                color: Hsla::default(),
                background_color: None,
                underline: None,
                strikethrough: None,
            }];
            let (shown, runs) =
                wrapper.truncate_line(text.into(), px(width as f32), "…", &runs, gpui::TruncateFrom::End);
            assert_eq!(runs.iter().map(|run| run.len).sum::<usize>(), shown.len(), "runs cover {shown:?}");
            let Some(kept) = shown.strip_suffix('…') else {
                continue;
            };
            assert!(
                kept.is_empty() || kept.chars().last().is_some_and(char::is_alphanumeric),
                "at {width} px the cut ends on a letter or digit: {shown:?}"
            );
        }
    });
}

struct HalfPixel;

impl Render for HalfPixel {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().child(div().debug_selector(|| "box".into()).w(px(10.25)).h(px(10.)))
    }
}

#[gpui::test]
fn half_device_pixels_round_up(cx: &mut TestAppContext) {
    let (_, cx) = cx.add_window_view(|_, _| HalfPixel);
    cx.run_until_parked();
    assert_eq!(cx.update(|window, _| window.scale_factor()), 2.);
    let width = cx.debug_bounds("box").expect("the box is painted").size.width;
    assert_eq!(width, px(10.5), "20.5 device pixels keep their 21st pixel, as Traydex is designed at 125%");
}

struct Segments {
    offset: f32,
}

impl Render for Segments {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w(px(100.)).flex().child(div().w(relative(self.offset))).child(
            div()
                .debug_selector(|| "row".into())
                .w(relative(0.5))
                .flex()
                .p(px(1.5))
                .children((0..3).map(|i| div().debug_selector(move || format!("cell{i}")).flex_1().h(px(10.)))),
        )
    }
}

#[gpui::test]
fn children_split_inside_a_fractionally_placed_parent_keep_equal_padding(cx: &mut TestAppContext) {
    for offset in [0.0015, 0.0025, 0.0035, 0.1013, 0.2047] {
        let (_, cx) = cx.add_window_view(move |_, _| Segments { offset });
        cx.run_until_parked();
        let mut bounds = |name: &'static str| cx.debug_bounds(name).expect("the segment row is painted");
        let (row, first, last) = (bounds("row"), bounds("cell0"), bounds("cell2"));
        assert_eq!(first.left() - row.left(), px(1.5), "left padding at offset {offset}");
        assert_eq!(row.right() - last.right(), px(1.5), "right padding at offset {offset}");
    }
}

#[gpui::test]
fn baseline_offset_leaves_equal_room_above_capitals_and_below_baseline(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let text = cx.text_system();
        let id = text.resolve_font(&font("Segoe UI"));
        let (size, line) = (px(13.), px(18.));
        let cap_height = text.cap_height(id, size);
        let baseline = text.baseline_offset(id, size, line);
        let (above, below) = (f32::from(baseline - cap_height), f32::from(line - baseline));
        assert!((above - below).abs() < 1e-3, "{above} above the capitals, {below} below the baseline");
    });
}

#[test]
fn icon_lines_land_on_whole_device_pixels_at_any_scale() {
    use gpui::AssetSource as _;
    const ROW: usize = 5;
    let plus = crate::icons::Assets.load("plus").expect("load").expect("the plus icon");
    for device in [14, 17, 18, 20, 21] {
        let texels = DevicePixels(device * 2);
        let (_, mask) = gpui::svg_icon_mask(&plus, size(texels, texels)).expect("rasterized");
        let side = device as usize * 2;
        let pixel = |x: usize, y: usize| -> u32 {
            [(0, 0), (1, 0), (0, 1), (1, 1)]
                .iter()
                .map(|(dx, dy)| u32::from(mask[(y * 2 + dy) * side + x * 2 + dx]))
                .sum::<u32>()
                / 4
        };
        let row: Vec<u32> = (0..device as usize).map(|x| pixel(x, ROW)).collect();
        assert!(
            row.iter().any(|alpha| *alpha >= 250),
            "at {device} px the vertical stroke of + is spread over {row:?} without a solid pixel"
        );
    }
}

#[test]
fn logo_arrow_stays_centered_in_its_pentagon() {
    use gpui::AssetSource as _;
    let arrow = crate::icons::Assets.load("brand-arrow").expect("load").expect("the logo arrow");
    for device in [18, 20, 22, 25] {
        let side = device as usize * 2;
        let (_, mask) = gpui::svg_icon_mask(&arrow, size(DevicePixels(side as i32), DevicePixels(side as i32)))
            .expect("rasterized");
        let stem = side * 3 / 8;
        let row = &mask[stem * side..(stem + 1) * side];
        let ink: f32 = row.iter().map(|alpha| f32::from(*alpha)).sum();
        let middle = row.iter().enumerate().map(|(x, alpha)| x as f32 * f32::from(*alpha)).sum::<f32>() / ink;
        let center = side as f32 / 2. - 0.5;
        assert!((middle - center).abs() <= 0.25, "at {device} px the arrow stem isn't centered: {row:?}");
    }
}

struct Popup {
    at: Point<Pixels>,
}

impl Render for Popup {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(gpui::deferred(
            gpui::anchored()
                .position(self.at)
                .child(div().debug_selector(|| "popup".into()).size(px(20.)).bg(gpui::black())),
        ))
    }
}

#[gpui::test]
fn anchored_element_lands_on_the_device_pixel_it_was_given(cx: &mut TestAppContext) {
    let at = point(px(10.5), px(20.5));
    let (_, cx) = cx.add_window_view(move |_, _| Popup { at });
    cx.run_until_parked();
    assert_eq!(cx.update(|window, _| window.scale_factor()), 2.);
    let popup = cx.debug_bounds("popup").expect("the popup is drawn");
    assert_eq!(
        popup.origin, at,
        "half a logical pixel is a whole device pixel at scale 2 and must not be rounded away"
    );
}
