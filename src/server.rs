use std::net::SocketAddr;
use std::thread;

use mioco::tcp::TcpListener;
use mioco;

use client::Client;
use manager::ManagerHandle;

pub struct Server {
    addr: SocketAddr,
    manager: ManagerHandle
}

impl Server {
    pub fn new(manager: ManagerHandle, addr: &SocketAddr) -> Server {
        Server {
            addr: addr.clone(),
            manager: manager
        }
    }

    pub fn listen(&self) {
        let manager = self.manager.clone();
        let addr = self.addr.clone();

        thread::spawn(move|| {
            mioco::start(move|| {
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
