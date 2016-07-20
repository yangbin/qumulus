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
    Load,
    Merge(Vis, Node),
    Size(Sender<usize>),
    State(Sender<ZoneState>)
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

/// Tracks current state of a Zone
#[derive(Clone, Copy, Debug, Default)]
pub struct ZoneState {
    state: u64 // TODO: use atomics
}

pub struct Zone {
    path: Path,               // Path to this Zone
    data: ZoneData,           // 'Atomic' data for this Zone
    state: ZoneState,         // Current state of Zone
    manager: ManagerHandle,   // Handle to manager
    handle: ZoneHandle,       // Handle to zone
    rx: Receiver<ZoneCall>,   // Zone message inbox
    queued: Vec<ZoneCall>,    // When Zone data is not active, queue up all commands
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

    pub fn load(&self) {
        self.tx.send(ZoneCall::Load).unwrap();
    }

    pub fn merge(&self, parent_vis: Vis, diff: Node) {
        self.tx.send(ZoneCall::Merge(parent_vis, diff)).unwrap();
    }

    pub fn size(&self) -> usize {
        let (tx, rx) = channel();

        self.tx.send(ZoneCall::Size(tx)).unwrap();
        rx.recv().unwrap()
    }

    /// Gets current `ZoneState`
    pub fn state(&self) -> ZoneState {
        let (tx, rx) = channel();

        self.tx.send(ZoneCall::State(tx)).unwrap();
        rx.recv().unwrap()
    }
}

impl ZoneState {
    const INIT: u64    = 0;
    const LOADING: u64 = 1;
    const ACTIVE: u64  = 2;
    const DIRTY: u64   = 3;
    const WRITING: u64 = 4;

    /// Zone is waiting (for enough resources) to load data. User commands are queued.
    pub fn is_init(&self) -> bool { self.state == ZoneState::INIT }

    /// Zone is waiting for data to load. User commands are queued
    pub fn is_loading(&self) -> bool { self.state == ZoneState::LOADING }

    /// Zone is ready to accept user commands: data is loaded and clean
    pub fn is_active(&self) -> bool { self.state == ZoneState::ACTIVE }

    /// Zone has dirty data: write requested or pending
    pub fn is_dirty(&self) -> bool { self.state == ZoneState::DIRTY }

    /// Write pending, data will be clean when done
    pub fn is_writing(&self) -> bool { self.state == ZoneState::WRITING }

    /// Data is ready, allow reads and writes
    pub fn is_ready(&self) -> bool { self.state >= ZoneState::ACTIVE }

    /// Set to specified state
    pub fn set(&mut self, state: u64) {
        assert!(state <= ZoneState::WRITING);
        self.state = state;
    }
}

impl Zone {
    pub fn spawn(manager: ManagerHandle, path: &Path) -> ZoneHandle {
        let zone = Zone::new(manager, path);

        let handle = zone.handle.clone();

        let name = path.path.join(".");

        mioco::spawn(move|| {
            zone.message_loop();
        });

        handle
    }

    pub fn new(manager: ManagerHandle, path: &Path) -> Zone {
        let (tx, rx) = channel();

        Zone {
            path: path.clone(),
            data: ZoneData {
                node: Node::expand(Value::Null, 0),
                vis: match path.path.len() {
                    0 => Vis::new(1, 0),
                    _ => Default::default()
                }
            },
            state: Default::default(),
            manager: manager,
            handle: ZoneHandle { tx: tx },
            rx: rx,
            queued: vec![],
            listeners: vec![],
            writes: 0
        }
    }

    fn message_loop(mut self) {
        loop {
            let call = self.rx.recv().unwrap();

            match call {
                ZoneCall::UserCommand(cmd) => {
                    let result = self.dispatch(cmd.command, &cmd.listener);

                    cmd.reply.send(result).unwrap(); // TODO: don't crash the Zone!
                },
                ZoneCall::Load => {
                    self.load();
                },
                ZoneCall::Merge(vis, diff) => {
                    self.merge(vis, diff);
                },
                ZoneCall::Size(reply) => {
                    reply.send(self.size()).unwrap();
                },
                ZoneCall::State(reply) => {
                    reply.send(self.state()).unwrap();
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

    /// Load data if not already loaded. Usually called by `Manager` when sufficient memory is available
    pub fn load(&mut self) {
        if ! self.state.is_ready() {
            self.manager.store.load(&self.handle, &self.path);
            self.state.set(ZoneState::LOADING);
        }
    }

    /// Get estimated size
    pub fn size(&self) -> usize {
        self.data.node.max_bytes_path().0
    }

    /// Get zone state
    pub fn state(&self) -> ZoneState {
        self.state
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

#[test]
fn test_zone_state() {
    let mut state: ZoneState = Default::default();

    assert!(state.is_init());
    assert!(!state.is_ready());

    state.set(ZoneState::LOADING);
    assert!(state.is_loading());
    assert!(!state.is_ready());

    state.set(ZoneState::ACTIVE);
    assert!(state.is_active());
    assert!(state.is_ready());

    state.set(ZoneState::DIRTY);
    assert!(state.is_dirty());
    assert!(state.is_ready());

    state.set(ZoneState::WRITING);
    assert!(state.is_writing());
    assert!(state.is_ready());
}
