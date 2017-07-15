//! Cluster manager. Handles Cluster and Sharding (TODO)

use std::collections::{HashMap};
use std::net::{SocketAddr,TcpListener,TcpStream};
use std::sync::Arc;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread::Builder;

use bincode;

use app::{App, AppHandle};
use node::NodeTree;
use path::Path;
use replica::Replica;

/// A handle to the Cluster process. This is the shareable public interface.
#[derive(Clone)]
pub struct ClusterHandle {
    tx: Sender<ClusterCall>
}

/// Channel (both ends) to talk to Cluster, `rx` needed to spawn Cluster.
pub struct ClusterChannel {
    tx: Sender<ClusterCall>,
    rx: Receiver<ClusterCall>
}

/// The Cluster manager.
pub struct Cluster {
    app: AppHandle,
    handle: ClusterHandle,
    id: Replica,
    peers: HashMap<Replica, Peer>,
    replicas: Vec<Replica>,
    rx: Receiver<ClusterCall>
}

/// Intra-Cluster Messages.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum ClusterMessage {
    /// Data to be merged for Path
    Merge(Path, NodeTree),
    Sync
}

/// Interface to Peer.
#[derive(Clone, Debug)]
pub struct Peer {
    tx: Sender<Arc<ClusterMessage>>
}

/// Peer internal state.
pub struct PeerState {
    addr: SocketAddr,
    pending: Option<Arc<ClusterMessage>>,
    stream: Option<TcpStream>,
    rx: Receiver<Arc<ClusterMessage>>
}

pub struct Server {
}

/// Used for dispatching calls via message passing.
#[derive(Debug)]
pub enum ClusterCall {
    Add(Replica),
    HandleClusterMessage(ClusterMessage),
    Replicate(Path, NodeTree),
    Sync,
    SyncAll,
    SyncZone(Path)
}

impl ClusterHandle {
    /// Add a new Replica to cluster.
    pub fn add(&self, replica: Replica) {
        self.send(ClusterCall::Add(replica));
    }

    /// Syncs all Zones.
    pub fn sync(&self) {
        self.send(ClusterCall::Sync);
    }

    /// Syncs all Zones on all Peers.
    pub fn sync_all(&self) {
        self.send(ClusterCall::SyncAll);
    }

    /// Syncs Zone to all Peers.
    pub fn sync_zone(&self, path: Path) {
        self.send(ClusterCall::SyncZone(path));
    }

    /// Replicate data to all replicas.
    pub fn replicate(&self, path: &Path, data: NodeTree) {
        self.send(ClusterCall::Replicate(path.clone(), data));
    }

    /// Handles a message from the cluster.
    pub fn handle_cluster_message(&self, msg: ClusterMessage) {
        self.send(ClusterCall::HandleClusterMessage(msg));
    }

    fn send(&self, call: ClusterCall) {
        self.tx.send(call).expect("Cluster process not running");
    }
}

/// Represents both ends of a channel needed to talk to Cluster.
impl ClusterChannel {
    pub fn new() -> ClusterChannel {
        let (tx, rx) = channel();

        ClusterChannel { rx: rx, tx: tx }
    }

    pub fn handle(&self) -> ClusterHandle {
        ClusterHandle { tx: self.tx.clone() }
    }
}

impl Cluster {
    pub fn new(app: &mut App) -> Cluster {
        let rx =  app.channels.cluster.take().expect("Receiver already taken");

        Cluster {
            app: app.handle(),
            id: app.id.clone(),
            handle: app.cluster.clone(),
            peers: HashMap::new(),
            replicas: vec![],
            rx: rx.rx
        }
    }

    /// Start the Cluster "process".
    pub fn spawn(app: &mut App) {
        let mut cluster = Cluster::new(app);

        thread("Cluster").spawn(move || {
            cluster.run();
        }).expect("Cluster spawn failed");
    }

    pub fn run(&mut self) {
        Server::spawn(&self.id.peer_addr(), self.handle.clone());
        self.message_loop();
    }

    fn message_loop(&mut self) {
        loop {
            let call = self.rx.recv().expect("Cluster rx broken");

            match call {
                ClusterCall::Add(replica) => self.add(replica),
                ClusterCall::HandleClusterMessage(msg) => self.handle_cluster_message(msg),
                ClusterCall::Replicate(path, data) => self.replicate(path, data),
                ClusterCall::Sync => self.sync(),
                ClusterCall::SyncAll => self.sync_all(),
                ClusterCall::SyncZone(path) => self.sync_zone(path)
            }
        }
    }

    /// Handles a message from the cluster.
    fn handle_cluster_message(&self, msg: ClusterMessage) {
        match msg {
            ClusterMessage::Merge(path, data) => {
                // TODO thread pool
                let zone = self.app.manager.load(&path);

                zone.merge(data, false);
            },
            ClusterMessage::Sync => self.sync()
        }
    }

    /// Add a new Replica to Cluster
    pub fn add(&mut self, replica: Replica) {
        if replica == self.id {
            return;
        }

        if self.replicas.contains(&replica) {
            return;
        }

        self.replicas.push(replica.clone());

        let peer = Peer::spawn(replica.peer_addr());

        self.peers.insert(replica, peer);
        // TODO: sync?
    }

    /// Replicates data to all replicas.
    pub fn replicate(&self, path: Path, data: NodeTree) {
        // TODO: shard
        // for now, replicate to all replicas
        let message = Arc::new(ClusterMessage::Merge(path.clone(), data));

        for (_addr, peer) in &self.peers {
            peer.send(message.clone());
        }
    }

    /// Synchronize each Zone to all Peers.
    pub fn sync(&self) {
        self.app.store.each_zone(|path| {
            match self.app.store.load_data(path.clone()) {
                None => println!("Could not sync {:?}", path),
                Some(data) => self.replicate(path, data.tree)
            }
        })
    }

    /// Request all peers to synchronize local data.
    pub fn sync_all(&self) {
        self.broadcast(ClusterMessage::Sync);
        self.sync();
    }

    /// Synchronize Zone to all Peers.
    pub fn sync_zone(&self, path: Path) {
        // TODO: does not check for non-existent Zones
        match self.app.store.load_data(path.clone()) {
            None => println!("Could not sync {:?}", path),
            Some(data) => self.replicate(path, data.tree)
        }
    }

    fn broadcast(&self, message: ClusterMessage) {
        let message = Arc::new(message);

        for (_addr, peer) in &self.peers {
            peer.send(message.clone());
        }
    }
}

/// Represents the remote Peer for us to replicate to. Data to be merged into the local Peer is
/// handled by Server
impl Peer {
    /// Start a new Peer "process".
    pub fn spawn(addr: SocketAddr) -> Peer {
        let (tx, rx) = channel();

        let mut state = PeerState {
            addr: addr,
            pending: None,
            stream: None,
            rx: rx
        };

        thread("Peer").spawn(move || {
            state.connect();
            state.message_loop();
        }).expect("Peer spawn failed");

        Peer {
            tx: tx
        }

    }

    /// Sends a message to this remote Peer
    pub fn send(&self, msg: Arc<ClusterMessage>) {
        self.tx.send(msg).expect("Peer channel disconnected");
    }
}

impl PeerState {
    fn check_overflow(&self) {
        // TODO
    }

    fn connect(&mut self) {
        if self.stream.is_none() {
            println!("Connecting to peer at {}...", self.addr);
            self.stream = TcpStream::connect(self.addr).ok();
        }
    }

    fn message_loop(&mut self) {
        loop {
            self.check_overflow();

            let msg = match self.pending.take() {
                Some(m) => m,
                None => {
                    match self.rx.recv() {
                        Ok(m) => m,
                        Err(_) => return
                    }
                }
            };

            self.connect();

            self.stream = match self.stream {
                Some(ref mut stream) => {
                    let limit = bincode::Infinite;

                    match bincode::serialize_into(stream, &msg, limit) {
                        Ok(_) => continue,
                        Err(e) => println!("Peer outgoing serialization failed: {}", e)
                    };

                    None
                },
                None => None
            };

            self.pending = Some(msg); // Message not sent, retry later
        }
    }

}

impl Server {
    pub fn spawn(addr: &SocketAddr, cluster: ClusterHandle) -> Server {
        let listener = TcpListener::bind(addr).expect("cluster::Server cannot bind");

        println!("Cluster Listening on: {}", addr);

        thread("cluster::Server").spawn(move || {
            Server::accept_loop(cluster, listener);
        }).expect("Could not start cluster::Server");

        Server {}
    }

    fn accept_loop(cluster: ClusterHandle, listener: TcpListener) {
        loop {
            let stream = listener.accept();

            match stream {
                Ok((stream, addr)) => {
                    // connection succeeded
                    println!("Peer Connection from: {}", addr);

                    let cluster = cluster.clone();

                    thread("cluster::Peer.incoming").spawn(move || {
                        Server::handle_peer(cluster, stream);
                    }).expect("Could not start cluster::Peer.incoming");
                },
                Err(e) => {
                    // connection failed
                    println!("Connection error: {}", e);
                }
            }
        }
    }

    fn handle_peer(cluster: ClusterHandle, mut stream: TcpStream) {
        loop {
            let limit = bincode::Bounded(10 * 1024 * 1024);

            match bincode::deserialize_from(&mut stream, limit) {
                Err(e) => {
                    println!("Bad message {:?}", e);
                    return;
                },
                Ok(msg) => cluster.handle_cluster_message(msg)
            };
        }
    }
}

fn thread(name: &str) -> Builder {
    Builder::new().name(name.into())
}

#[test]
fn test_cluster() {
    let replicas = vec![
        "127.0.0.1:1001".parse().unwrap(),
        "127.0.0.1:1002".parse().unwrap()
    ];

    use app;

    let id = "127.0.0.1:1000".parse().unwrap();
    let mut app = app::App::new(id);
    let mut cluster = Cluster::new(&mut app);

    cluster.add("127.0.0.1:1000".parse().unwrap());
    cluster.add("127.0.0.1:1001".parse().unwrap());
    cluster.add("127.0.0.1:1002".parse().unwrap());

    assert_eq!(cluster.replicas, replicas);
}
