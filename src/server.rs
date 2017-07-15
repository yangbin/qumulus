use std::net::SocketAddr;
use std::thread;

use mioco::tcp::TcpListener;
use mioco;

use app::{App, AppHandle};
use client::Client;

pub struct Server {
    addr: SocketAddr,
    app: AppHandle
}

impl Server {
    pub fn new(app: &App, addr: SocketAddr) -> Server {
        Server {
            addr: addr,
            app: app.handle()
        }
    }

    pub fn listen(&self) {
        let addr = self.addr.clone();
        let app = self.app.clone();

        thread::spawn(move|| {
            mioco::start(move|| {
                let listener = TcpListener::bind(&addr).unwrap();

                accept_loop(app, listener);
            }).unwrap();
        });
    }
}

fn accept_loop(app: AppHandle, listener: TcpListener) {
    loop {
        let stream = listener.accept();

        match stream {
            Ok(stream) => {
                // connection succeeded
                println!("Connection from: {}", stream.peer_addr().unwrap());
                Client::new(app.clone(), stream);
            },
            Err(e) => {
                // connection failed
                println!("Connection error: {}", e);
            }
        }
    }
}
