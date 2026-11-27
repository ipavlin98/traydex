use super::Shelf;
use crate::input::TextInput;
use crate::theme::{Catalog, Entry, Hex, Hsv, ThemeChoice, ThemeColors};
use gpui::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Paint {
    Accent,
    Background,
    Text,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Picking {
    pub paint: Paint,
    pub hsv: Hsv,
}

pub struct Themes {
    catalog: Catalog,
    picking: Option<Picking>,
    dismissed: Option<Paint>,
    hex: Entity<TextInput>,
}

impl Themes {
    pub(super) fn new(hex: Entity<TextInput>) -> Self {
        Self { catalog: Catalog::default(), picking: None, dismissed: None, hex }
    }

    pub fn hex(&self) -> &Entity<TextInput> {
        &self.hex
    }

    pub fn catalog(&self) -> &Catalog {
        &self.catalog
    }

    pub fn picking(&self) -> Option<Picking> {
        self.picking
    }

    pub(super) fn close(&mut self) -> bool {
        self.picking.take().is_some()
    }
}

impl Shelf {
    pub(crate) fn themes(&self) -> &Themes {
        &self.themes
    }

    pub(crate) fn current_theme(&self) -> Option<&Entry> {
        self.themes.catalog.current(&self.store.settings().theme, self.appearance)
    }

    pub(crate) fn current_colors(&self) -> ThemeColors {
        self.current_theme().map(|entry| self.store.settings().theme_colors(entry.id())).unwrap_or_default()
    }

    pub(super) fn load_themes(&self, cx: &mut Context<Self>) {
        let storage = self.store.storage().clone();
        self.background(
            cx,
            move || Ok(Catalog::load(storage.theme_files())),
            |shelf, catalog, cx| match catalog {
                Ok(catalog) => {
                    shelf.themes.catalog = catalog;
                    shelf.apply_theme(cx);
                }
                Err(error) => log::warn!("Couldn't load the themes: {error:#}"),
            },
        );
    }

    pub fn set_theme(&mut self, choice: ThemeChoice, cx: &mut Context<Self>) {
        self.menu.close();
        self.themes.close();
        self.update_settings(|settings| settings.theme = choice, cx);
    }

    fn recolor(&mut self, change: impl FnOnce(&mut ThemeColors), cx: &mut Context<Self>) {
        let Some(id) = self.current_theme().map(|entry| entry.id().to_owned()) else { return };
        let mut colors = self.store.settings().theme_colors(&id);
        change(&mut colors);
        self.update_settings(|settings| settings.set_theme_colors(&id, colors), cx);
    }

    pub fn set_accent(&mut self, accent: Option<Hex>, cx: &mut Context<Self>) {
        self.themes.close();
        self.recolor(|colors| colors.accent = accent, cx);
    }

    pub fn set_background(&mut self, background: Option<Hex>, cx: &mut Context<Self>) {
        self.themes.close();
        self.recolor(|colors| (colors.background, colors.text) = (background, None), cx);
    }

    pub fn reset_text(&mut self, cx: &mut Context<Self>) {
        self.themes.close();
        self.recolor(|colors| colors.text = None, cx);
    }

    pub fn toggle_picker(&mut self, paint: Paint, cx: &mut Context<Self>) {
        if self.themes.dismissed.take() == Some(paint) {
            return;
        }
        if self.themes.picking.is_some_and(|picking| picking.paint == paint) {
            self.close_picker(cx);
            return;
        }
        self.menu.close();
        let theme = self.theme();
        let color = match paint {
            Paint::Accent => theme.accent,
            Paint::Background => theme.bg,
            Paint::Text => theme.text,
        };
        self.themes.picking = Some(Picking { paint, hsv: Hsv::of(color) });
        match paint {
            Paint::Accent | Paint::Background => self.pick_shade(Hsv::of(color), cx),
            Paint::Text => self.show_hex(cx),
        }
    }

    pub fn close_picker(&mut self, cx: &mut Context<Self>) {
        if self.themes.close() {
            cx.notify();
        }
    }

    pub fn dismiss_picker(&mut self, cx: &mut Context<Self>) {
        self.themes.dismissed = self.themes.picking.map(|picking| picking.paint);
        self.close_picker(cx);
    }

    pub(super) fn forget_dismissed_picker(&mut self) {
        self.themes.dismissed = None;
    }

    pub fn pick_shade(&mut self, hsv: Hsv, cx: &mut Context<Self>) {
        self.apply_shade(hsv, cx);
        self.show_hex(cx);
    }

    pub(super) fn hex_typed(&mut self, cx: &mut Context<Self>) {
        let typed = self.themes.hex.read(cx).text().trim().trim_start_matches('#').to_owned();
        let Some(color) = Hex::parse(&format!("#{typed}")) else { return };
        let Some(picking) = self.themes.picking else { return };
        if Hex::of(picking.hsv.color()) != color {
            self.apply_shade(Hsv::of(color.color()), cx);
        }
    }

    pub(super) fn hex_focused(&self, window: &Window, cx: &App) -> bool {
        self.themes.picking.is_some() && self.themes.hex.read(cx).focused(window)
    }

    fn show_hex(&mut self, cx: &mut Context<Self>) {
        let Some(picking) = self.themes.picking else { return };
        let text = String::from(Hex::of(picking.hsv.color())).to_uppercase();
        self.themes.hex.update(cx, |input, cx| input.set_text(text, cx));
        cx.notify();
    }

    fn apply_shade(&mut self, hsv: Hsv, cx: &mut Context<Self>) {
        let Some(picking) = self.themes.picking.as_mut() else { return };
        picking.hsv = hsv;
        let paint = picking.paint;
        let color = Some(Hex::of(hsv.color()));
        match paint {
            Paint::Accent => self.recolor(|colors| colors.accent = color, cx),
            Paint::Background => self.recolor(|colors| (colors.background, colors.text) = (color, None), cx),
            Paint::Text => self.recolor(|colors| colors.text = color, cx),
        }
        cx.notify();
    }

    pub fn open_themes(&self) {
        let folder = self.store.storage().themes();
        let services = self.services.clone();
        self.services.spawn(Box::new(move || {
            if let Err(error) = std::fs::create_dir_all(&folder) {
                log::warn!("Couldn't create {}: {error}", folder.display());
            }
            services.shell_open(&folder.to_string_lossy());
        }));
    }
}
