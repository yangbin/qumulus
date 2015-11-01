extern crate serde;
extern crate serde_json;

pub mod node;
pub mod server;

fn main() {
    println!("Qumulus v0.0.1");

    let server = server::Server::new(8888);

    server.listen();

    loop {
        std::thread::park();
    }
}
