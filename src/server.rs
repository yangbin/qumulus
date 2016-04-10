use std::net::TcpListener;
use std::thread;

use client::Client;
use manager::ManagerHandle;

pub struct Server {
    port: u16,
    manager: ManagerHandle
}

impl Server {
    pub fn new(manager: ManagerHandle, port: u16) -> Server {
        Server {
            port: port,
            manager: manager
        }
    }

    pub fn listen(&self) {
        let manager = self.manager.clone();
        let listener = TcpListener::bind(("127.0.0.1", self.port)).unwrap();

        thread::spawn(move|| {
            accept_loop(manager, listener);
        });
    }
}

fn accept_loop(manager: ManagerHandle, listener: TcpListener) {
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                // connection succeeded
                println!("Connection from: {}", stream.peer_addr().unwrap());
                Client::new(manager.clone(), stream);
            },
            Err(e) => {
                // connection failed
                println!("Connection error: {}", e);
            }
        }
    }
}
