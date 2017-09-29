//! A simple filesystem based zone store. For test use only.

use std;
use std::collections::VecDeque;
use std::collections::hash_map::DefaultHasher;
use std::error::Error;
use std::fs::{DirBuilder, File};
use std::hash::{Hash, Hasher};
use std::io::ErrorKind;
use std::io::prelude::*;
use std::sync::{Arc, Mutex};
use std::sync::mpsc::{Receiver, Sender};
use std::thread;

use bincode;
use threadpool::ThreadPool;

use super::*;
use app::{App, AppHandle};
use path::Path;
use zone::{ZoneData, ZoneHandle};

const NUM_THREADS: usize = 50;

pub struct FS {
    app: AppHandle,

    dir: std::path::PathBuf,
    rx: Receiver<StoreCall>,

    read_pool: ThreadPool,
    write_pool: ThreadPool,

    write_queue: Arc<Mutex<VecDeque<ZoneHandle>>>
}

impl FS {
    /// Start the Store "process".
    pub fn spawn(app: &mut App) {
        // TODO: take serializer as parameter?
        let dir = format!("data_{}", app.id);
        let channel = app.channels.store.take().expect("Receiver already taken");
        let store = FS::new(app.handle(), &dir, channel);

        thread::spawn(move|| {
            store.message_loop();
        });
    }

    pub fn new(app: AppHandle, dir: &str, channel: StoreChannel) -> FS {
        let dir = std::path::PathBuf::from(dir);

        if ! dir.is_dir() {
            DirBuilder::new().recursive(true).create(&dir).unwrap();
        }

        FS {
            app: app,
            dir: dir,
            rx: channel.rx,
            read_pool: ThreadPool::new(NUM_THREADS),
            write_pool: ThreadPool::new(NUM_THREADS),
            write_queue: Arc::new(Mutex::new(VecDeque::new()))
        }
    }

    fn message_loop(self) {
        loop {
            let call = self.rx.recv().unwrap();

            match call {
                StoreCall::List(reply) => self.list(reply),
                StoreCall::Load(zone, path) => self.load(zone, path),
                StoreCall::LoadData(path, tx) => self.load_data(path, tx),
                StoreCall::RequestWrite(zone) => self.request_write(zone),
                StoreCall::Write(zone, path, data) => self.write(zone, path, data)
            }
        }
    }

    /// Lists all Zone Paths stored locally
    pub fn list(&self, tx: Sender<Path>) {
        let entries = match std::fs::read_dir(&self.dir) {
            Err(err) => {
                error!("Error listing directory.");
                error!("  {:?}", err);
                return;
            },
            Ok(entries) => entries
        };

        for entry in entries {
            let entry = match entry {
                Err(err) => {
                    error!("Error reading entry.");
                    error!("  {:?}", err);
                    return;
                },
                Ok(entry) => entry
            };


            match blocking_read(&entry.path()) {
                Err(err) => {
                    error!("Error loading {:?}: {}", entry, err.description());
                    error!("  {:?}", err);
                },
                Ok(node) => {
                    tx.send(node.path).unwrap();
                }
            }
        }
    }

    /// Loads data for a `Zone` asynchronously, notifying its handle when done.
    pub fn load(&self, zone: ZoneHandle, path: Path) {
        let mut filepath = self.dir.clone();

        self.app.stats.store.reads_pending.increment();

        let stats = self.app.stats.clone();

        self.read_pool.execute(move|| {
            debug!("Loading: {:?}", path);

            let filename = zonefilename(&path);

            filepath.push(filename);

            debug!("reading {}", filepath.display());

            match blocking_read(&*filepath) {
                Err(err) => {
                    error!("Error loading {:?} - {}: {}", path, filepath.display(), err.description());
                    error!("{:?}", err);
                    stats.store.reads_errors.increment();
                    // TODO: set Zone to error state
                    //zone.set_error(err);
                },
                Ok(node) => zone.loaded(node)
            };

            stats.store.reads_pending.decrement();
            stats.store.reads.increment();
        });
    }

    /// Asynchronously load and send `ZoneData` for `Path` to channel.
    pub fn load_data(&self, path: Path, tx: Sender<Option<ZoneData>>) {
        let mut filepath = self.dir.clone();

        self.read_pool.execute(move|| {
            debug!("Loading: {:?}", path);

            let filename = zonefilename(&path);

            filepath.push(filename);

            debug!("reading {}", filepath.display());

            tx.send(blocking_read(&*filepath).ok()).is_ok(); // ignore if caller goes away
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

        self.app.stats.store.writes_pending.increment();

        let stats = self.app.stats.clone();

        self.write_pool.execute(move|| {
            debug!("Writing: {:?}", path);

            let filename = zonefilename(&path);

            filepath.push(filename);

            debug!("writing {}", filepath.display());

            match blocking_write(&*filepath, data) {
                Err(err) => {
                    error!("Error writing {:?} - {}: {}", path, filepath.display(), err.description());
                    error!("{:?}", err);
                    stats.store.writes_errors.increment();
                    // TODO set Zone to error state
                    //zone.set_error(err);
                },
                Ok(_) => zone.saved()
            };

            stats.store.writes_pending.decrement();
            stats.store.writes.increment();

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

    match bincode::deserialize(&buffer) {
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
    let mut hasher = DefaultHasher::new();
    
    zonename.hash(&mut hasher);
    filename.push_str(&format!("{:X}", hasher.finish()));

    filename
}

#[test]
fn test_read_write() {
    let dir = std::path::PathBuf::from("test_data/read_write");

    if ! dir.is_dir() {
        DirBuilder::new().recursive(true).create(&dir).unwrap();
    }

    let mut file = dir.clone();

    file.push("test_read_write");

    std::fs::remove_file(&file).ok();

    let data = blocking_read(&file).unwrap();

    assert_eq!(data, Default::default());

    let limit = bincode::Infinite;
    let serialized = bincode::serialize(&data, limit).unwrap();

    blocking_write(&file, serialized).unwrap();

    assert_eq!(blocking_read(&file).unwrap(), data);

    use node::{Node, NodeTree, Vis};
    use serde_json::Value as JSON;

    let expected = ZoneData::new(
        Path::empty(),
        NodeTree {
            vis: Vis::update(1000),
            node: Node::expand(JSON::String(String::from("moo")), 1000)
        }
    );

    let limit = bincode::Infinite;
    let serialized = bincode::serialize(&expected, limit).unwrap();

    blocking_write(&file, serialized).unwrap();

    let verify = blocking_read(&file).unwrap();

    assert_eq!(verify, expected);
}

#[test]
fn test_list() {
    let dir = std::path::PathBuf::from("test_data/list");

    if dir.exists() {
        std::fs::remove_dir_all(dir).unwrap();
    }

    let chan = StoreChannel::new();

    let app = App::new("127.0.0.1:42".parse().unwrap());
    let store = FS::new(app.handle(), "127.0.0.1:42", chan);

    let noop_zone = ZoneHandle::test_handle(Arc::new(path![]));
    let limit = bincode::Infinite;

    for i in 0..3 {
        let path = Path::new(vec![i.to_string()]);
        let zone_data = ZoneData::new(path.clone(), Default::default());

        let serialized = bincode::serialize(&zone_data, limit).unwrap();

        store.write(noop_zone.clone(), path, serialized);
    }

    std::thread::sleep(std::time::Duration::from_millis(200));

    let (tx, rx) = channel();

    store.list(tx);

    let mut paths: Vec<Path> = rx.iter().collect();

    paths.sort();

    assert_eq!(paths, [
        Path::new(vec!["0".into()]),
        Path::new(vec!["1".into()]),
        Path::new(vec!["2".into()]),
    ]);
}
