//! Cluster manager. Handles Cluster and Sharding (TODO)

use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;

use manager::ManagerHandle;
use replica::Replica;

pub struct Cluster {
    manager: ManagerHandle,
    replicas: Vec<Replica>,
    rx: Receiver<ClusterCall>
}

/// A handle to the Cluster process. This is the shareable public interface.
#[derive(Clone)]
pub struct ClusterHandle {
    tx: Sender<ClusterCall>
}

/// A handle to the Cluster process. This is the shareable public interface.
pub struct ClusterPreHandle {
    pub handle: ClusterHandle,
    rx: Receiver<ClusterCall>
}

/// Used for dispatching calls via message passing.
pub enum ClusterCall {
    Add(Replica),
    Sync
}

impl ClusterHandle {
    /// Add a new Replica to cluster
    pub fn add(&self, replica: Replica) {
        self.tx.send(ClusterCall::Add(replica)).unwrap()
    }

    /// Syncs all Zones with lastest Replica list
    pub fn sync(&self) {
        self.tx.send(ClusterCall::Sync).expect("Cluster process not running");
    }

    /// Creates a noop ClusterHandle for testing
    #[cfg(test)]
    pub fn test_handle() -> ClusterHandle {
        ClusterHandle {
            tx: channel().0
        }
    }
}

/// Cluster and Manger need handles to each other.
impl ClusterPreHandle {
    pub fn new() -> ClusterPreHandle {
        let (tx, rx) = channel();

        ClusterPreHandle {
            handle: ClusterHandle {
                tx: tx
            },
            rx: rx
        }
    }

    /// Start the Cluster "process".
    pub fn spawn(self, manager: ManagerHandle) {
        let cluster = Cluster {
            manager: manager,
            replicas: vec![],
            rx: self.rx
        };

        thread::Builder::new().name("Cluster".into()).spawn(move || {
            cluster.message_loop();
        }).expect("Cluster spawn failed");
    }
}

impl Cluster {
    pub fn new(handle: ClusterPreHandle, manager: ManagerHandle) -> Cluster {
        Cluster {
            manager: manager,
            replicas: vec![],
            rx: handle.rx
        }
    }

    fn message_loop(mut self) {
        loop {
            let call = self.rx.recv().unwrap();

            match call {
                ClusterCall::Add(replica) => self.add(replica),
                ClusterCall::Sync => self.sync()
            }
        }
    }

    /// Add a new Replica to Cluster
    pub fn add(&mut self, replica: Replica) {
        if self.replicas.contains(&replica) {
            return;
        }

        self.replicas.push(replica);
        // TODO: sync?
    }

    /// Make sure each Zone has the latest list of Replicas
    pub fn sync(&self) {
        self.manager.store.each_zone(|path| {
            let zone = self.manager.load(&path);

            zone.sync_replicas(self.replicas.clone());
        })
    }
}

#[test]
fn test_cluster() {
    let replicas = vec![
        "127.0.0.1:1000".parse().unwrap(),
        "127.0.0.1:1001".parse().unwrap(),
        "127.0.0.1:1002".parse().unwrap()
    ];

    use manager;

    let manager = manager::ManagerHandle::test_handle();
    let handle = ClusterPreHandle::new();
    let mut cluster = Cluster::new(handle, manager);

    cluster.add("127.0.0.1:1000".parse().unwrap());
    cluster.add("127.0.0.1:1001".parse().unwrap());
    cluster.add("127.0.0.1:1002".parse().unwrap());

    assert_eq!(cluster.replicas, replicas);
}
