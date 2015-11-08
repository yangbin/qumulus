//! Zone registry, dispatches commands and spawns Zones

use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

pub struct Manager {
    active: RwLock<BTreeMap<String, bool>>
}

impl Manager {
    pub fn new() -> Arc<Manager> {
        Arc::new(Manager { active: RwLock::new(BTreeMap::new()) })
    }

    pub fn load(&self, zone_name: &str) {
        if self.zone_loaded(zone_name) {
            return;
        }

        let mut active = self.active.write().unwrap();

        active.insert(zone_name.to_string(), true);
    }

    pub fn zone_loaded(&self, zone_name: &str) -> bool {
        let active = self.active.read().unwrap();

        return active.contains_key(zone_name);
    }
}
