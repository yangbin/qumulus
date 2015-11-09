extern crate serde;
extern crate serde_json;

pub mod client;
pub mod manager;
pub mod node;
pub mod server;

fn main() {
    println!("Qumulus v0.0.1");

    let manager = manager::Manager::new();

    manager.load("root");

    let server = server::Server::new(manager.clone(), 8888);

    println!("root loaded: {}", manager.zone_loaded("root"));

    server.listen();

    loop {
        std::thread::park();
    }
}
