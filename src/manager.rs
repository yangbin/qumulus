//! Zone registry, dispatches commands and spawns Zones

use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

use command::Command;
use path::Path;

pub struct Manager {
    active: RwLock<BTreeMap<Path, bool>>
}

impl Manager {
    pub fn new() -> Arc<Manager> {
        Arc::new(Manager { active: RwLock::new(BTreeMap::new()) })
    }

    pub fn load(&self, path: Path) {
        if self.zone_loaded(&path) {
            return;
        }

        let mut active = self.active.write().unwrap();

        active.insert(path, true);
    }

    pub fn zone_loaded(&self, path: &Path) -> bool {
        let active = self.active.read().unwrap();

        active.contains_key(path)
    }

    pub fn dispatch(&self, _command: Command) -> bool {
        false
    }
}
