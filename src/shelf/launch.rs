use super::{Shelf, Target};
use crate::actions::TileId;
use crate::i18n::tr;
use crate::store::{Payload, TileInput};
use gpui::*;

impl Shelf {
    pub fn launch(&mut self, tile: TileId, cx: &mut Context<Self>) {
        match self.store.settings().tile_input {
            TileInput::Clipboard => {
                let storage = self.store.storage().clone();
                self.after_update_then(
                    move |desktop| desktop.paste(&storage),
                    move |shelf, result, cx| match result {
                        Ok(payload) => shelf.perform(tile, payload, cx),
                        Err(error) => shelf.fail(error.to_string(), cx),
                    },
                    cx,
                );
            }
            TileInput::Selection => match Payload::of(&self.selected_items()) {
                Some(payload) => self.perform(tile, payload, cx),
                None => self.fail(tr().select_first, cx),
            },
            TileInput::Picker => {
                let services = self.services.clone();
                let title = tr().pick_files;
                self.dialog(
                    cx,
                    move || services.pick_files(title),
                    move |shelf, result, cx| match result {
                        Ok(paths) if paths.is_empty() => {}
                        Ok(paths) => shelf.perform(tile, Payload::Paths(paths), cx),
                        Err(error) => shelf.fail(error.to_string(), cx),
                    },
                );
            }
        }
    }

    pub(super) fn dropped_back(&mut self, cx: &mut Context<Self>) {
        let target = match self.dropped_target() {
            Some(Target::Tile(tile)) if !self.refuses(tile) => Some(tile),
            Some(Target::Cancel) => {
                log::debug!("Shelf drag dropped onto the cancel tile");
                None
            }
            _ => None,
        };
        let items = self.session.returned();
        if let (Some(tile), Some(payload)) = (target, Payload::of(&items)) {
            self.perform(tile, payload, cx);
        }
        cx.notify();
    }

    pub(super) fn lend_ended(&mut self, cx: &mut Context<Self>) {
        self.session.lend_ended();
        cx.notify();
    }
}
