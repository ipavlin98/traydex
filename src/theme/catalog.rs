use super::palette::{Hex, accented, dark, neutrals, parse_color, readable, text_for};
use super::{Theme, ThemeChoice};
use gpui::{Rgba, WindowAppearance};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::BTreeMap;

const LIGHT: &str = "Light";
const DARK: &str = "Dark";
const SYSTEM: &str = "System";
const BACKGROUND: &str = "background";
const TEXT: &str = "text";
const ACCENT: &str = "accent";

const EXAMPLES: [(&str, &str); 3] = [
    ("catppuccin-mocha", include_str!("../../resources/themes/catppuccin-mocha.json")),
    ("dracula", include_str!("../../resources/themes/dracula.json")),
    ("soft-sand", include_str!("../../resources/themes/soft-sand.json")),
];

#[derive(Clone, Copy, Default, PartialEq, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemeColors {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accent: Option<Hex>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background: Option<Hex>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<Hex>,
}

impl ThemeColors {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

pub fn colors_by_theme<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<BTreeMap<String, ThemeColors>, D::Error> {
    let value = serde_json::Value::deserialize(deserializer)?;
    let serde_json::Value::Object(themes) = value else { return Ok(BTreeMap::new()) };
    Ok(themes
        .into_iter()
        .filter_map(|(id, colors)| Some((id, serde_json::from_value(colors).ok()?)))
        .filter(|(_, colors): &(String, ThemeColors)| !colors.is_empty())
        .collect())
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Origin {
    Light,
    Dark,
    Example,
    File,
}

#[derive(Clone)]
pub struct Entry {
    id: String,
    name: String,
    origin: Origin,
    theme: Theme,
}

impl Entry {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn origin(&self) -> Origin {
        self.origin
    }

    pub fn own(&self) -> Theme {
        self.theme
    }

    pub fn tinted(&self, colors: &ThemeColors) -> Theme {
        let own = self.theme;
        let background = colors.background.map(Hex::color);
        if background.is_none() && colors.text.is_none() {
            return colors.accent.map_or(own, |accent| accented(own, accent.color()));
        }
        let bg = background.unwrap_or(own.bg);
        let text = colors.text.map(Hex::color).or(background.map(text_for)).unwrap_or(own.text);
        let theme = neutrals(bg, text);
        let accent = colors.accent.map(Hex::color).unwrap_or(if readable(own.accent, bg) { own.accent } else { text });
        accented(theme, accent)
    }
}

#[derive(Clone)]
pub struct Catalog {
    entries: Vec<Entry>,
    failures: Vec<String>,
}

impl Default for Catalog {
    fn default() -> Self {
        Self::load(Vec::new())
    }
}

impl Catalog {
    pub fn load(files: Vec<(String, String)>) -> Self {
        let mut catalog = Self {
            entries: vec![
                Entry { id: LIGHT.into(), name: LIGHT.into(), origin: Origin::Light, theme: Theme::light() },
                Entry { id: DARK.into(), name: DARK.into(), origin: Origin::Dark, theme: Theme::dark() },
            ],
            failures: Vec::new(),
        };
        for (id, text) in EXAMPLES {
            match parse(id, text, Origin::Example) {
                Ok(entry) => catalog.entries.push(entry),
                Err(error) => log::error!("The built-in theme {id} is broken: {error}"),
            }
        }
        for (id, text) in files {
            let taken = id.eq_ignore_ascii_case(SYSTEM) || catalog.entries.iter().any(|entry| entry.id.eq_ignore_ascii_case(&id));
            let parsed =
                if taken { Err("this name is taken by a built-in theme".to_owned()) } else { parse(&id, &text, Origin::File) };
            match parsed {
                Ok(entry) => catalog.entries.push(entry),
                Err(error) => {
                    log::warn!("Couldn't load the theme {id}.json: {error}");
                    catalog.failures.push(format!("{id}.json: {error}"));
                }
            }
        }
        catalog
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn failures(&self) -> &[String] {
        &self.failures
    }

    pub fn current(&self, choice: &ThemeChoice, appearance: WindowAppearance) -> Option<&Entry> {
        let chosen = match choice {
            ThemeChoice::System => None,
            ThemeChoice::Named(id) => self.entries.iter().find(|entry| &entry.id == id),
        };
        let system = if Theme::system(appearance).is_dark() { Origin::Dark } else { Origin::Light };
        chosen.or_else(|| self.entries.iter().find(|entry| entry.origin == system))
    }

    pub fn resolve(
        &self,
        choice: &ThemeChoice,
        appearance: WindowAppearance,
        colors: &BTreeMap<String, ThemeColors>,
    ) -> Theme {
        match self.current(choice, appearance) {
            Some(entry) => entry.tinted(colors.get(&entry.id).unwrap_or(&ThemeColors::default())),
            None => Theme::system(appearance),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    name: Option<String>,
    base: Option<Base>,
    #[serde(default)]
    palette: BTreeMap<String, String>,
    #[serde(default)]
    colors: BTreeMap<String, String>,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Base {
    Light,
    Dark,
}

fn parse(id: &str, text: &str, origin: Origin) -> Result<Entry, String> {
    let file: File = serde_json::from_str(text).map_err(|error| error.to_string())?;
    let palette = resolve_palette(file.palette)?;
    let role = |name: &str| palette.get(name).copied();
    let base = match file.base {
        Some(Base::Dark) => Theme::dark(),
        Some(Base::Light) => Theme::light(),
        None if role(BACKGROUND).is_some_and(dark) => Theme::dark(),
        None => Theme::light(),
    };
    let mut theme = match (role(BACKGROUND), role(TEXT)) {
        (None, None) => base,
        (background, text) => {
            let bg = background.unwrap_or(base.bg);
            let theme = neutrals(bg, text.unwrap_or_else(|| text_for(bg)));
            let accent = if readable(base.accent, bg) { base.accent } else { theme.text };
            accented(theme, accent)
        }
    };
    if let Some(accent) = role(ACCENT) {
        theme = accented(theme, accent);
    }
    for (token, value) in &file.colors {
        let color = palette.get(value.as_str()).copied().or_else(|| parse_color(value));
        let color = color.ok_or_else(|| format!("colors.{token}: `{value}` is neither a color nor a palette name"))?;
        *theme.token_mut(token).ok_or_else(|| format!("unknown color `{token}`"))? = color;
    }
    Ok(Entry { id: id.to_owned(), name: file.name.unwrap_or_else(|| id.to_owned()), origin, theme })
}

fn resolve_palette(palette: BTreeMap<String, String>) -> Result<BTreeMap<String, Rgba>, String> {
    let mut resolved = BTreeMap::new();
    for name in palette.keys() {
        let mut value = name;
        let mut color = None;
        for _ in 0..=palette.len() {
            let Some(next) = palette.get(value) else { break };
            if let Some(parsed) = parse_color(next) {
                color = Some(parsed);
                break;
            }
            value = next;
        }
        let color = color.ok_or_else(|| format!("palette.{name}: `{}` is neither a color nor a palette name", palette.get(name).map_or("", String::as_str)))?;
        resolved.insert(name.clone(), color);
    }
    Ok(resolved)
}
