use super::{Action, ENGINES, Kind, Plugin, Presence, Slot, Tool, plugin};
use crate::store::Storage;
use crate::system::Services;
use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::{fs, io};

pub const MAX_TILES: usize = 36;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct TileId(u32);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Scan(u64);

pub struct Tile {
    pub id: TileId,
    pub action: Action,
}

pub struct Tiles {
    storage: Storage,
    list: Vec<Tile>,
    next: u32,
    tools: HashMap<Tool, Presence>,
    plugins: Vec<Arc<Plugin>>,
    scan: u64,
}

impl Tiles {
    pub fn new(storage: Storage) -> Self {
        Self { storage, list: Vec::new(), next: 0, tools: HashMap::new(), plugins: Vec::new(), scan: 0 }
    }

    pub fn load(&mut self, services: &dyn Services) -> Result<()> {
        self.plugins = plugin::scan(&self.storage);
        let actions = load(&self.storage, services)?;
        self.replace(actions);
        Ok(())
    }

    pub fn reset(&mut self, services: &dyn Services) -> Result<()> {
        let actions = reset(&self.storage, services)?;
        self.replace(actions);
        Ok(())
    }

    pub fn save(&self) -> Result<()> {
        let actions: Vec<Action> = self.list.iter().map(|tile| tile.action.clone()).collect();
        save(&self.storage, &actions)
    }

    fn replace(&mut self, actions: Vec<Action>) {
        self.list.clear();
        for action in actions {
            self.insert(action);
        }
    }

    fn insert(&mut self, mut action: Action) -> TileId {
        if let Kind::Custom(custom) = &mut action.kind {
            custom.attach(&self.plugins);
        }
        let id = TileId(self.next);
        self.next += 1;
        self.list.push(Tile { id, action });
        id
    }

    pub fn iter(&self) -> impl Iterator<Item = &Tile> {
        self.list.iter()
    }

    pub fn actions(&self) -> impl Iterator<Item = &Tile> {
        self.list.iter().filter(|tile| !tile.action.cancels())
    }

    pub fn len(&self) -> usize {
        self.actions().count()
    }

    pub fn is_empty(&self) -> bool {
        self.actions().next().is_none()
    }

    pub fn get(&self, id: TileId) -> Option<&Action> {
        self.list.iter().find(|tile| tile.id == id).map(|tile| &tile.action)
    }

    pub fn get_mut(&mut self, id: TileId) -> Option<&mut Action> {
        self.list.iter_mut().find(|tile| tile.id == id).map(|tile| &mut tile.action)
    }

    pub fn position(&self, id: TileId) -> Option<usize> {
        self.list.iter().position(|tile| tile.id == id)
    }

    pub fn find(&self, slot: &Slot) -> Option<TileId> {
        self.list.iter().find(|tile| tile.action.slot() == *slot).map(|tile| tile.id)
    }

    pub fn taken(&self, slot: &Slot, except: Option<TileId>) -> bool {
        self.list.iter().any(|tile| Some(tile.id) != except && tile.action.slot() == *slot)
    }

    pub fn full(&self) -> bool {
        self.len() >= MAX_TILES
    }

    pub fn add(&mut self, action: Action) -> Option<TileId> {
        if self.full() || self.taken(&action.slot(), None) {
            return None;
        }
        let cancel_last = self.list.last().is_some_and(|tile| tile.action.cancels());
        let id = self.insert(action);
        if cancel_last && let Some(before) = self.list.len().checked_sub(2) {
            self.list.swap(before, before + 1);
        }
        Some(id)
    }

    pub fn move_to(&mut self, id: TileId, target: TileId) -> bool {
        let (Some(from), Some(to)) = (self.position(id), self.position(target)) else {
            return false;
        };
        if from == to {
            return false;
        }
        let tile = self.list.remove(from);
        self.list.insert(to, tile);
        true
    }

    pub fn remove(&mut self, id: TileId) -> Option<Action> {
        let index = self.list.iter().position(|tile| tile.id == id && !tile.action.cancels())?;
        Some(self.list.remove(index).action)
    }

    pub fn detect_tools(storage: &Storage, services: &dyn Services) -> HashMap<Tool, Presence> {
        Tool::ALL.into_iter().map(|tool| (tool, tool.presence(storage, services))).collect()
    }

    pub fn begin_scan(&mut self) -> Scan {
        self.scan += 1;
        Scan(self.scan)
    }

    pub fn finish_scan(&mut self, scan: Scan, tools: HashMap<Tool, Presence>, plugins: Vec<Arc<Plugin>>) -> bool {
        if scan != Scan(self.scan) {
            return false;
        }
        self.tools = tools;
        self.set_plugins(plugins);
        true
    }

    pub fn plugins(&self) -> &[Arc<Plugin>] {
        &self.plugins
    }

    pub fn set_plugins(&mut self, plugins: Vec<Arc<Plugin>>) {
        self.scan += 1;
        self.plugins = plugins;
        for tile in &mut self.list {
            if let Kind::Custom(custom) = &mut tile.action.kind {
                custom.attach(&self.plugins);
            }
        }
    }

    pub fn presence(&self, tool: Tool) -> Presence {
        self.tools.get(&tool).copied().unwrap_or(Presence::Missing)
    }

    pub fn ready(&self, action: &Action) -> bool {
        action.ready() && action.tools().iter().all(|tool| self.presence(*tool).ready())
    }
}

fn location(storage: &Storage) -> PathBuf {
    storage.root().join("actions.json")
}

pub(super) fn load(storage: &Storage, system: &dyn Services) -> Result<Vec<Action>> {
    let path = location(storage);
    let data = match fs::read(&path) {
        Ok(data) => data,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let actions = defaults(system);
            if let Err(error) = save(storage, &actions) {
                log::warn!("Couldn't save the default tiles: {error:#}");
            }
            return Ok(actions);
        }
        Err(error) => return Err(error.into()),
    };
    let mut actions: Vec<Action> = serde_json::from_slice(&data).map_err(|error| {
        if let Err(backup) = fs::copy(&path, storage.root().join("actions.broken.json")) {
            log::error!("Couldn't back up the broken tiles: {backup}");
        }
        anyhow::Error::from(error)
    })?;
    let mut seen = Vec::new();
    actions.retain(|action| {
        let slot = action.slot();
        let fresh = !seen.contains(&slot);
        seen.push(slot);
        fresh
    });
    if actions.len() < seen.len() {
        log::info!("Removed {} duplicate tiles", seen.len() - actions.len());
        save(storage, &actions)?;
    }
    if !actions.iter().any(Action::cancels) {
        actions.push(Action::new(Kind::Cancel));
    }
    Ok(actions)
}

pub(super) fn save(storage: &Storage, actions: &[Action]) -> Result<()> {
    let temp = storage.root().join("actions.tmp");
    fs::write(&temp, serde_json::to_vec_pretty(actions)?)?;
    fs::rename(temp, location(storage))?;
    Ok(())
}

fn defaults(system: &dyn Services) -> Vec<Action> {
    let desktop = system
        .desktop_folder()
        .map(|folder| Action { name: system.display_name(&folder), ..Action::new(Kind::Copy { folder }) });
    desktop
        .into_iter()
        .chain([
            Action::new(Kind::Zip),
            Action::new(Kind::Png),
            Action::new(Kind::Search { url: ENGINES[0].into() }),
            Action::new(Kind::Cancel),
        ])
        .collect()
}

pub(super) fn reset(storage: &Storage, system: &dyn Services) -> Result<Vec<Action>> {
    let actions = defaults(system);
    save(storage, &actions)?;
    Ok(actions)
}
