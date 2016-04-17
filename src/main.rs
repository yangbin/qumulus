//! A distributed hierarchical data distribution thingy

extern crate serde;
extern crate serde_json;
extern crate time;

pub mod client;
pub mod command;
pub mod delegate;
pub mod listener;
pub mod manager;
pub mod node;
pub mod path;
pub mod shell;
pub mod server;
pub mod zone;

fn main() {
    println!("Qumulus v0.0.1");

    let manager = manager::Manager::spawn();

    let path = path::Path::empty();
    manager.load(&path);

    let port = std::env::var("PORT").unwrap_or("".to_string()).parse().unwrap_or(8888);

    let server = server::Server::new(manager.clone(), port);

    println!("root loaded: {}", manager.zone_loaded(&path));

    server.listen();

    println!("listening on port: {}", port);

    let stdin = std::io::stdin();

    shell::start(manager.clone(), stdin.lock(), std::io::stdout());

    loop {
        std::thread::park();
    }
}
