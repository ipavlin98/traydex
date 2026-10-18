use super::{GRID, Probe};
use crate::{helper, input, wait, win};
use anyhow::{Result, bail};
use std::time::Duration;
use windows::Win32::Foundation::{HWND, POINT};

impl Probe<'_> {
    pub(super) fn on_screen(&self) -> Result<()> {
        let panel = self.panel()?;
        let client = win::client(panel);
        let (screen, _) = win::monitor(POINT { x: (client.left + client.right) / 2, y: client.top });
        if client.left < screen.left
            || client.top < screen.top
            || client.right > screen.right
            || client.bottom > screen.bottom
        {
            bail!(
                "the window goes off screen: window x {}..{}, y {}..{}, screen x {}..{}, y {}..{}",
                client.left,
                client.right,
                client.top,
                client.bottom,
                screen.left,
                screen.right,
                screen.top,
                screen.bottom
            );
        }
        Ok(())
    }

    pub(super) fn occluders(&self, panel: HWND) -> Vec<String> {
        let client = win::client(panel);
        let mut found: Vec<String> = Vec::new();
        for row in 0..GRID {
            for column in 0..GRID {
                let point = POINT {
                    x: client.left + 2 + (client.right - client.left - 4) * column / (GRID - 1),
                    y: client.top + 2 + (client.bottom - client.top - 4) * row / (GRID - 1),
                };
                let root = win::root_at(point);
                if root != panel {
                    let name = format!("«{}» ({})", win::title(root), win::class(root));
                    if !found.contains(&name) {
                        found.push(name);
                    }
                }
            }
        }
        found
    }

    pub(super) fn front(&self) -> Result<()> {
        let panel = self.panel()?;
        if !win::visible(panel) {
            bail!("the panel is hidden, there's nothing to bring to the front");
        }
        if !self.occluders(panel).is_empty() {
            win::set_topmost(panel);
            std::thread::sleep(Duration::from_millis(100));
        }
        let covered = self.occluders(panel);
        if !covered.is_empty() {
            bail!("the window is covered even after making it topmost: {}", covered.join(", "));
        }
        if win::foreground() != panel {
            let helper = self.ctx.layout.helper;
            input::click(POINT { x: (helper.left + helper.right) / 2, y: (helper.top + helper.bottom) / 2 })?;
            wait("the helper window became active", Duration::from_secs(3), || {
                (win::foreground() == helper::window()).then_some(())
            })?;
            win::set_foreground(panel);
            wait("the panel became active", Duration::from_secs(3), || (win::foreground() == panel).then_some(()))?;
        }
        Ok(())
    }

    pub(super) fn ready(&self) -> Result<()> {
        if !self.input {
            return Ok(());
        }
        let panel = self.panel()?;
        if win::visible(panel) && (win::foreground() != panel || !self.occluders(panel).is_empty()) {
            self.front()?;
        }
        Ok(())
    }

    pub(super) fn focus(&self, panel: HWND) -> String {
        let foreground = win::foreground();
        if foreground == panel {
            "panel".into()
        } else if self.input && foreground == helper::window() {
            "helper window".into()
        } else {
            format!("foreign window \"{}\" ({})", win::title(foreground), win::class(foreground))
        }
    }
}
