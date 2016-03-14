//! Owns a subtree of entire tree, also unit of concurrency

use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

use serde_json::Value;

use command::Command;
use command::Call;
use node::Node;
use node::Vis;
use path::Path;

// TODO: Consider Zone as a thread

#[derive(Debug)]
pub struct ZoneData {
    node: Node, // Mergeable data for this Zone
    vis: Vis    // Visibility of this Zone through ancestors
}

pub struct Zone {
    path: Path,            // Path to this Zone
    data: RwLock<ZoneData> // 'Atomic' data for this Zone
    // TODO: size: u64,
    // TODO: prefixes: Option<BTreeMap<String, Node>>
    // TODO: replicas: Vec<Replicas>
    // TODO: listeners: Vec<Listeners>
}

impl Zone {
    pub fn new(path: Path) -> Zone {
        Zone {
            path: path,
            data: RwLock::new(ZoneData {
                node: Node::expand(&Value::Null, 0),
                vis: Default::default()
            })
        }
    }

    pub fn dispatch(&self, command: Command) -> Value {
        match command.call {
            Call::Bind => unimplemented!(),
            Call::Read => unimplemented!(),
            Call::Write => {
                self.write(command.path, command.timestamp, command.params);
                Value::Null
            }
        }
    }

    /// Writes value(s) to the node at `path` at time `ts`
    pub fn write(&self, path: Path, ts: u64, value: Value) {
        // TODO verify path
        let mut diff = Node::expand(&value, ts);

        let mut data = self.data.write().unwrap();

        {
            let ZoneData { ref mut node, ref mut vis } = *data;

            let (updates, external) = node.merge(&mut diff, *vis, *vis);
        }

        println!("Data written, node is now: {:?}", data.node);

        // TODO: updates goes to notify
        // TODO: external goes to external nodes
        // TODO: diff goes to replicas
    }
}
