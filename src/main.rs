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

    let port = std::env::var("PORT").unwrap_or("".to_string()).parse().unwrap_or(8888);

    let server = server::Server::new(manager.clone(), port);

    println!("root loaded: {}", manager.zone_loaded(&path));

    server.listen();

    println!("listening on port: {}", port);

    loop {
        std::thread::park();
    }
}
