use crate::i18n::tr;
use crate::os;
use crate::profile::{self, Profile};
use gpui::{Keystroke, SharedString};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::BTreeMap;
use windows::Win32::UI::Input::KeyboardAndMouse::*;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Serialize, Deserialize)]
pub enum Command {
    Toggle,
    Pin,
    Actions,
    Settings,
    Storage,
    Layout,
    Paste,
    Note,
    Copy,
    SelectAll,
    Open,
    Reveal,
    Preview,
    Mark,
    Stick,
    Remove,
    Clear,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Scope {
    System,
    Window,
}

impl Command {
    pub const ALL: [Command; 17] = [
        Command::Toggle,
        Command::Pin,
        Command::Actions,
        Command::Settings,
        Command::Storage,
        Command::Layout,
        Command::Paste,
        Command::Note,
        Command::Copy,
        Command::SelectAll,
        Command::Open,
        Command::Reveal,
        Command::Preview,
        Command::Mark,
        Command::Stick,
        Command::Remove,
        Command::Clear,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Command::Toggle => tr().key_toggle,
            Command::Pin => tr().key_pin,
            Command::Actions => tr().key_actions,
            Command::Settings => tr().key_settings,
            Command::Storage => tr().key_storage,
            Command::Layout => tr().key_layout,
            Command::Paste => tr().key_paste,
            Command::Note => tr().key_note,
            Command::Copy => tr().key_copy,
            Command::SelectAll => tr().key_select_all,
            Command::Open => tr().key_open,
            Command::Reveal => tr().key_reveal,
            Command::Preview => tr().key_preview,
            Command::Mark => tr().key_mark,
            Command::Stick => tr().key_stick,
            Command::Remove => tr().key_remove,
            Command::Clear => tr().key_clear,
        }
    }

    pub fn scope(self) -> Scope {
        match self {
            Command::Toggle => Scope::System,
            _ => Scope::Window,
        }
    }

    pub fn editable(self) -> bool {
        matches!(self, Command::Toggle)
    }

    pub fn default_shortcut(self) -> Shortcut {
        self.default_in(profile::current())
    }

    fn default_in(self, profile: &Profile) -> Shortcut {
        match self {
            Command::Toggle if profile.testing() => TEST_TOGGLE,
            Command::Toggle => Shortcut::ctrl(VK_SPACE).with_shift(),
            Command::Pin => Shortcut::ctrl(VK_P),
            Command::Actions => Shortcut::ctrl(VK_K),
            Command::Settings => Shortcut::ctrl(VK_O),
            Command::Storage => Shortcut::ctrl(VK_E),
            Command::Layout => Shortcut::ctrl(VK_G),
            Command::Paste => Shortcut::ctrl(VK_V),
            Command::Note => Shortcut::ctrl(VK_N),
            Command::Copy => Shortcut::ctrl(VK_C),
            Command::SelectAll => Shortcut::ctrl(VK_A),
            Command::Open => Shortcut::plain(VK_RETURN),
            Command::Reveal => Shortcut::ctrl(VK_RETURN),
            Command::Preview => Shortcut::plain(VK_SPACE),
            Command::Mark => Shortcut::plain(VK_X),
            Command::Stick => Shortcut::plain(VK_P),
            Command::Remove => Shortcut::plain(VK_DELETE),
            Command::Clear => Shortcut::ctrl(VK_DELETE),
        }
    }

    fn alias(self) -> Option<Shortcut> {
        match self {
            Command::Remove => Some(Shortcut::plain(VK_BACK)),
            _ => None,
        }
    }
}

pub const TEST_TOGGLE: Shortcut = Shortcut { ctrl: true, shift: true, alt: true, win: false, key: VK_F9.0 };

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Shortcut {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub win: bool,
    pub key: u16,
}

const NAMED: &[(VIRTUAL_KEY, &str, &str)] = &[
    (VK_SPACE, "space", "Space"),
    (VK_BACK, "backspace", "Backspace"),
    (VK_RETURN, "enter", "Enter"),
    (VK_TAB, "tab", "Tab"),
    (VK_UP, "up", "↑"),
    (VK_DOWN, "down", "↓"),
    (VK_RIGHT, "right", "→"),
    (VK_LEFT, "left", "←"),
    (VK_HOME, "home", "Home"),
    (VK_END, "end", "End"),
    (VK_PRIOR, "pageup", "PgUp"),
    (VK_NEXT, "pagedown", "PgDn"),
    (VK_ESCAPE, "escape", "Esc"),
    (VK_INSERT, "insert", "Ins"),
    (VK_DELETE, "delete", "Del"),
    (VK_APPS, "menu", "Menu"),
];

const LABELS: &[(VIRTUAL_KEY, &str)] = &[
    (VK_PAUSE, "Pause"),
    (VK_SNAPSHOT, "PrtSc"),
    (VK_MULTIPLY, "Num *"),
    (VK_ADD, "Num +"),
    (VK_SUBTRACT, "Num -"),
    (VK_DECIMAL, "Num ."),
    (VK_DIVIDE, "Num /"),
    (VK_OEM_1, ";"),
    (VK_OEM_PLUS, "="),
    (VK_OEM_COMMA, ","),
    (VK_OEM_MINUS, "-"),
    (VK_OEM_PERIOD, "."),
    (VK_OEM_2, "/"),
    (VK_OEM_3, "`"),
    (VK_OEM_4, "["),
    (VK_OEM_5, "\\"),
    (VK_OEM_6, "]"),
    (VK_OEM_7, "'"),
];

fn function_key(key: u16) -> Option<u16> {
    (VK_F1.0..=VK_F24.0).contains(&key).then(|| key - VK_F1.0 + 1)
}

fn named(key: u16) -> Option<(&'static str, &'static str)> {
    NAMED.iter().find(|(vk, _, _)| vk.0 == key).map(|(_, name, label)| (*name, *label))
}

fn key_name(key: u16, layout: &dyn Fn(u16) -> Option<char>) -> Option<String> {
    if let Some((name, _)) = named(key) {
        return Some(name.into());
    }
    if let Some(number) = function_key(key) {
        return Some(format!("f{number}"));
    }
    layout(key).map(|c| c.to_lowercase().to_string())
}

fn key_label(key: u16) -> String {
    match key {
        k if k == VK_RETURN.0 => tr().enter_key.into(),
        k if k == VK_ESCAPE.0 => tr().escape_key.into(),
        k if (VK_A.0..=VK_Z.0).contains(&k) || (VK_0.0..=VK_9.0).contains(&k) => char::from(k as u8).into(),
        k if (VK_NUMPAD0.0..=VK_NUMPAD9.0).contains(&k) => format!("Num {}", k - VK_NUMPAD0.0),
        k => named(k)
            .map(|(_, label)| label.to_string())
            .or_else(|| function_key(k).map(|number| format!("F{number}")))
            .or_else(|| LABELS.iter().find(|(vk, _)| vk.0 == k).map(|(_, label)| label.to_string()))
            .or_else(|| os::layout_char(k).map(|c| c.to_uppercase().to_string()))
            .unwrap_or_else(|| format!("#{k}")),
    }
}

impl Shortcut {
    pub const fn plain(key: VIRTUAL_KEY) -> Self {
        Self { ctrl: false, shift: false, alt: false, win: false, key: key.0 }
    }

    const fn ctrl(key: VIRTUAL_KEY) -> Self {
        Self { ctrl: true, ..Self::plain(key) }
    }

    const fn with_shift(self) -> Self {
        Self { shift: true, ..self }
    }

    pub fn is_cancel(self) -> bool {
        self == Self::plain(VK_ESCAPE)
    }

    pub fn fits(self, scope: Scope) -> bool {
        match scope {
            Scope::Window => true,
            Scope::System => self.ctrl || self.alt || self.win || function_key(self.key).is_some(),
        }
    }

    pub fn labels(self) -> Vec<SharedString> {
        let modifiers = [(self.ctrl, "Ctrl"), (self.shift, "Shift"), (self.alt, "Alt"), (self.win, "Win")];
        modifiers
            .into_iter()
            .filter(|(held, _)| *held)
            .map(|(_, label)| SharedString::from(label))
            .chain((self.key != 0).then(|| key_label(self.key).into()))
            .collect()
    }

    pub fn text(self) -> String {
        self.labels().iter().map(|label| label.as_ref()).collect::<Vec<_>>().join("+")
    }

    fn matches(self, keystroke: &Keystroke, layout: &dyn Fn(u16) -> Option<char>) -> bool {
        let held = keystroke.modifiers;
        held.control == self.ctrl
            && held.shift == self.shift
            && held.alt == self.alt
            && held.platform == self.win
            && key_name(self.key, layout).is_some_and(|name| name == keystroke.key)
    }
}

#[derive(Clone, Default, Serialize)]
pub struct Shortcuts(BTreeMap<Command, Shortcut>);

impl<'de> Deserialize<'de> for Shortcuts {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let saved = BTreeMap::<String, serde_json::Value>::deserialize(deserializer)?;
        Ok(Self(
            saved
                .into_iter()
                .filter_map(|(command, shortcut)| {
                    let command: Command = serde_json::from_value(serde_json::Value::String(command)).ok()?;
                    let shortcut: Shortcut = serde_json::from_value(shortcut).ok()?;
                    (command.editable() && shortcut.key != 0).then_some((command, shortcut))
                })
                .collect(),
        ))
    }
}

impl Shortcuts {
    pub fn get(&self, command: Command) -> Shortcut {
        self.0.get(&command).copied().unwrap_or_else(|| command.default_shortcut())
    }

    pub fn set(&mut self, command: Command, shortcut: Shortcut) {
        if shortcut == command.default_shortcut() {
            self.0.remove(&command);
        } else {
            self.0.insert(command, shortcut);
        }
    }

    pub fn customized(&self, command: Command) -> bool {
        self.0.contains_key(&command)
    }

    pub fn owner(&self, shortcut: Shortcut) -> Option<Command> {
        Command::ALL.into_iter().find(|command| self.get(*command) == shortcut || command.alias() == Some(shortcut))
    }

    pub fn find(&self, keystroke: &Keystroke, layout: &dyn Fn(u16) -> Option<char>) -> Option<Command> {
        Command::ALL.into_iter().filter(|command| command.scope() == Scope::Window).find(|command| {
            self.get(*command).matches(keystroke, layout)
                || command.alias().is_some_and(|alias| alias.matches(keystroke, layout))
        })
    }

    pub fn system(&self) -> Vec<(Command, Shortcut)> {
        Command::ALL
            .into_iter()
            .filter(|command| command.scope() == Scope::System)
            .map(|command| (command, self.get(command)))
            .collect()
    }
}

#[cfg(test)]
mod tests;
