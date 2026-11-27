use super::*;
use std::collections::BTreeMap;

const APPEARANCES: [WindowAppearance; 4] =
    [WindowAppearance::Light, WindowAppearance::Dark, WindowAppearance::VibrantLight, WindowAppearance::VibrantDark];

fn named(id: &str) -> ThemeChoice {
    ThemeChoice::Named(id.to_owned())
}

fn resolve(catalog: &Catalog, choice: &ThemeChoice, colors: &[(&str, ThemeColors)]) -> Theme {
    let colors: BTreeMap<String, ThemeColors> = colors.iter().map(|(id, colors)| ((*id).to_owned(), *colors)).collect();
    catalog.resolve(choice, WindowAppearance::Light, &colors)
}

fn hex(text: &str) -> Hex {
    Hex::parse(text).expect("a #rrggbb color")
}

#[test]
fn explicit_themes_ignore_system_appearance() {
    let catalog = Catalog::default();
    for appearance in APPEARANCES {
        assert_eq!(catalog.resolve(&named("Dark"), appearance, &BTreeMap::new()).bg, rgb(0x000000));
        assert_eq!(catalog.resolve(&named("Light"), appearance, &BTreeMap::new()).bg, rgb(0xffffff));
    }
}

#[test]
fn system_choice_follows_appearance() {
    let catalog = Catalog::default();
    let bg = |appearance| catalog.resolve(&ThemeChoice::System, appearance, &BTreeMap::new()).bg;
    assert_eq!(bg(WindowAppearance::Dark), rgb(0x000000));
    assert_eq!(bg(WindowAppearance::VibrantDark), rgb(0x000000));
    assert_eq!(bg(WindowAppearance::Light), rgb(0xffffff));
    assert_eq!(bg(WindowAppearance::VibrantLight), rgb(0xffffff));
}

#[test]
fn a_missing_theme_falls_back_to_the_system_one() {
    let catalog = Catalog::default();
    assert_eq!(catalog.resolve(&named("deleted"), WindowAppearance::Dark, &BTreeMap::new()).bg, rgb(0x000000));
}

#[test]
fn saved_theme_names_are_stable() {
    let names: Vec<String> = [ThemeChoice::System, named("Light"), named("Dark"), named("dracula")]
        .iter()
        .map(|choice| serde_json::to_string(choice).expect("json"))
        .collect();
    assert_eq!(names, ["\"System\"", "\"Light\"", "\"Dark\"", "\"dracula\""]);
    assert_eq!(serde_json::from_str::<ThemeChoice>("\"System\"").expect("json"), ThemeChoice::System);
    assert_eq!(serde_json::from_str::<ThemeChoice>("7").expect("json"), ThemeChoice::System);
    assert!(ThemeChoice::default() == ThemeChoice::System);
}

#[test]
fn every_built_in_theme_loads() {
    let catalog = Catalog::default();
    let ids: Vec<&str> = catalog.entries().iter().map(Entry::id).collect();
    assert_eq!(ids, ["Light", "Dark", "catppuccin-mocha", "dracula", "soft-sand"]);
    assert!(catalog.failures().is_empty());
}

#[test]
fn a_theme_file_derives_its_shades_from_the_palette_and_overrides_named_colors() {
    let file = r##"{"name": "Night", "palette": {"ink": "#e0e0e0", "background": "#101820", "text": "ink", "accent": "#ff8800"},
        "colors": {"danger": "#ff0000", "faint": "ink"}}"##;
    let catalog = Catalog::load(vec![("night".into(), file.into())]);
    assert!(catalog.failures().is_empty(), "{:?}", catalog.failures());
    let theme = resolve(&catalog, &named("night"), &[]);
    assert_eq!((theme.bg, theme.text, theme.accent), (rgb(0x101820), rgb(0xe0e0e0), rgb(0xff8800)));
    assert_eq!((theme.danger, theme.faint), (rgb(0xff0000), rgb(0xe0e0e0)));
    assert!(theme.is_dark());
    assert!(theme.hover != theme.bg && theme.border != theme.bg);
    assert!(contrast(theme.muted, theme.bg) >= BODY);
}

#[test]
fn broken_or_clashing_theme_files_are_reported_and_skipped() {
    let catalog = Catalog::load(vec![
        ("light".into(), "{}".into()),
        ("bad".into(), r##"{"colors": {"glow": "#ffffff"}}"##.into()),
        ("loop".into(), r#"{"palette": {"background": "text", "text": "background"}}"#.into()),
        ("fine".into(), r##"{"palette": {"accent": "#00aa00"}}"##.into()),
    ]);
    let ids: Vec<&str> = catalog.entries().iter().map(Entry::id).collect();
    assert!(ids.contains(&"fine") && !ids.contains(&"light") && !ids.contains(&"bad") && !ids.contains(&"loop"));
    assert_eq!(catalog.failures().len(), 3, "{:?}", catalog.failures());
    assert_eq!(resolve(&catalog, &named("fine"), &[]).bg, rgb(0xffffff), "no background keeps the light base");
}

#[test]
fn a_chosen_background_brings_readable_text_and_its_own_shades() {
    let catalog = Catalog::default();
    let colors = ThemeColors { background: Some(hex("#0f172a")), ..ThemeColors::default() };
    let theme = resolve(&catalog, &named("Light"), &[("Light", colors)]);
    assert_eq!(theme.bg, rgb(0x0f172a));
    assert!(theme.is_dark());
    for (name, color) in [("text", theme.text), ("muted", theme.muted), ("accent_text", theme.accent_text)] {
        assert!(contrast(color, theme.bg) >= BODY, "{name}");
    }
    assert!(theme.surface != Theme::light().surface && theme.hover != Theme::light().hover);
}

#[test]
fn an_accent_unreadable_on_the_new_background_gives_way_to_the_text_color() {
    let catalog = Catalog::load(vec![("ink".into(), r##"{"palette": {"accent": "#111111"}}"##.into())]);
    let colors = ThemeColors { background: Some(hex("#000000")), ..ThemeColors::default() };
    let theme = resolve(&catalog, &named("ink"), &[("ink", colors)]);
    assert_eq!(theme.accent, theme.text);
}

#[test]
fn own_text_and_accent_win_over_the_automatic_ones() {
    let catalog = Catalog::default();
    let colors =
        ThemeColors { accent: Some(hex("#22c55e")), background: Some(hex("#f6f3ec")), text: Some(hex("#3a2a1a")) };
    let theme = resolve(&catalog, &named("Dark"), &[("Dark", colors)]);
    assert_eq!((theme.bg, theme.text, theme.accent), (rgb(0xf6f3ec), rgb(0x3a2a1a), rgb(0x22c55e)));
    assert_eq!(theme.brand, rgb(0x22c55e));
}

#[test]
fn colors_chosen_for_one_theme_stay_with_it() {
    let catalog = Catalog::default();
    let colors = ThemeColors { accent: Some(hex("#ef4444")), ..ThemeColors::default() };
    let tinted = [("dracula", colors)];
    assert_eq!(resolve(&catalog, &named("dracula"), &tinted).accent, rgb(0xef4444));
    assert_eq!(resolve(&catalog, &named("Light"), &tinted).accent, Theme::light().accent);
}

#[test]
fn presets_skip_the_color_the_theme_already_offers() {
    let shown: Vec<Hex> = offered(&BACKGROUNDS, rgb(0xffffff)).map(|(_, color)| color).collect();
    assert_eq!(shown.len(), 7);
    assert!(!shown.contains(&hex("#ffffff")));
    assert_eq!(offered(&ACCENTS, rgb(0x123456)).count(), 7);
}

#[test]
fn colors_survive_the_trip_through_hue_saturation_and_value() {
    for color in [0xff0080, 0x33996a, 0x1a1a1a, 0xffffff, 0x4d80e6] {
        let back = Hex::of(Hsv::of(rgb(color)).color());
        assert_eq!(back, Hex::of(rgb(color)), "{color:06x}");
    }
    assert_eq!(Hsv::of(rgb(0x808080)).saturation, 0., "grays have no saturation");
}

#[test]
fn hex_colors_read_back_as_written() {
    assert_eq!(String::from(hex("#A1b2C3")), "#a1b2c3");
    assert!(Hex::parse("red").is_none() && Hex::parse("#abc").is_none() && Hex::parse("#a1b2c3d4").is_none());
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
    for entry in Catalog::default().entries() {
        let t = entry.own();
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
                failures.push(format!("{}: {name} {ratio:.2} < {least}", entry.id()));
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
