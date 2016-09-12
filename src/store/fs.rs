//! A simple filesystem based zone store. For test use only.

use std;
use std::collections::VecDeque;
use std::error::Error;
use std::fs::{DirBuilder, File};
use std::hash::{Hash, Hasher, SipHasher};
use std::io::ErrorKind;
use std::io::prelude::*;
use std::sync::{Arc, Mutex};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;

use bincode;
use threadpool::ThreadPool;

use super::*;
use path::Path;
use zone::{ZoneData, ZoneHandle};

const NUM_THREADS: usize = 10;

pub struct FS {
    dir: std::path::PathBuf,
    rx: Receiver<StoreCall>,
    tx: Sender<StoreCall>,

    read_pool: ThreadPool,
    write_pool: ThreadPool,

    write_queue: Arc<Mutex<VecDeque<ZoneHandle>>>
}

impl FS {
    /// Start the Store "process".
    pub fn spawn(dir: &str) -> StoreHandle {
        // TODO: take serializer as parameter?

        let store = FS::new(dir);
        let handle = store.handle();

        thread::spawn(move|| {
            store.message_loop();
        });

        handle
    }

    pub fn new(dir: &str) -> FS {
        let dir = std::path::PathBuf::from(dir);

        if ! dir.is_dir() {
            DirBuilder::new().recursive(true).create(&dir).unwrap();
        }

        let (tx, rx) = channel();

        FS {
            dir: dir,
            tx: tx,
            rx: rx,
            read_pool: ThreadPool::new(NUM_THREADS),
            write_pool: ThreadPool::new(NUM_THREADS),
            write_queue: Arc::new(Mutex::new(VecDeque::new()))
        }
    }

    /// Return a handle to Store "process".
    fn handle(&self) -> StoreHandle {
        StoreHandle { tx: self.tx.clone() }
    }

    fn message_loop(self) {
        loop {
            let call = self.rx.recv().unwrap();

            match call {
                StoreCall::Load(zone, path) => self.load(zone, path),
                StoreCall::RequestWrite(zone) => self.request_write(zone),
                StoreCall::Write(zone, path, data) => self.write(zone, path, data)
            }
        }
    }

    /// Loads data for a `Zone` asynchronously, notifying its handle when done.
    pub fn load(&self, zone: ZoneHandle, path: Path) {
        let path = path.clone();
        let mut filepath = self.dir.clone();

        // TODO threadpool
        self.read_pool.execute(move|| {
            debug!("Loading: {:?}", path);

            let filename = zonefilename(&path);

            filepath.push(filename);

            debug!("reading {}", filepath.display());

            match blocking_read(&*filepath) {
                Err(err) => {
                    error!("Error loading {:?} - {}: {}", path, filepath.display(), err.description());
                    error!("{:?}", err);
                    // TODO: set Zone to error state
                    //zone.set_error(err);
                },
                Ok(node) => zone.loaded(node)
            };
        });
    }

    /// Request for notification to write data.
    pub fn request_write(&self, zone: ZoneHandle) {
        if self.write_pool.active_count() >= NUM_THREADS {
            // No write slots available, save for later
            self.write_queue.lock().unwrap().push_back(zone);
        }
        else {
            zone.save();
        }
    }

    /// Write data for a `Zone` asynchronously, notifying its handle when done.
    pub fn write(&self, zone: ZoneHandle, path: Path, data: Vec<u8>) {
        let path = path.clone();
        let mut filepath = self.dir.clone();

        let pending = self.write_queue.clone();

        // TODO threadpool
        self.write_pool.execute(move|| {
            debug!("Writing: {:?}", path);

            let filename = zonefilename(&path);

            filepath.push(filename);

            debug!("writing {}", filepath.display());

            match blocking_write(&*filepath, data) {
                Err(err) => {
                    error!("Error writing {:?} - {}: {}", path, filepath.display(), err.description());
                    error!("{:?}", err);
                    // TODO set Zone to error state
                    //zone.set_error(err);
                },
                Ok(_) => zone.saved()
            };

            let mut pending = pending.lock().unwrap();

            // "Wake" any zones waiting to write
            if let Some(zone) = pending.pop_front() {
                zone.save();
            }
        });
    }
}

fn blocking_read(filepath: &std::path::Path) -> Result<ZoneData, StoreError> {
    debug!("blocking_read: {:?}", filepath);

    let mut file = match File::open(filepath) {
        Err(err) => {
            match err.kind() {
                ErrorKind::NotFound => {
                    return Ok(Default::default())
                },
                _ => {
                    error!("  Error loading {}: {}", filepath.display(), err.description());
                    error!("    {:?}", err);
                }
            }

            error!("IO error: {}", err.description());

            return Err(StoreError::ReadError(Box::new(err)));
        },
        Ok(file) => file,
    };

    let mut buffer = Vec::new();

    // read the whole file
    if let Err(err) = file.read_to_end(&mut buffer) {
        return Err(StoreError::ReadError(Box::new(err)));
    }

    match bincode::serde::deserialize(&buffer) {
        Err(err) => {
            error!("err {}:", err.description());
            Err(StoreError::ReadError(Box::new(err)))
        },
        Ok(data) => Ok(data)
    }
}

fn blocking_write(filepath: &std::path::Path, serialized: Vec<u8>) -> Result<(), StoreError> {
    debug!("blocking_write: {:?}", filepath);

    let tmp_path = filepath.with_extension("tmp");

    let mut file = match File::create(&tmp_path) {
        Err(err) => {
            error!("  Error loading {}: {}", filepath.display(), err.description());
            error!("    {:?}", err);

            error!("IO error: {}", err.description());

            return Err(StoreError::ReadError(Box::new(err)));
        },
        Ok(file) => file,
    };

    if let Err(err) = file.write_all(&serialized) {
        return Err(StoreError::WriteError(Box::new(err)));
    }

    if let Err(err) = std::fs::rename(&tmp_path, &filepath) {
        return Err(StoreError::WriteError(Box::new(err)));
    }

    Ok(())
}

fn zonefilename(path: &Path) -> String {
    let zonename = path.path.join(".");
    let mut filename = String::from("r");

    if path.len() > 0 { 
        filename.push_str(&zonename);
    }

    // Truncate and remove unsafe characters
    filename.truncate(80);

    let mut filename: String = filename.chars().map(|x| match x { 
        '#' | '0'...'9' | 'A'...'Z' | 'a'...'z' => x,
        _  => '_'
    }).collect();

    filename.push_str("_");

    // Add a unique hash
    let mut hasher = SipHasher::new();
    
    zonename.hash(&mut hasher);
    filename.push_str(&format!("{:X}", hasher.finish()));

    filename
}

#[test]
fn test_read_write() {
    let dir = std::path::PathBuf::from("test_data");

    if ! dir.is_dir() {
        DirBuilder::new().recursive(true).create(&dir).unwrap();
    }

    let mut file = dir.clone();

    file.push("test_read_write");

    std::fs::remove_file(&file).ok();

    let data = blocking_read(&file).unwrap();

    assert_eq!(data, Default::default());

    let limit = bincode::SizeLimit::Infinite;
    let serialized = bincode::serde::serialize(&data, limit).unwrap();

    blocking_write(&file, serialized).unwrap();

    assert_eq!(blocking_read(&file).unwrap(), data);

    use node::{Node, Vis};
    use serde_json::Value as JSON;

    let expected = ZoneData::new(
        Vis::update(1000),
        Node::expand(JSON::String(String::from("moo")), 1000)
    );

    let limit = bincode::SizeLimit::Infinite;
    let serialized = bincode::serde::serialize(&expected, limit).unwrap();

    blocking_write(&file, serialized).unwrap();

    let verify = blocking_read(&file).unwrap();

    assert_eq!(verify, expected);
}
