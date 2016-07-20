//! Zone persistence
//!
//! The Store "process" handles Zone persistence.
//!
//! Zones can load data or request to save data. When requesting to save data, `Store` will notify
//! the Zone when it is not busy, at which point the Zone can send its latest copy of its data.

use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;

use node::Node;
use path::Path;
use zone::ZoneHandle;

/// A handle to the store process. This is the shareable public interface.
#[derive(Clone)]
pub struct StoreHandle {
    tx: Sender<StoreCall>
}

/// Used for dispatching calls via message passing.
pub enum StoreCall {
    Load(ZoneHandle, Path),
    RequestWrite(ZoneHandle),
    Write(ZoneHandle, Path, Node)
}

/// Struct owned by Store process.
pub struct Store {
    rx: Receiver<StoreCall>,
    tx: Sender<StoreCall>
}

impl StoreHandle {
    /// Reads data for a given zone path and sends data back directly to the `Zone`.
    pub fn load(&self, zone: &ZoneHandle, path: &Path) {
        self.tx.send(StoreCall::Load(zone.clone(), path.clone())).unwrap();
    }

    /// Ask for non-busy write notification.
    pub fn request_write(&self, zone: &ZoneHandle) {
        unimplemented!();
    }

    /// Saves data for a zone and notifies zone directly via its handle.
    pub fn write(&self, zone: &ZoneHandle, path: &Path, node: &Node) {
        // TODO optimize: serialize instead of cloning before sending over channel
        self.tx.send(StoreCall::Write(zone.clone(), path.clone(), node.clone())).unwrap();
    }
}

impl Store {
    /// Start the Store "process".
    pub fn spawn() -> StoreHandle {
        // TODO: take serializer and backend as parameters

        let store = Store::new();
        let handle = store.handle();

        thread::spawn(move|| {
            store.message_loop();
        });

        handle
    }

    pub fn new() -> Store {
        let (tx, rx) = channel();

        Store { tx: tx, rx: rx }
    }

    /// Return a handle to Store "process".
    fn handle(&self) -> StoreHandle {
        StoreHandle { tx: self.tx.clone() }
    }

    fn message_loop(self) {
        loop {
            let call = self.rx.recv().unwrap();

            match call {
                StoreCall::Load(zone, path) => self.load(&zone, &path),
                StoreCall::RequestWrite(zone) => self.request_write(&zone),
                StoreCall::Write(zone, path, data) => self.write(&zone, &path, &data)
            }
        }
    }

    /// Loads data for a `Zone` asynchronously, notifying its handle when done.
    pub fn load(&self, zone: &ZoneHandle, path: &Path) {
        unimplemented!()
    }

    /// Request for notification to write data.
    pub fn request_write(&self, zone: &ZoneHandle) {
        unimplemented!()
    }

    /// Write data for a `Zone` asynchronously, notifying its handle when done.
    pub fn write(&self, zone: &ZoneHandle, path: &Path, data: &Node) {
        unimplemented!()
    }
}
