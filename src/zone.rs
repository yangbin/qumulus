//! Owns a subtree of entire tree, also unit of concurrency
//!
//! The Zone structure represents the subtree and is run as a thread.
//! ZoneHandle is the public interface to a single zone.

use std::collections::BTreeMap;
use std::sync::mpsc;
use std::sync::mpsc::Sender;
use std::thread;

use serde_json::Value;

use command::Command;
use command::Call;
use listener::Listener;
use node::Node;
use node::Vis;
use node::Update;
use path::Path;

#[derive(Debug)]
pub struct ZoneData {
    node: Node, // Mergeable data for this Zone
    vis: Vis    // Visibility of this Zone through ancestors
}

#[derive(Clone)]
pub struct ZoneHandle {
    tx: Sender<ZoneCommand>
}

struct ZoneCommand {
    command: Command,
    reply: Sender<Value>,
    listener: Sender<String>
}

pub struct Zone {
    path: Path,               // Path to this Zone
    data: ZoneData,           // 'Atomic' data for this Zone
    listeners: Vec<Listener>, // List of binds
    writes: u64               // Number of writes since last fragment check
    // TODO: size: u64,
    // TODO: prefixes: Option<BTreeMap<String, Node>>
    // TODO: replicas: Vec<Replicas>
}

impl ZoneHandle {
    pub fn dispatch(&self, command: Command, listener: &Sender<String>) -> Value {
        let (tx, rx) = mpsc::channel();

        let command = ZoneCommand { command: command, reply: tx, listener: listener.clone() };

        self.tx.send(command).unwrap();
        rx.recv().unwrap()
    }
}

impl Zone {
    pub fn spawn(path: &Path) -> ZoneHandle {
        let (tx, rx) = mpsc::channel();

        let zone = Zone::new(path);

        thread::spawn(move|| {
            zone.message_loop(rx);
        });

        ZoneHandle { tx: tx }
    }

    pub fn new(path: &Path) -> Zone {
        Zone {
            path: path.clone(),
            data: ZoneData {
                node: Node::expand(&Value::Null, 0),
                vis: match path.path.len() {
                    0 => Vis::new(1, 0),
                    _ => Default::default()
                }
            },
            listeners: vec![],
            writes: 0
        }
    }

    fn message_loop(mut self, rx: mpsc::Receiver<ZoneCommand>) {
        for cmd in rx {
            let result = self.dispatch(cmd.command, &cmd.listener);

            cmd.reply.send(result).unwrap(); // TODO: don't crash the Zone!
        }
    }

    pub fn dispatch(&mut self, command: Command, tx: &Sender<String>) -> Value {
        match command.call {
            Call::Bind => {
                self.bind(&command.path, tx)
            },
            Call::Kill => {
                self.kill(&command.path, command.timestamp);
                Value::Null
            }
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
    pub fn bind(&mut self, path: &Path, tx: &Sender<String>) -> Value {
        // TODO verify path

        self.sub(path, tx);
        self.read(path)
    }

    /// Kill value(s)
    pub fn kill(&mut self, path: &Path, ts: u64) {
        let node = Node::delete(ts);

        let diff = node.prepend_path(&path.path);

        self.merge(diff);
        // TODO: externals goes to external nodes
        // TODO: diff goes to replicas
    }

    pub fn merge(&mut self, mut diff: Node) {
        let (update, _) = {
            let ZoneData { ref mut node, vis } = self.data;

            node.merge(&mut diff, vis, vis)
        };

        if let Some(update) = update {
            self.notify(&update);
            self.writes += 1;

            if self.writes >= 1000 {
                self.split_check();
            }
        }

        // TODO: externals goes to external nodes
        // TODO: diff goes to replicas
    }

    /// Read value(s)
    pub fn read(&self, path: &Path) -> Value {
        // TODO verify path

        let read = self.data.node.read(self.data.vis, path);

        let (update, _) = read;

        // TODO: return externals too

        update.map_or(Value::Null, |u| u.to_json())
    }

    /// Writes value(s) to the node at `path` at time `ts`
    pub fn write(&mut self, path: &Path, ts: u64, value: Value) {
        // TODO verify path
        let diff = Node::expand_from(&path.path[..], &value, ts);

        self.merge(diff);
    }

    fn notify(&self, update: &Update) {
        for listener in &self.listeners {
            listener.update(update).unwrap();
            // TODO: don't crash Zone; remove listener from list
        }
    }

    fn sub(&mut self, path: &Path, tx: &Sender<String>) {
        let listener = Listener::new(path, tx);

        self.listeners.push(listener);
    }

    fn split_check(&mut self) {
        println!("Checking if split needed, {} writes", self.writes);

        // TODO

        self.writes = 0;
    }
}
