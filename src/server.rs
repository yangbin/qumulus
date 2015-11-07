use std::net::TcpListener;
use std::thread;

use client::Client;

pub struct Server {
    port: u16
}

impl Server {
    pub fn new(port: u16) -> Server {
        Server {
            port: port
        }
    }

    pub fn listen(&self) {
        let listener = TcpListener::bind(("127.0.0.1", self.port)).unwrap();

        thread::spawn(move|| {
            accept_loop(listener);
        });
    }

}

fn accept_loop(listener: TcpListener) {
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                // connection succeeded
                println!("Connection from: {}", stream.peer_addr().unwrap());
                Client::new(stream);
            },
            Err(e) => {
                // connection failed
                println!("Connection error: {}", e);
            }
        }
    }
}
