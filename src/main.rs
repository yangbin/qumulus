//! A distributed hierarchical data distribution thingy

extern crate serde;
extern crate serde_json;
extern crate time;

pub mod client;
pub mod command;
pub mod manager;
pub mod node;
pub mod path;
pub mod server;
pub mod zone;

fn main() {
    println!("Qumulus v0.0.1");

    let manager = manager::Manager::new();

    let path = path::Path::new(vec![]);
    manager.load(path.clone());

    let server = server::Server::new(manager.clone(), 8888);

    println!("root loaded: {}", manager.zone_loaded(&path));

    server.listen();

    loop {
        std::thread::park();
    }
}
