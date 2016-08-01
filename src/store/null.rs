//! A null store that loads emppty data and ignores writes. For test use only

use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;

use super::*;
use path::Path;
use zone::{ZoneData, ZoneHandle};

pub struct Null {
    rx: Receiver<StoreCall>,
    tx: Sender<StoreCall>,
}

impl Null {
    /// Start the Store "process".
    pub fn spawn() -> StoreHandle {
        let store = Null::new();
        let handle = store.handle();

        thread::spawn(move|| {
            store.message_loop();
        });

        handle
    }

    pub fn new() -> Null {
        let (tx, rx) = channel();

        Null { tx: tx, rx: rx }
    }

    /// Return a handle to Store "process".
    fn handle(&self) -> StoreHandle {
        StoreHandle { tx: self.tx.clone() }
    }

    fn message_loop(self) {
        loop {
            let call = self.rx.recv().unwrap();

            match call {
                StoreCall::Load(zone, path) => self.load(zone, &path),
                StoreCall::RequestWrite(zone) => self.request_write(zone),
                StoreCall::Write(zone, path, data) => self.write(zone, &path, &data)
            }
        }
    }

    /// Loads data for a `Zone` asynchronously, notifying its handle when done. Will always load an
    /// empty data set.
    pub fn load(&self, zone: ZoneHandle, _: &Path) {
        zone.loaded(Default::default());
    }

    /// Request for notification to write data. Never gonna happen.
    pub fn request_write(&self, _: ZoneHandle) {
    }

    /// Write data for a `Zone` asynchronously, notifying its handle when done.
    /// Not happening either.
    pub fn write(&self, _: ZoneHandle, _: &Path, _: &ZoneData) {
    }

}
