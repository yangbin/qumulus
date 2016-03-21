//! Owns a subtree of entire tree, also unit of concurrency

use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};
use std::sync::mpsc::Sender;

use serde_json::Value;

use command::Command;
use command::Call;
use listener::Listener;
use node::Node;
use node::Vis;
use node::Update;
use path::Path;

// TODO: Consider Zone as a thread

#[derive(Debug)]
pub struct ZoneData {
    node: Node, // Mergeable data for this Zone
    vis: Vis    // Visibility of this Zone through ancestors
}

pub struct Zone {
    path: Path,                      // Path to this Zone
    data: RwLock<ZoneData>,          // 'Atomic' data for this Zone
    listeners: RwLock<Vec<Listener>> // List of binds
    // TODO: size: u64,
    // TODO: prefixes: Option<BTreeMap<String, Node>>
    // TODO: replicas: Vec<Replicas>
}

impl Zone {
    pub fn new(path: Path) -> Zone {
        Zone {
            path: path.clone(),
            data: RwLock::new(ZoneData {
                node: Node::expand(&Value::Null, 0),
                vis: match path.path.len() {
                    0 => Vis::new(1, 0),
                    _ => Default::default()
                }
            }),
            listeners: RwLock::new(vec![])
        }
    }

    pub fn dispatch(&self, command: Command, tx: &Sender<String>) -> Value {
        match command.call {
            Call::Bind => {
                self.bind(&command.path, tx)
            },
            Call::Read => {
                self.read(&command.path)
            },
            Call::Write => {
                self.write(&command.path, command.timestamp, command.params);
                Value::Null
            }
        }
    }

    /// Bind value(s)
    pub fn bind(&self, path: &Path, tx: &Sender<String>) -> Value {
        // TODO verify path

        self.sub(path, tx);
        self.read(path)
    }

    /// Read value(s)
    pub fn read(&self, path: &Path) -> Value {
        // TODO verify path

        let data = self.data.read().unwrap();

        let read = data.node.read(data.vis, path);

        let (update, _) = read;

        // TODO: return externals too

        update.map_or(Value::Null, |u| u.to_json())
    }

    /// Writes value(s) to the node at `path` at time `ts`
    pub fn write(&self, path: &Path, ts: u64, value: Value) {
        // TODO verify path
        let mut diff = Node::expand_from(&path.path[..], &value, ts);

        let mut data = self.data.write().unwrap();

        let (update, _) = {
            let ZoneData { ref mut node, ref mut vis } = *data;

            node.merge(&mut diff, *vis, *vis)
        };

        if let Some(update) = update {
            self.notify(&update);
        }
        // TODO: externals goes to external nodes
        // TODO: diff goes to replicas
    }

    fn notify(&self, update: &Update) {
        let listeners = self.listeners.read().unwrap();

        for listener in &*listeners {
            listener.update(update);
        }
    }

    fn sub(&self, path: &Path, tx: &Sender<String>) {
        let mut listeners = self.listeners.write().unwrap();

        let listener = Listener::new(path, tx);

        listeners.push(listener);
    }
}
