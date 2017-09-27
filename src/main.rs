//! A distributed hierarchical data distribution thingy

#![recursion_limit="128"]

extern crate bincode;
extern crate env_logger;
#[macro_use] extern crate log;
extern crate mioco;
extern crate rand;
extern crate serde;
extern crate serde_json;
#[macro_use] extern crate serde_derive;
extern crate threadpool;
extern crate time;

pub mod app;
pub mod client;
pub mod cluster;
pub mod command;
pub mod delegate;
pub mod listener;
pub mod manager;
pub mod monitor;
pub mod node;
#[macro_use] pub mod path;
pub mod replica;
pub mod shell;
pub mod server;
pub mod store;
pub mod value;
pub mod zone;

fn main() {
    env_logger::init().unwrap();

    println!("Qumulus v0.0.1");

    let args: Vec<_> = std::env::args().collect();

    if args.len() != 2 {
        println!("Usage: {} <ID>", &args[0]);
        println!("Missing ID. ID must be provided as an IP:port string.");
        println!("This is used as the listening address as well as the data directory.");

        return;
    }

    let id_str = &args[1];
    let id: replica::Replica = id_str.parse().unwrap();

    println!("  ID / address: {:?}", &id);

    let mut app = app::App::new(id.clone());

    store::fs::FS::spawn(&mut app);
    manager::Manager::spawn(&mut app);
    cluster::Cluster::spawn(&mut app);

    app.manager.load(&path::Path::empty());

    println!("Listening addresses:");
    println!("  API: {}", id.api_addr());
    println!("  Peer: {}", id.peer_addr());
    println!("  Monitor: {}", id.monitor_addr());

    let server = server::Server::new(&app, id.api_addr());
    server.listen();

    let replicas: Vec<replica::Replica> = match std::env::var("CLUSTER") {
        Ok(r) => r.split(' ').map(|r| r.parse().unwrap()).collect(),
        Err(_) => vec![]
    };

    println!("Adding replicas:");

    for replica in replicas {
        println!("  {:?}", &replica);
        app.cluster.add(replica);
    }

    monitor::Monitor::spawn(&app);

    let stdin = std::io::stdin();

    shell::start(app, stdin.lock(), std::io::stdout());

    loop {
        std::thread::park();
    }
}
