//! Owns a subtree of entire tree, also unit of concurrency
//!
//! The `Zone` structure represents the subtree and is owned by a single thread.
//!
//! `ZoneHandle` is the shareable / clonable public interface to a `Zone`.

use std::hash::{Hash, Hasher};
use std::sync::Arc;

use mioco;
use mioco::sync::mpsc::{channel, Receiver, Sender};
use serde_json::Value;

use command::Command;
use command::Call;
use delegate::delegate;
use listener::{Listener, RListener};
use manager::ManagerHandle;
use node::{DelegatedMatch, Node, Update, Vis, NodeTree};
use path::Path;

/// Persistent Zone data
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct ZoneData {
    pub path: Path,    // TODO: repeated data needed for persistence
    pub tree: NodeTree // Mergeable data for this Zone
}

/// Public shareable handle to a `Zone`
#[derive(Clone)]
pub struct ZoneHandle {
    path: Arc<Path>,
    tx: Sender<ZoneCall>
}

/// Zones communicate via message passing. This enum is a list of valid calls.
enum ZoneCall {
    UserCommand(UserCommand),
    Hibernate,
    Load,
    Loaded(ZoneData),
    Merge(NodeTree),
    MergeWithListeners(NodeTree, Vec<RListener>),
    Save,
    Saved,
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
    path: Arc<Path>,          // Path to this Zone
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

    /// Signal `Zone` to load data. Usually called by `Manager`.
    pub fn load(&self) {
        self.tx.send(ZoneCall::Load).unwrap();
    }

    /// Signal `Zone` with loaded data. Usually called by `Store` with loaded data.
    pub fn loaded(&self, data: ZoneData) {
        self.tx.send(ZoneCall::Loaded(data)).unwrap();
    }

    /// Merge data into this `Zone`. The effective parent visibility (through all ancestors) must
    /// be provided.
    pub fn merge(&self, diff: NodeTree) {
        self.tx.send(ZoneCall::Merge(diff)).unwrap();
    }

    /// Same as `merge` except a list of listeners is also provided. The listeners expect to see
    /// changes that would bring them up to date with data in this `Zone`
    pub fn merge_with_listeners(&self, diff: NodeTree, listeners: Vec<RListener>) {
        self.tx.send(ZoneCall::MergeWithListeners(diff, listeners)).unwrap();
    }

    pub fn path(&self) -> Path {
        (*self.path).clone()
    }

    /// Signal `Zone` to hibernate. Usually called by `EvictionManager`.
    pub fn hibernate(&self) {
        self.tx.send(ZoneCall::Hibernate).unwrap();
    }

    /// Signal `Zone` to the zone to save data. Usually called by `Store` to indicate write-readiness.
    pub fn save(&self) {
        self.tx.send(ZoneCall::Save).unwrap();
    }

    /// Signal `Zone` that save has completed. Usually called by `Store` after write completes.
    pub fn saved(&self) {
        self.tx.send(ZoneCall::Saved).unwrap();
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

impl PartialEq for ZoneHandle {
    fn eq(&self, other: &ZoneHandle) -> bool {
        self.path == other.path
    }
}

impl Eq for ZoneHandle {}

impl Hash for ZoneHandle {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.path.hash(state);
    }
}

impl ZoneState {
    const IDLE: u64    = 0;
    const INIT: u64    = 1;
    const LOADING: u64 = 2;
    const ACTIVE: u64  = 3;
    const DIRTY: u64   = 4;
    const WRITING: u64 = 5;

    /// Zone is idle / hibernating
    pub fn is_idle(&self) -> bool { self.state == ZoneState::IDLE }

    /// Zone is requesting and is waiting for available resources to load data.
    /// User commands are queued.
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

        mioco::spawn(move|| {
            zone.message_loop();
        });

        handle
    }

    pub fn new(manager: ManagerHandle, path: &Path) -> Zone {
        let (tx, rx) = channel();

        let arc_path = Arc::new(path.clone());

        Zone {
            path: arc_path.clone(),
            data: ZoneData {
                path: path.clone(),
                tree: NodeTree {
                    node: Default::default(),
                    vis: match path.len() {
                        0 => Vis::permanent(),
                        _ => Default::default()
                    }
                }
            },
            state: Default::default(),
            manager: manager,
            handle: ZoneHandle { path: arc_path, tx: tx },
            rx: rx,
            queued: vec![],
            listeners: vec![],
            writes: 0
        }
    }

    fn message_loop(mut self) {
        loop {
            let call = self.rx.recv().unwrap();

            if self.state.is_ready() {
                self.handle_call(call);
            }
            else {
                match call {
                    ZoneCall::Load |
                    ZoneCall::Loaded(_) |
                    ZoneCall::Hibernate |
                    ZoneCall::Size(_) |
                    ZoneCall::State(_) => {
                        self.handle_call(call);
                    },
                    _ => {
                        self.queued.push(call);

                        if self.state.is_idle() {
                            self.manager.zone_request_load(self.handle.clone());
                            self.state.set(ZoneState::INIT);
                        }
                    }
                }
            }
        }
    }

    fn handle_call(&mut self, call: ZoneCall) {
        match call {
            ZoneCall::UserCommand(cmd) => {
                let result = self.dispatch(cmd.command, &cmd.listener);

                cmd.reply.send(result).unwrap(); // TODO: don't crash the Zone!
            },
            ZoneCall::Load => {
                self.load();
            },
            ZoneCall::Loaded(data) => {
                self.loaded(data);
            },
            ZoneCall::Merge(diff) => {
                self.merge(diff);
                self.split_check();
            },
            ZoneCall::MergeWithListeners(diff, listeners) => {
                self.merge_with_listeners(diff, listeners);
                self.split_check();
            },
            ZoneCall::Hibernate => {
                self.hibernate();
            },
            ZoneCall::Save => {
                self.save();
            },
            ZoneCall::Saved => {
                self.saved();
            },
            ZoneCall::Size(reply) => {
                reply.send(self.size()).unwrap();
            },
            ZoneCall::State(reply) => {
                reply.send(self.state()).unwrap();
            }
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
                self.split_check();

                ZoneResult { ..Default::default() }
            }
        }
    }

    /// Bind value(s)
    pub fn bind(&mut self, path: &Path, tx: &Sender<String>) -> (Option<Update>, Vec<DelegatedMatch>) {
        // TODO verify path
        // TODO don't sub if path has been delegated completely

        self.sub(path, tx);
        self.read(path)
    }

    /// Kill value(s)
    pub fn kill(&mut self, path: &Path, ts: u64) {
        let node = Node::delete(ts);

        let diff = node.prepend_path(&path.path);

        self.merge(diff.noop_vis());
        // TODO: externals goes to external nodes
        // TODO: diff goes to replicas
    }

    /// Merge value(s). Merge is generic and most operations are defined as a merge.
    pub fn merge(&mut self, mut diff: NodeTree) {
        let (update, externals) = self.data.tree.merge(&mut diff);

        // Only notify if there are changes
        if let Some(update) = update {
            self.notify(&update);
            self.writes += 1;
            self.dirty();
        }

        if externals.len() > 0 {
            for external in externals {
                // Check if Node is in a transition to being delegated.
                // Existing listeners are either:
                //   - unaffected by delegation
                //   - delegated and no longer in scope for this Zone
                //   - delegated but still in scope
                if external.initial {
                    let mut x_listeners = vec![];

                    self.listeners.retain(|l| {
                        let (retain, x_listener) = l.delegate(&external.path);

                        if let Some(x_listener) = x_listener {
                            x_listeners.push(x_listener);
                        }

                        retain
                    });

                    self.manager.send_external_with_listeners(&self.path, external, x_listeners);
                }
                else {
                    // Data meant for delegated node
                    self.manager.send_external(&self.path, external);
                }
            }
        }

        if ! diff.node.is_noop() {
            // TODO: diff goes to replicas
        }
    }

    /// Same as Merge except a list of listeners is provided, which expects
    /// updates that would bring those listeners up to date.
    ///
    /// TODO: This might be better implemented as a merge_bind operation (where
    /// bind takes in a "cached values" parameter), which will solve the
    /// recursive delegation problem.
    pub fn merge_with_listeners(&mut self, diff: NodeTree, listeners: Vec<RListener>) {
        // First, bring listeners up to date
        let (update, externals) = {
            // TODO: workaround merge mutating receiver and argument
            let mut tree_clone = self.data.tree.clone();
            let mut diff_clone = diff.clone();

            // merge "the other way" to get the reverse updates
            diff_clone.merge(&mut tree_clone)
        };

        if externals.len() > 0 {
            // TODO: Zone has delegations so listeners need to be propagated to
            // those external Zones as well.
            println!("Warning: recursive delegation not handled");
        }

        // Convert all RListeners to Listeners
        let mut listeners: Vec<_> = listeners
            .into_iter()
            .map(|l| l.to_absolute(self.path.clone()))
            .collect();

        // Only notify if there are backports
        if let Some(update) = update {
            listeners.retain(|listener| {
                listener.update(&update).is_ok()
            });
        }

        // Merge data delegated from parent
        self.merge(diff);

        // Add delegated listeners to `Zone`
        self.listeners.append(&mut listeners);
    }

    /// Read value(s)
    pub fn read(&self, path: &Path) -> (Option<Update>, Vec<DelegatedMatch>) {
        // TODO verify path

        self.data.tree.read(path)
    }

    /// Load data if not already loaded. Usually called by `Manager` when sufficient memory is available.
    pub fn load(&mut self) {
        if self.state.is_init() {
            self.manager.store.load(&self.handle, &self.path);
            self.state.set(ZoneState::LOADING);
        }
        else {
            panic!("Zone is in the wrong state to load data: {:?}", self.path())
        }
    }

    /// Callback for stores to send loaded data to `Zone`. Usually called by a `Store` process.
    pub fn loaded(&mut self, mut data: ZoneData) {
        if self.state.is_loading() {
            if self.path.len() == 0 {
                data.tree.vis = Vis::permanent();
            }

            self.data.tree = data.tree;
            self.state.set(ZoneState::ACTIVE);

            let queued = self.queued.split_off(0);

            for call in queued {
                // TODO: This re-orders requests. Find a better way. Required
                //       for now as some state handling is not re-entrant
                self.handle.tx.send(call).unwrap();
            }
        }
        else {
            unimplemented!()
        }
    }

    /// Callback to notify Zone to hibernate.
    pub fn hibernate(&mut self) {
        if self.state.is_active() {
            self.state.set(ZoneState::IDLE);
            self.data.tree = Default::default();
            self.manager.zone_hibernated(self.handle.clone());
        }
        else {
            self.manager.zone_defer_hibernation(self.handle.clone());
        }
    }

    /// Callback to notify Zone of available resources to persist dirty data.
    pub fn save(&mut self) {
        if self.state.is_dirty() {
            self.manager.store.write(&self.handle, &self.path, &self.data);
            self.state.set(ZoneState::WRITING);
        }
        else {
            println!("Spurious save callback in {:?}", &self.path);
        }
    }

    /// Callback to notify Zone that data was persisted.
    pub fn saved(&mut self) {
        if self.state.is_writing() {
            self.state.set(ZoneState::ACTIVE);
        }
        else if self.state.is_dirty() {
            // Zone dirtied itself during a write
            self.manager.store.request_write(&self.handle);
        }
        else {
            unimplemented!();
        }
    }

    /// Get zone path.
    pub fn path(&self) -> Path {
        (*self.path).clone()
    }

    /// Get estimated size.
    pub fn size(&self) -> usize {
        // TODO: size does not handle cloaked data properly
        self.data.tree.node.total_byte_size()
    }

    /// Get zone state.
    pub fn state(&self) -> ZoneState {
        self.state
    }

    /// Writes value(s) to the node at `path` at time `ts`
    pub fn write(&mut self, path: &Path, ts: u64, value: Value) {
        // TODO verify path
        let diff = Node::expand_from(&path.path[..], value, ts);

        self.merge(diff.noop_vis());
    }

    fn dirty(&mut self) {
        if self.state.is_dirty() {
            return; // already dirty
        }

        if self.state.is_active() {
            self.manager.store.request_write(&self.handle);
            self.state.set(ZoneState::DIRTY);

            return;
        }

        if self.state.is_writing() {
            self.state.set(ZoneState::DIRTY);

            return;
        }

        unimplemented!();
    }

    /// Notifies listeners
    fn notify(&mut self, update: &Update) {
        self.listeners.retain(|listener| {
            listener.update(update).is_ok()
        });
    }

    fn sub(&mut self, path: &Path, tx: &Sender<String>) {
        let listener = Listener::new(self.path.clone(), Arc::new(path.clone()), tx);

        self.listeners.push(listener);
    }

    fn split_check(&mut self) {
        if self.writes >= 10 {
            self.writes = 0;

            if let Some(delegate_node) = delegate(&self.data.tree.node) {
                self.merge(delegate_node.noop_vis());
            }
        }
    }
}

impl ZoneData {
    pub fn new(path: Path, tree: NodeTree) -> ZoneData {
        ZoneData {
            path: path,
            tree: tree
        }
    }
}

#[test]
fn test_zone_state() {
    let mut state: ZoneState = Default::default();

    assert!(state.is_idle());
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
