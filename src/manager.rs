//! Zone registry, dispatches commands and spawns Zones

use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};
use std::sync::mpsc;

use serde_json::Value;

use command::Command;
use path::Path;
use zone::Zone;

pub struct Manager {
    active: RwLock<BTreeMap<Path, Arc<Zone>>>
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

        let zone = Arc::new(Zone::new(path.clone()));

        active.insert(path, zone);
    }

    pub fn zone_loaded(&self, path: &Path) -> bool {
        let active = self.active.read().unwrap();

        active.contains_key(path)
    }

    pub fn dispatch(&self, command: Command, tx: &mpsc::Sender<String>) -> Value {
        let zone = self.find_nearest(&command.path);

        zone.dispatch(command, tx)
    }

    pub fn find_nearest(&self, path: &Path) -> Arc<Zone> {
        let active = self.active.read().unwrap();

        // TODO actually find the nearest zone
        let zone = active.get(&Path::new(vec![]));

        match zone {
            Some(zone) => zone.clone(),
            None => {
                unimplemented!();
            }
        }
    }
}
