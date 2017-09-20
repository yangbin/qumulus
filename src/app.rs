//! Represents the entire application.
//!
//! `handle` Contains handles of all processes.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use command::Call;
use cluster::{ClusterHandle, ClusterChannel};
use manager::{ManagerHandle, ManagerChannel};
use replica::Replica;
use store::{StoreHandle, StoreChannel};

pub struct App {
    pub id: Replica,

    pub cluster: ClusterHandle,
    pub manager: ManagerHandle,
    pub store: StoreHandle,

    pub channels: Channels,

    pub stats: Arc<Stats>
}

/// The shareable reference to the App
#[derive(Clone)]
pub struct AppHandle {
    pub cluster: ClusterHandle,
    pub manager: ManagerHandle,
    pub store: StoreHandle,

    pub stats: Arc<Stats>
}

#[derive(Clone)]
pub struct Handles {
    pub cluster: ClusterHandle,
    pub manager: ManagerHandle,
    pub store: StoreHandle
}

pub struct Channels {
    pub cluster: Option<ClusterChannel>,
    pub manager: Option<ManagerChannel>,
    pub store: Option<StoreChannel>
}

#[derive(Default, Serialize)]
pub struct Stats {
    pub clients: ClientStats,
    pub cluster: ClusterStats,
    pub store: StoreStats,
    pub zones: ZoneStats
}

#[derive(Default, Serialize)]
pub struct ClientStats {
    pub connects: Stat,
    pub disconnects: Stat,
    pub commands: CommandStats,
    pub replies: Stat
}

#[derive(Default, Serialize)]
pub struct ClusterStats {
    pub broadcast: Stat,
    pub handle_cluster_message: Stat,
    pub replicas: Stat,
    pub replicate: Stat
}

#[derive(Default, Serialize)]
pub struct StoreStats {
    pub reads: Stat,
    pub reads_pending: Stat,
    pub reads_errors: Stat,
    pub writes: Stat,
    pub writes_pending: Stat,
    pub writes_errors: Stat
}

#[derive(Default, Serialize)]
pub struct ZoneStats {
    pub local_active: Stat,
    pub local_loaded: Stat
}

#[derive(Default, Serialize)]
pub struct CommandStats {
    pub bind: Stat,
    pub kill: Stat,
    pub read: Stat,
    pub write: Stat
}

#[derive(Default)]
pub struct Stat {
    value: AtomicUsize
}

impl App {
    /// Create channels for all processes
    pub fn new(id: Replica) -> App {
        let cluster = ClusterChannel::new();
        let manager = ManagerChannel::new();
        let store = StoreChannel::new();

        App {
            id: id,

            cluster: cluster.handle(),
            manager: manager.handle(),
            store: store.handle(),

            channels: Channels {
                cluster: Some(cluster),
                manager: Some(manager),
                store: Some(store)
            },

            stats: Default::default()
        }
    }

    pub fn handle(&self) -> AppHandle {
        AppHandle {
            cluster: self.cluster.clone(),
            manager: self.manager.clone(),
            store: self.store.clone(),

            stats: self.stats.clone()
        }
    }

    pub fn stats(&self) -> Arc<Stats> {
        self.stats.clone()
    }
}

impl Stats {
    pub fn to_json(&self) -> String {
        use serde_json;

        serde_json::to_string(self).unwrap()
    }
}

impl CommandStats {
    pub fn increment(&self, call: &Call) {
        match call {
            &Call::Bind => self.bind.increment(),
            &Call::Kill => self.kill.increment(),
            &Call::Read => self.read.increment(),
            &Call::Write => self.write.increment()
        };
    }
}

impl Stat {
    pub fn increment(&self) {
        self.value.fetch_add(1, Ordering::Relaxed);
    }

    pub fn decrement(&self) {
        self.value.fetch_sub(1, Ordering::Relaxed);
    }

    pub fn set(&self, value: usize) {
        self.value.store(value, Ordering::Relaxed);
    }

    pub fn value(&self) -> usize {
        self.value.load(Ordering::Relaxed)
    }
}

use serde::{Serialize, Serializer};

impl Serialize for Stat {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where S: Serializer
    {
        serializer.serialize_u64(self.value() as u64)
    }
}
