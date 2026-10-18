use super::{Confirm, Screen, Shelf};
use crate::actions::{self, Action, Custom, Input, Kind, Plugin, Slot, TileId};
use crate::i18n::{fill, tr};
use crate::input::{Changed, Submitted, TextInput};
use gpui::*;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::Arc;

type SettingInputs = (Vec<(String, Entity<TextInput>)>, Vec<Subscription>);

pub(super) fn is_bundle(path: &Path) -> bool {
    path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case(actions::BUNDLE))
}

impl Shelf {
    pub fn plugins(&self) -> &[Arc<Plugin>] {
        self.tiles.plugins()
    }

    pub(super) fn install_plugins(&mut self, bundles: Vec<PathBuf>, cx: &mut Context<Self>) {
        let storage = self.store.storage().clone();
        self.background(
            cx,
            move || {
                let installed: Vec<_> =
                    bundles.iter().map(|bundle| actions::install_plugin(&storage, bundle)).collect();
                Ok((installed, actions::scan_plugins(&storage)))
            },
            |shelf, result, cx| {
                let Ok((installed, plugins)) = result else {
                    return;
                };
                shelf.tiles.set_plugins(plugins);
                let (mut added, mut errors) = (Vec::new(), Vec::new());
                for outcome in installed {
                    match outcome {
                        Ok(plugin) => {
                            log::info!("Plugin \"{}\" installed", plugin.id());
                            added.push(plugin);
                        }
                        Err(error) => errors.push(format!("{error:#}")),
                    }
                }
                match added.as_slice() {
                    [] => {}
                    [plugin] => {
                        shelf.configure_plugin(plugin.id(), cx);
                        shelf.show_toast(fill(tr().plugin_installed, plugin.name()), cx);
                    }
                    many => {
                        shelf.show(Screen::Actions, cx);
                        shelf.show_toast(fill(tr().plugins_installed, many.len()), cx);
                    }
                }
                for error in errors {
                    shelf.fail(error, cx);
                }
                cx.notify();
            },
        );
    }

    pub(crate) fn plugin(&self, id: &str) -> Option<&Arc<Plugin>> {
        self.plugins().iter().find(|plugin| plugin.id() == id)
    }

    pub fn ask_uninstall(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.plugin(id).is_some() {
            self.confirming = Some(Confirm::Uninstall(id.to_string()));
            cx.notify();
        }
    }

    pub(super) fn uninstall_plugin(&mut self, id: &str, cx: &mut Context<Self>) {
        self.confirming = None;
        let Some(plugin) = self.plugin(id).cloned() else {
            return cx.notify();
        };
        let storage = self.store.storage().clone();
        let removed = plugin.clone();
        self.background(
            cx,
            move || {
                actions::uninstall_plugin(&removed)?;
                Ok(actions::scan_plugins(&storage))
            },
            move |shelf, result, cx| {
                let plugins = match result {
                    Ok(plugins) => plugins,
                    Err(error) => return shelf.fail(format!("{error:#}"), cx),
                };
                log::info!("Plugin \"{}\" uninstalled", plugin.id());
                if let Some(tile) = shelf.tiles.find(&Slot::Custom(plugin.id().to_string())) {
                    shelf.remove_action(tile, cx);
                }
                shelf.tiles.set_plugins(plugins);
                shelf.show_toast(fill(tr().plugin_uninstalled, plugin.name()), cx);
                cx.notify();
            },
        );
        cx.notify();
    }

    fn configure_plugin(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(plugin) = self.plugin(id).cloned() else {
            return self.show(Screen::Actions, cx);
        };
        let action = Action::new(Kind::Custom(Custom::new(plugin)));
        let tile = match self.tiles.find(&action.slot()) {
            Some(tile) => Some(tile),
            None if self.tiles.full() => None,
            None => self.add_action(action, cx),
        };
        match tile {
            Some(tile) => self.edit(tile, cx),
            None => self.show(Screen::Actions, cx),
        }
    }

    pub(super) fn setting_inputs(&self, tile: TileId, action: &Action, cx: &mut Context<Self>) -> SettingInputs {
        let (mut inputs, mut subscriptions) = (Vec::new(), Vec::new());
        let Kind::Custom(custom) = &action.kind else {
            return (inputs, subscriptions);
        };
        let Some(plugin) = custom.plugin() else {
            return (inputs, subscriptions);
        };
        for setting in plugin.settings().iter().filter(|setting| setting.input != Input::Toggle) {
            let (key, value) = (setting.key.clone(), custom.text(setting));
            let secret = setting.input == Input::Secret;
            let input = cx.new(|cx| TextInput::new(value, setting.placeholder.clone(), secret, cx));
            let changed = key.clone();
            subscriptions.push(cx.subscribe(&input, move |shelf, input, _: &Changed, cx| {
                let text = input.read(cx).text().to_string();
                shelf.set_value(tile, &changed, Value::String(text), cx);
            }));
            subscriptions.push(cx.subscribe(&input, |shelf, _, _: &Submitted, cx| shelf.leave_editor(cx)));
            inputs.push((key, input));
        }
        (inputs, subscriptions)
    }

    pub fn set_value(&mut self, tile: TileId, key: &str, value: Value, cx: &mut Context<Self>) {
        let Some(Kind::Custom(custom)) = self.tiles.get_mut(tile).map(|action| &mut action.kind) else {
            return;
        };
        custom.set(key, value);
        self.save_tiles(cx);
    }

    pub fn pick_value(&mut self, tile: TileId, key: String, folders: bool, cx: &mut Context<Self>) {
        let title = if folders { tr().pick_folder } else { tr().pick_file };
        let services = self.services.clone();
        self.dialog(
            cx,
            move || services.pick(title, folders),
            move |shelf, result, cx| match result {
                Ok(Some(path)) => {
                    let text = path.display().to_string();
                    let input = shelf
                        .editor()
                        .filter(|editor| editor.tile == tile)
                        .and_then(|editor| editor.settings.iter().find(|(known, _)| *known == key))
                        .map(|(_, input)| input.clone());
                    if let Some(input) = input {
                        input.update(cx, |input, cx| input.set_text(text.clone(), cx));
                    }
                    shelf.set_value(tile, &key, Value::String(text), cx);
                }
                Ok(None) => {}
                Err(error) => shelf.fail(error.to_string(), cx),
            },
        );
    }
}
