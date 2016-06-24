//! Owns a subtree of entire tree, also unit of concurrency
//!
//! The Zone structure represents the subtree and is run as a thread.
//! ZoneHandle is the public interface to a single zone.

use mioco;
use mioco::sync::mpsc::{channel, Receiver, Sender};
use serde_json::Value;

use command::Command;
use command::Call;
use delegate::delegate;
use listener::Listener;
use manager::ManagerHandle;
use node::{DelegatedMatch, Node, Update, Vis};
use path::Path;

#[derive(Debug)]
pub struct ZoneData {
    node: Node, // Mergeable data for this Zone
    vis: Vis    // Visibility of this Zone through ancestors
}

#[derive(Clone)]
pub struct ZoneHandle {
    tx: Sender<ZoneCall>
}

enum ZoneCall {
    UserCommand(UserCommand),
    Merge(Vis, Node),
    Size(Sender<usize>)
}

struct UserCommand {
    command: Command,
    reply: Sender<ZoneResult>,
    listener: Sender<String>
}

#[derive(Default)]
pub struct ZoneResult {
    pub update: Option<Update>,
    pub delegated: Vec<DelegatedMatch>
}

pub struct Zone {
    manager: ManagerHandle,   // Handle to manager
    path: Path,               // Path to this Zone
    data: ZoneData,           // 'Atomic' data for this Zone
    listeners: Vec<Listener>, // List of binds
    writes: u64               // Number of writes since last fragment check
    // TODO: size: u64,
    // TODO: prefixes: Option<BTreeMap<String, Node>>
    // TODO: replicas: Vec<Replicas>
}

impl ZoneHandle {
    pub fn dispatch(&self, command: Command, listener: &Sender<String>) -> ZoneResult {
        let (tx, rx) = channel();

        let command = UserCommand { command: command, reply: tx, listener: listener.clone() };

        self.tx.send(ZoneCall::UserCommand(command)).unwrap();
        rx.recv().unwrap()
    }

    pub fn merge(&self, parent_vis: Vis, diff: Node) {
        self.tx.send(ZoneCall::Merge(parent_vis, diff)).unwrap();
    }

    pub fn size(&self) -> usize {
        let (tx, rx) = channel();

        self.tx.send(ZoneCall::Size(tx)).unwrap();
        rx.recv().unwrap()
    }
}

impl Zone {
    pub fn spawn(manager: ManagerHandle, path: &Path) -> ZoneHandle {
        let (tx, rx) = channel();

        let zone = Zone::new(manager, path);

        let name = path.path.join(".");

        mioco::spawn(move|| {
            zone.message_loop(rx);
        });

        ZoneHandle { tx: tx }
    }

    pub fn new(manager: ManagerHandle, path: &Path) -> Zone {
        Zone {
            manager: manager,
            path: path.clone(),
            data: ZoneData {
                node: Node::expand(Value::Null, 0),
                vis: match path.path.len() {
                    0 => Vis::new(1, 0),
                    _ => Default::default()
                }
            },
            listeners: vec![],
            writes: 0
        }
    }

    fn message_loop(mut self, rx: Receiver<ZoneCall>) {
        loop {
            let call = rx.recv().unwrap();

            match call {
                ZoneCall::UserCommand(cmd) => {
                    let result = self.dispatch(cmd.command, &cmd.listener);

                    cmd.reply.send(result).unwrap(); // TODO: don't crash the Zone!
                },
                ZoneCall::Merge(vis, diff) => {
                    self.merge(vis, diff);
                },
                ZoneCall::Size(reply) => {
                    reply.send(self.size()).unwrap();
                }
            }

            self.split_check();
        }
    }

    pub fn dispatch(&mut self, command: Command, tx: &Sender<String>) -> ZoneResult {
        match command.call {
            Call::Bind => {
                let (update, delegated) = self.bind(&command.path, tx);

                ZoneResult { update: update, delegated: delegated }
            },
            Call::Kill => {
                self.kill(&command.path, command.timestamp);

                ZoneResult { ..Default::default() }
            }
            Call::Read => {
                let (update, delegated) = self.read(&command.path);

                ZoneResult { update: update, delegated: delegated }
            },
            Call::Write => {
                self.write(&command.path, command.timestamp, command.params);

                ZoneResult { ..Default::default() }
            }
        }
    }

    /// Bind value(s)
    pub fn bind(&mut self, path: &Path, tx: &Sender<String>) -> (Option<Update>, Vec<DelegatedMatch>) {
        // TODO verify path

        self.sub(path, tx);
        self.read(path)
    }

    /// Kill value(s)
    pub fn kill(&mut self, path: &Path, ts: u64) {
        let node = Node::delete(ts);

        let diff = node.prepend_path(&path.path);

        self.merge(Default::default(), diff);
        // TODO: externals goes to external nodes
        // TODO: diff goes to replicas
    }

    /// Merge value(s). Merge is generic and most operations are defined as a merge.
    pub fn merge(&mut self, mut parent_new_vis: Vis, mut diff: Node) {
        let (update, externals) = {
            let ZoneData { ref mut node, vis } = self.data;

            parent_new_vis.merge(&vis); // 'new' vis cannot contain older data than current vis
            node.merge(&mut diff, vis, parent_new_vis)
        };

        self.data.vis = parent_new_vis;

        // Only notify if there are changes
        if let Some(update) = update {
            self.notify(&update);
            self.writes += 1;
        }

        if externals.len() > 0 {
            self.manager.send_externals(&self.path, externals);
        }

        if ! diff.is_noop() {
            // TODO: diff goes to replicas
        }
    }

    /// Read value(s)
    pub fn read(&self, path: &Path) -> (Option<Update>, Vec<DelegatedMatch>) {
        // TODO verify path

        self.data.node.read(self.data.vis, path)
    }

    /// Get estimated size
    pub fn size(&self) -> usize {
        self.data.node.max_bytes_path().0
    }

    /// Writes value(s) to the node at `path` at time `ts`
    pub fn write(&mut self, path: &Path, ts: u64, value: Value) {
        // TODO verify path
        let diff = Node::expand_from(&path.path[..], value, ts);

        self.merge(Default::default(), diff);
    }

    fn notify(&self, update: &Update) {
        for listener in &self.listeners {
            listener.update(update).unwrap();
            // TODO: don't crash Zone; remove listener from list
            // TODO: if externals change, binds need to be propagated to new Zones
        }
    }

    fn sub(&mut self, path: &Path, tx: &Sender<String>) {
        let listener = Listener::new(path, tx);

        self.listeners.push(listener);
    }

    fn split_check(&mut self) {
        if self.writes >= 10 {
            self.writes = 0;

            if let Some(delegate_node) = delegate(&self.data.node) {
                self.merge(Default::default(), delegate_node);
            }
        }
    }
}
