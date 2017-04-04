//! A distributed hierarchical data distribution thingy

#![recursion_limit="128"]

#![feature(associated_consts)]
#![feature(custom_derive)]

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

pub mod client;
pub mod command;
pub mod delegate;
pub mod listener;
pub mod manager;
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

    let store = store::fs::FS::spawn(&("data_".to_string() + id_str));
    let manager = manager::Manager::spawn(store);

    let path = path::Path::empty();
    manager.load(&path);

    let server = server::Server::new(manager.clone(), id.addr());

    println!("root loaded: {}", manager.zone_loaded(&path));

    server.listen();

    println!("listening on: {}", id.addr());

    let stdin = std::io::stdin();

    shell::start(manager.clone(), stdin.lock(), std::io::stdout());

    loop {
        std::thread::park();
    }
}
