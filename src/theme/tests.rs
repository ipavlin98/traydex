use super::*;

#[test]
fn explicit_modes_ignore_system_appearance() {
    for appearance in
        [WindowAppearance::Light, WindowAppearance::Dark, WindowAppearance::VibrantLight, WindowAppearance::VibrantDark]
    {
        assert_eq!(Theme::resolve(ThemeMode::Dark, appearance).bg, rgb(0x000000));
        assert_eq!(Theme::resolve(ThemeMode::Light, appearance).bg, rgb(0xffffff));
    }
}

#[test]
fn system_mode_follows_appearance() {
    assert_eq!(Theme::resolve(ThemeMode::System, WindowAppearance::Dark).bg, rgb(0x000000));
    assert_eq!(Theme::resolve(ThemeMode::System, WindowAppearance::VibrantDark).bg, rgb(0x000000));
    assert_eq!(Theme::resolve(ThemeMode::System, WindowAppearance::Light).bg, rgb(0xffffff));
    assert_eq!(Theme::resolve(ThemeMode::System, WindowAppearance::VibrantLight).bg, rgb(0xffffff));
}

#[test]
fn text_contrasts_with_background_in_both_themes() {
    for mode in [ThemeMode::Light, ThemeMode::Dark] {
        let theme = Theme::resolve(mode, WindowAppearance::Light);
        let luminance = |color: Rgba| 0.2126 * color.r + 0.7152 * color.g + 0.0722 * color.b;
        assert!((luminance(theme.text) - luminance(theme.bg)).abs() > 0.7);
    }
}

#[test]
fn saved_theme_names_are_stable() {
    let names: Vec<String> = [ThemeMode::System, ThemeMode::Light, ThemeMode::Dark]
        .iter()
        .map(|mode| serde_json::to_string(mode).expect("json"))
        .collect();
    assert_eq!(names, ["\"System\"", "\"Light\"", "\"Dark\""]);
    assert!(ThemeMode::default() == ThemeMode::System);
}

fn luminance(color: Rgba) -> f32 {
    let channel = |value: f32| {
        if value <= 0.04045 { value / 12.92 } else { ((value + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * channel(color.r) + 0.7152 * channel(color.g) + 0.0722 * channel(color.b)
}

fn contrast(text: Rgba, background: Rgba) -> f32 {
    let (a, b) = (luminance(text), luminance(background));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

fn over(top: Rgba, base: Rgba) -> Rgba {
    let mix = |high: f32, low: f32| high * top.a + low * (1. - top.a);
    Rgba { r: mix(top.r, base.r), g: mix(top.g, base.g), b: mix(top.b, base.b), a: 1. }
}

const BODY: f32 = 4.5;
const TERTIARY: f32 = 2.5;

#[test]
fn text_tokens_meet_contrast_thresholds_on_their_backgrounds() {
    let mut failures = Vec::new();
    for mode in [ThemeMode::Light, ThemeMode::Dark] {
        let t = Theme::resolve(mode, WindowAppearance::Light);
        let pairs = [
            ("text/bg", t.text, t.bg, BODY),
            ("text/surface", t.text, t.surface, BODY),
            ("text/hover", t.text, t.hover, BODY),
            ("text/active", t.text, t.active, BODY),
            ("text/tint-bg", t.text, over(t.accent_tint, t.bg), BODY),
            ("text/tint-surface", t.text, over(t.accent_tint, t.surface), BODY),
            ("text/selection", t.text, over(t.selection, t.bg), BODY),
            ("muted/bg", t.muted, t.bg, BODY),
            ("muted/surface", t.muted, t.surface, BODY),
            ("muted/hover", t.muted, t.hover, BODY),
            ("muted/active", t.muted, t.active, BODY),
            ("accent_text/bg", t.accent_text, t.bg, BODY),
            ("accent_text/surface", t.accent_text, t.surface, BODY),
            ("accent_text/tint-bg", t.accent_text, over(t.accent_tint, t.bg), BODY),
            ("accent_text/tint-surface", t.accent_text, over(t.accent_tint, t.surface), BODY),
            ("accent_text/selection", t.accent_text, over(t.selection, t.bg), BODY),
            ("danger/bg", t.danger, t.bg, BODY),
            ("danger/surface", t.danger, t.surface, BODY),
            ("danger/hover", t.danger, t.hover, BODY),
            ("danger/danger-tint-surface", t.danger, over(t.danger_tint, t.surface), BODY),
            ("muted/danger-tint-surface", t.muted, over(t.danger_tint, t.surface), BODY),
            ("faint/bg", t.faint, t.bg, TERTIARY),
            ("faint/surface", t.faint, t.surface, TERTIARY),
            ("faint/hover", t.faint, t.hover, TERTIARY),
            ("code_ink/code_paper", t.code_ink, t.code_paper, BODY),
        ];
        for (name, text, background, least) in pairs {
            let ratio = contrast(text, background);
            if ratio < least {
                failures
                    .push(format!("{}: {name} {ratio:.2} < {least}", if t.bg == rgb(0) { "dark" } else { "light" }));
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn contrast_formula_matches_wcag_reference_values() {
    assert!((contrast(rgb(0x000000), rgb(0xffffff)) - 21.).abs() < 0.01);
    assert!((contrast(rgb(0x767676), rgb(0xffffff)) - 4.54).abs() < 0.01);
    assert!((contrast(rgb(0xffffff), rgb(0xffffff)) - 1.).abs() < 0.001);
    assert_eq!(over(rgba(0xffffff00), rgb(0x123456)), rgb(0x123456));
}
