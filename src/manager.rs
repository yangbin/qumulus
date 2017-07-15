//! Zone registry, dispatches commands and spawns Zones

use std::any::Any;
use std::collections::{BTreeMap, HashSet, VecDeque};
use std::thread;

use mioco;
use mioco::sync::mpsc::{channel, Receiver, Sender};
use rand;

use app::{App, AppHandle};
use listener::RListener;
use node::External;
use path::Path;
use zone::{Zone, ZoneHandle};

const MAX_LOADED_SOFT: usize = 600;
const MAX_LOADED_HARD: usize = 800;

/// A handle to the Manager process. This is the shareable public interface.
#[derive(Clone)]
pub struct ManagerHandle {
    tx: Sender<(Option<Sender<Box<Any + Send>>>, ManagerCall)>
}

/// Channel (both ends) to talk to Manager, `rx` needed to spawn Manager.
pub struct ManagerChannel {
    rx: Receiver<(Option<Sender<Box<Any + Send>>>, ManagerCall)>,
    tx: Sender<(Option<Sender<Box<Any + Send>>>, ManagerCall)>
}

pub enum ManagerCall {
    FindNearest(Path),
    Find(Path),
    List,
    Load(Path),
    ZoneLoaded(Path),

    // Called by Zones
    SignalDeferHibernation(ZoneHandle),
    SignalHibernated(ZoneHandle),
    SignalRequestLoad(ZoneHandle),
}

pub struct Manager {
    app: AppHandle,
    eviction: EvictionHandle,
    active: BTreeMap<Path, ZoneHandle>,
    loaded: usize,
    requesting_load: VecDeque<ZoneHandle>,
    rx: Receiver<(Option<Sender<Box<Any + Send>>>, ManagerCall)>
}

impl ManagerHandle {
    pub fn find(&self, path: &Path) -> Option<ZoneHandle> {
        self.call(ManagerCall::Find(path.clone()))
    }

    pub fn find_nearest(&self, path: &Path) -> (Path, ZoneHandle) {
        self.call(ManagerCall::FindNearest(path.clone()))
    }

    pub fn load(&self, path: &Path) -> ZoneHandle {
        self.call(ManagerCall::Load(path.clone()))
    }

    /// Routes delegated data to the correct `Zone`
    pub fn send_external(&self, prefix: &Path, external: External, replicate: bool) {
        let mut path = prefix.clone();

        // TODO: zone may be remote

        // Borrow checker doesn't like:
        //   path.append(&mut external.path);
        let mut p = external.path;
        path.append(&mut p);

        let zone = self.load(&path);

        zone.merge(external.tree, replicate); // TODO flow control
    }

    /// Routes delegated data to the correct `Zone` with a list of listeners.
    pub fn send_external_with_listeners(&self, prefix: &Path, external: External, listeners: Vec<RListener>) {
        let mut path = prefix.clone();

        // TODO: zone may be remote

        // Borrow checker doesn't like:
        //   path.append(&mut external.path);
        let mut p = external.path;
        path.append(&mut p);

        let zone = self.load(&path);

        zone.merge_with_listeners(external.tree, listeners); // TODO flow control
    }

    pub fn send_externals(&self, prefix: &Path, externals: Vec<External>) {
        // TODO: concurrency here is bad
        let mut path = prefix.clone();
        let len = path.len();

        for mut external in externals {
            // TODO: zone may be remote

            path.append(&mut external.path);

            let zone = self.load(&path);

            zone.merge(external.tree, true); // TODO flow control
            path.truncate(len);
        }
    }

    pub fn zone_loaded(&self, path: &Path) -> bool {
        self.call(ManagerCall::ZoneLoaded(path.clone()))
    }

    pub fn list(&self) -> Vec<ZoneHandle> {
        self.call(ManagerCall::List)
    }

    /// Called by Zone to defer hibernation.
    pub fn zone_defer_hibernation(&self, zone: ZoneHandle) {
        self.cast(ManagerCall::SignalDeferHibernation(zone));
    }

    /// Called by Zone to notify of hibernation.
    pub fn zone_hibernated(&self, zone: ZoneHandle) {
        self.cast(ManagerCall::SignalHibernated(zone));
    }

    /// Called by Zone to request to load data.
    pub fn zone_request_load(&self, zone: ZoneHandle) {
        self.cast(ManagerCall::SignalRequestLoad(zone));
    }

    /// Generic function to call a function on the underlying Manager through message passing.
    fn call<T: Any>(&self, call: ManagerCall) -> T {
        let (tx, rx) = channel();

        self.tx.send((Some(tx), call)).unwrap();

        let result = rx.recv().unwrap();

        *result.downcast::<T>().unwrap()
    }

    /// Generic function to send a message to underlying Manager.
    fn cast(&self, msg: ManagerCall) {
        self.tx.send((None, msg)).unwrap();
    }
}

/// Represents both ends of a channel needed to talk to Manager.
impl ManagerChannel {
    pub fn new() -> ManagerChannel {
        let (tx, rx) = channel();

        ManagerChannel { rx: rx, tx: tx }
    }

    pub fn handle(&self) -> ManagerHandle {
        ManagerHandle { tx: self.tx.clone() }
    }
}

impl Manager {
    pub fn spawn(app: &mut App) {
        let manager = Manager::new(app);

        thread::spawn(move|| {
            mioco::start_threads(10, move|| {
                manager.message_loop();
            }).unwrap();
        });
    }

    pub fn new(app: &mut App) -> Manager {
        let eviction = EvictionManager::spawn();
        let channel = app.channels.manager.take().expect("Receiver already taken");

        let manager = Manager {
            app: app.handle(),
            eviction: eviction,
            active: BTreeMap::new(),
            loaded: 0,
            requesting_load: VecDeque::new(),
            rx: channel.rx
        };

        manager
    }

    fn message_loop(mut self) {
        loop {
            let (reply, call) = self.rx.recv().unwrap();

            let result: Box<Any + Send> = match call {
                ManagerCall::Find(path) => Box::new(self.find(&path)),
                ManagerCall::FindNearest(path) => Box::new(self.find_nearest(&path)),
                ManagerCall::List => Box::new(self.list()),
                ManagerCall::Load(path) => Box::new(self.load(&path)),
                ManagerCall::ZoneLoaded(path) => Box::new(self.zone_loaded(&path)),
                ManagerCall::SignalDeferHibernation(zone) => Box::new(self.zone_defer_hibernation(zone)),
                ManagerCall::SignalHibernated(zone) => Box::new(self.zone_hibernated(zone)),
                ManagerCall::SignalRequestLoad(zone) => Box::new(self.zone_request_load(zone)),
            };

            if let Some(reply) = reply {
                reply.send(result).unwrap();
            }
        }
    }

    /// Gets a handle to a Zone. This function does not block for the Zone to actually load.
    pub fn load(&mut self, path: &Path) -> ZoneHandle {
        if let Some(zone) = self.active.get(path) {
            return zone.clone();
        }

        let zone = Zone::spawn(self.app.clone(), path);

        self.active.insert(path.clone(), zone.clone());

        zone
    }

    pub fn zone_loaded(&self, path: &Path) -> bool {
        self.active.contains_key(path)
    }

    /// Find the exact `Zone` specified by `path`
    pub fn find(&self, path: &Path) -> Option<ZoneHandle> {
        self.active.get(path).cloned()
    }

    /// Find the 'closest' `Zone` that would be able to satisfy a call to `path`
    pub fn find_nearest(&self, path: &Path) -> (Path, ZoneHandle) {
        // TODO: probably could be more efficient
        // TODO: use a bloom filter?
        let mut probe = path.clone();

        loop {
            if let Some(found) = self.active.get(&probe) {
                return (probe, found.clone())
            }

            probe.pop(); // crash if no root node
        }
    }

    /// List all active zones
    pub fn list(&self) -> Vec<ZoneHandle> {
        self.active.values().cloned().collect()
    }

    /// Called by Zone as a deferment response to hibernation signal.
    pub fn zone_defer_hibernation(&self, zone: ZoneHandle) {
        self.eviction.tx.send(EvictionCall::Deferred(zone)).unwrap();
    }

    /// Called by Zone to notify of hibernation.
    pub fn zone_hibernated(&mut self, zone: ZoneHandle) {
        self.eviction.tx.send(EvictionCall::Unloaded(zone)).unwrap();
        self.loaded -= 1;

        if let Some(zone) = self.requesting_load.pop_front() {
            zone.load();
            self.eviction.tx.send(EvictionCall::Loaded(zone)).unwrap();
            self.loaded += 1;

            if self.requesting_load.len() == 0 {
                info!("Dropped below MAX_LOADED_HARD zones");
            }
        }
    }

    /// Called by Zone to request to load data.
    pub fn zone_request_load(&mut self, zone: ZoneHandle) {
        self.load_zone(zone);
    }

    fn load_zone(&mut self, zone: ZoneHandle) {
        if self.loaded > MAX_LOADED_HARD {
            if self.requesting_load.len() == 0 {
                info!("Exceeded MAX_LOADED_HARD zones");
            }

            self.requesting_load.push_back(zone);

        }
        else {
            zone.load();
            self.eviction.tx.send(EvictionCall::Loaded(zone)).unwrap();
            self.loaded += 1;
        }
    }
}

#[derive(Clone)]
struct EvictionHandle {
    tx: Sender<EvictionCall>
}

struct EvictionManager {
    loaded: HashSet<ZoneHandle>,
    pending: HashSet<ZoneHandle>,
    rx: Receiver<EvictionCall>,
    tx: Sender<EvictionCall>
}

enum EvictionCall {
    Loaded(ZoneHandle),
    Unloaded(ZoneHandle),
    Deferred(ZoneHandle)
}

impl EvictionManager {
    pub fn spawn() -> EvictionHandle {
        let manager = EvictionManager::new();
        let handle = manager.handle();

        thread::spawn(move|| {
            manager.message_loop();
        });

        handle
    }

    pub fn new() -> EvictionManager {
        let (tx, rx) = channel();

        EvictionManager {
            loaded: HashSet::new(),
            pending: HashSet::new(),
            rx: rx,
            tx: tx
        }
    }

    /// Return a handle to EvictionManager "process".
    fn handle(&self) -> EvictionHandle {
        EvictionHandle { tx: self.tx.clone() }
    }

    fn message_loop(mut self) {
        loop {
            let call = self.rx.recv().unwrap();

            match call {
                EvictionCall::Loaded(zone) => {
                    if zone.path().len() != 0 { // root node is exempted
                        self.loaded.insert(zone);
                    }
                },
                EvictionCall::Unloaded(zone) => {
                    self.loaded.remove(&zone);
                    self.pending.remove(&zone);
                },
                EvictionCall::Deferred(zone) => {
                    self.pending.remove(&zone);
                    self.loaded.insert(zone);
                }
            }

            // make a single pass
            self.evict();
        }
    }

    fn evict(&mut self) {
        let loaded = self.loaded.len();
        let pending = self.pending.len();
        let total = loaded + pending;

        if total <= MAX_LOADED_SOFT {
            return;
        }

        if loaded == 0 {
            println!("Nothing to evict");
            return;
        }

        let overflow = total - MAX_LOADED_SOFT;
        let r = rand::random::<usize>() % (MAX_LOADED_HARD - MAX_LOADED_SOFT) / 2;

        if r > overflow {
            return;
        }

        let i = rand::random::<u64>() % loaded as u64;
        let zone = self.loaded.iter().nth(i as usize).unwrap().clone();

        zone.hibernate();
        self.loaded.remove(&zone);
        self.pending.insert(zone);

        // TODO improve this cache eviction algorithm
    }
}

#[test]
fn test_find_nearest() {
    use app;

    let id = "127.0.0.1:1000".parse().unwrap();
    let mut app = app::App::new(id);

    Manager::spawn(&mut app);

    let manager = app.manager;

    let root        = Path::new(vec![]);
    let moo         = Path::new(vec!["moo".into()]);
    let moo_cow     = Path::new(vec!["moo".into(), "cow".into()]);
    let moo_cow_cow = Path::new(vec!["moo".into(), "cow".into(), "cow".into()]);

    manager.load(&root);
    assert_eq!(manager.find_nearest(&moo).0, root);
    assert_eq!(manager.find_nearest(&moo_cow).0, root);
    assert_eq!(manager.find_nearest(&moo_cow_cow).0, root);

    manager.load(&moo_cow);
    assert_eq!(manager.find_nearest(&moo).0, root);
    assert_eq!(manager.find_nearest(&moo_cow).0, moo_cow);
    assert_eq!(manager.find_nearest(&moo_cow_cow).0, moo_cow);

    manager.load(&moo);
    assert_eq!(manager.find_nearest(&moo).0, moo);
    assert_eq!(manager.find_nearest(&moo_cow).0, moo_cow);
    assert_eq!(manager.find_nearest(&moo_cow_cow).0, moo_cow);
}

#[test]
fn test_load() {
    use app;

    let id = "127.0.0.1:1000".parse().unwrap();
    let mut app = app::App::new(id);

    Manager::spawn(&mut app);

    let root = Path::new(vec![]);
    let zone = app.manager.load(&root);

    assert!(zone.state().is_idle());
}
