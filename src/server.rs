//use std::net::TcpListener;
use std::thread;

use mioco::tcp::TcpListener;
use mioco;

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
        let port = self.port;

        thread::spawn(move|| {
            mioco::start(move|| {
                use std::net::SocketAddr;

                let addr = SocketAddr::new("127.0.0.1".parse().unwrap(), port);
                let listener = TcpListener::bind(&addr).unwrap();

                println!("Listening on {:?}", listener.local_addr());

                accept_loop(manager, listener);
            }).unwrap();
        });
    }
}

fn accept_loop(manager: ManagerHandle, listener: TcpListener) {
    loop {
        let stream = listener.accept();

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
