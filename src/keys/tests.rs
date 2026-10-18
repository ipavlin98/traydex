use super::{Command, Scope, Shortcut, Shortcuts};
use crate::i18n::{Language, tr};
use crate::testing::{language, us_layout};
use gpui::Keystroke;
use windows::Win32::UI::Input::KeyboardAndMouse::*;

fn key(text: &str) -> Keystroke {
    Keystroke::parse(text).expect("keystroke")
}

fn find(shortcuts: &Shortcuts, text: &str) -> Option<Command> {
    shortcuts.find(&key(text), &us_layout)
}

fn russian(key: u16) -> Option<char> {
    let letters = [(VK_P, 'з'), (VK_K, 'л'), (VK_A, 'ф'), (VK_X, 'ч'), (VK_C, 'с'), (VK_V, 'м')];
    letters
        .iter()
        .find(|(vk, _)| vk.0 == key)
        .map(|(_, c)| *c)
        .or_else(|| us_layout(key).filter(|c| !c.is_ascii_alphabetic()))
}

fn shortcut(ctrl: bool, shift: bool, alt: bool, win: bool, key: VIRTUAL_KEY) -> Shortcut {
    Shortcut { ctrl, shift, alt, win, key: key.0 }
}

#[test]
fn default_window_shortcuts_dispatch_their_commands() {
    let shortcuts = Shortcuts::default();
    let expected = [
        ("ctrl-p", Command::Pin),
        ("ctrl-k", Command::Actions),
        ("ctrl-o", Command::Settings),
        ("ctrl-e", Command::Storage),
        ("ctrl-g", Command::Layout),
        ("ctrl-v", Command::Paste),
        ("ctrl-c", Command::Copy),
        ("ctrl-a", Command::SelectAll),
        ("enter", Command::Open),
        ("ctrl-enter", Command::Reveal),
        ("space", Command::Preview),
        ("x", Command::Mark),
        ("p", Command::Stick),
        ("delete", Command::Remove),
        ("backspace", Command::Remove),
        ("ctrl-delete", Command::Clear),
    ];
    for (text, command) in expected {
        assert_eq!(find(&shortcuts, text), Some(command), "{text}");
    }
}

#[test]
fn modifiers_must_match_exactly() {
    let shortcuts = Shortcuts::default();
    for text in
        ["shift-p", "ctrl-shift-p", "ctrl-alt-p", "win-p", "shift-enter", "alt-x", "shift-delete", "ctrl-backspace"]
    {
        assert_eq!(find(&shortcuts, text), None, "{text}");
    }
}

#[test]
fn system_command_is_not_dispatched_inside_window() {
    let shortcuts = Shortcuts::default();
    assert_eq!(find(&shortcuts, "ctrl-shift-space"), None);
    assert_eq!(shortcuts.system(), vec![(Command::Toggle, Command::Toggle.default_shortcut())]);
}

#[test]
fn letter_shortcuts_follow_the_active_layout() {
    let shortcuts = Shortcuts::default();
    assert_eq!(shortcuts.find(&key("ctrl-з"), &russian), Some(Command::Pin));
    assert_eq!(shortcuts.find(&key("ctrl-p"), &russian), None);
    assert_eq!(shortcuts.find(&key("ч"), &russian), Some(Command::Mark));
    assert_eq!(shortcuts.find(&key("enter"), &russian), Some(Command::Open));
}

#[test]
fn keys_without_layout_character_never_match() {
    let shortcuts = Shortcuts::default();
    assert_eq!(shortcuts.find(&key("ctrl-p"), &|_| None), None);
    assert_eq!(shortcuts.find(&key("delete"), &|_| None), Some(Command::Remove));
}

#[test]
fn custom_shortcut_with_function_key_and_win_modifier_matches() {
    let mut shortcuts = Shortcuts::default();
    shortcuts.set(Command::Toggle, shortcut(false, false, false, true, VK_F5));
    assert_eq!(shortcuts.system(), vec![(Command::Toggle, shortcut(false, false, false, true, VK_F5))]);
    assert!(Shortcut::plain(VK_F5).matches(&key("f5"), &us_layout));
    assert!(shortcut(false, false, false, true, VK_F5).matches(&key("win-f5"), &us_layout));
    assert!(shortcut(false, false, true, false, VK_OEM_2).matches(&key("alt-/"), &us_layout));
    assert!(!Shortcut::plain(VK_F5).matches(&key("f6"), &us_layout));
}

#[test]
fn default_shortcuts_do_not_conflict() {
    let all: Vec<(Command, Shortcut)> = Command::ALL
        .into_iter()
        .map(|command| (command, command.default_shortcut()))
        .chain([(Command::Remove, Shortcut::plain(VK_BACK))])
        .collect();
    for (index, (first, a)) in all.iter().enumerate() {
        for (second, b) in &all[index + 1..] {
            assert!(a != b || first == second, "{first:?} and {second:?} share {a:?}");
        }
    }
}

#[test]
fn only_toggle_is_editable_and_global() {
    for command in Command::ALL {
        assert_eq!(command.editable(), command == Command::Toggle, "{command:?}");
        assert_eq!(command.scope() == Scope::System, command == Command::Toggle, "{command:?}");
    }
    assert!(Command::Toggle.default_shortcut().fits(Scope::System));
}

#[test]
fn global_shortcuts_need_a_modifier_or_function_key() {
    assert!(!Shortcut::plain(VK_A).fits(Scope::System));
    assert!(!shortcut(false, true, false, false, VK_A).fits(Scope::System));
    assert!(shortcut(true, false, false, false, VK_A).fits(Scope::System));
    assert!(shortcut(false, false, true, false, VK_A).fits(Scope::System));
    assert!(shortcut(false, false, false, true, VK_A).fits(Scope::System));
    assert!(Shortcut::plain(VK_F1).fits(Scope::System));
    assert!(Shortcut::plain(VK_F24).fits(Scope::System));
    assert!(Shortcut::plain(VK_A).fits(Scope::Window));
}

#[test]
fn escape_alone_cancels_recording() {
    assert!(Shortcut::plain(VK_ESCAPE).is_cancel());
    assert!(!shortcut(true, false, false, false, VK_ESCAPE).is_cancel());
    assert!(!Shortcut::plain(VK_A).is_cancel());
}

#[test]
fn labels_list_modifiers_in_fixed_order() {
    let all = shortcut(true, true, true, true, VK_SPACE);
    assert_eq!(all.labels().len(), 5);
    assert_eq!(all.text(), "Ctrl+Shift+Alt+Win+Space");
    assert_eq!(shortcut(true, true, false, false, VK_SPACE).text(), "Ctrl+Shift+Space");
    assert_eq!(shortcut(true, false, false, false, VIRTUAL_KEY(0)).text(), "Ctrl");
    assert!(Shortcut::default().labels().is_empty());
}

#[test]
fn key_labels_are_readable() {
    let label = |key: VIRTUAL_KEY| Shortcut::plain(key).text();
    assert_eq!(label(VK_A), "A");
    assert_eq!(label(VK_7), "7");
    assert_eq!(label(VK_NUMPAD5), "Num 5");
    assert_eq!(label(VK_F13), "F13");
    assert_eq!(label(VK_OEM_2), "/");
    assert_eq!(label(VK_OEM_5), "\\");
    assert_eq!(label(VK_ADD), "Num +");
    assert_eq!(label(VK_UP), "↑");
    assert_eq!(label(VK_PRIOR), "PgUp");
    assert_eq!(label(VK_DELETE), "Del");
    assert_eq!(label(VK_RETURN), tr().enter_key);
    assert_eq!(label(VK_ESCAPE), tr().escape_key);
}

#[test]
fn enter_and_escape_labels_follow_language() {
    for language_ in Language::ALL {
        let _switched = language(language_);
        assert_eq!(Shortcut::plain(VK_RETURN).text(), tr().enter_key);
        assert!(!tr().enter_key.is_empty());
    }
}

#[test]
fn setting_default_removes_customization() {
    let mut shortcuts = Shortcuts::default();
    let custom = shortcut(true, false, true, false, VK_T);
    shortcuts.set(Command::Toggle, custom);
    assert!(shortcuts.customized(Command::Toggle));
    assert_eq!(shortcuts.get(Command::Toggle), custom);
    shortcuts.set(Command::Toggle, Command::Toggle.default_shortcut());
    assert!(!shortcuts.customized(Command::Toggle));
    assert_eq!(shortcuts.get(Command::Toggle), Command::Toggle.default_shortcut());
}

#[test]
fn owner_reports_conflicting_command() {
    let mut shortcuts = Shortcuts::default();
    assert_eq!(shortcuts.owner(shortcut(true, false, false, false, VK_P)), Some(Command::Pin));
    assert_eq!(shortcuts.owner(Shortcut::plain(VK_BACK)), Some(Command::Remove));
    assert_eq!(shortcuts.owner(shortcut(true, false, true, false, VK_T)), None);
    shortcuts.set(Command::Toggle, shortcut(true, false, true, false, VK_T));
    assert_eq!(shortcuts.owner(shortcut(true, false, true, false, VK_T)), Some(Command::Toggle));
    assert_eq!(shortcuts.owner(Command::Toggle.default_shortcut()), None);
}

#[test]
fn default_shortcuts_serialize_to_empty_map() {
    assert_eq!(serde_json::to_string(&Shortcuts::default()).expect("json"), "{}");
}

#[test]
fn customized_shortcuts_round_trip() {
    let mut shortcuts = Shortcuts::default();
    shortcuts.set(Command::Toggle, shortcut(false, true, true, false, VK_F9));
    let json = serde_json::to_string(&shortcuts).expect("json");
    let restored: Shortcuts = serde_json::from_str(&json).expect("restore");
    assert_eq!(restored.get(Command::Toggle), shortcut(false, true, true, false, VK_F9));
    assert!(restored.customized(Command::Toggle));
}

#[test]
fn saved_shortcuts_drop_invalid_and_non_editable_entries() {
    let json = r#"{
        "Pin": {"ctrl": true, "key": 81},
        "Bogus": {"ctrl": true, "key": 81},
        "Remove": "delete",
        "Toggle": {"alt": true, "key": 0}
    }"#;
    let restored: Shortcuts = serde_json::from_str(json).expect("restore");
    for command in Command::ALL {
        assert!(!restored.customized(command), "{command:?}");
        assert_eq!(restored.get(command), command.default_shortcut());
    }
}

#[test]
fn saved_shortcut_with_missing_fields_uses_defaults() {
    let restored: Shortcuts = serde_json::from_str(r#"{"Toggle": {"alt": true, "key": 65}}"#).expect("restore");
    assert_eq!(restored.get(Command::Toggle), shortcut(false, false, true, false, VK_A));
    let invalid: Shortcuts = serde_json::from_str(r#"{"Toggle": "ctrl+a"}"#).expect("restore");
    assert!(!invalid.customized(Command::Toggle));
}

#[test]
fn malformed_shortcut_map_is_an_error() {
    assert!(serde_json::from_str::<Shortcuts>("[]").is_err());
    assert!(serde_json::from_str::<Shortcuts>("42").is_err());
}

#[test]
fn every_command_has_a_label_in_every_language() {
    for language_ in Language::ALL {
        let _switched = language(language_);
        for command in Command::ALL {
            assert!(!command.label().trim().is_empty(), "{command:?}");
        }
    }
}
