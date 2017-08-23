//! Simple monitor, allows a single REST call to retrieve stats

use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::thread::Builder;

use app::{App, AppHandle};
use replica::Replica;

pub struct Monitor {
    app: AppHandle,
    id: Replica
}

pub struct Server {
    app: AppHandle,
    listener: TcpListener
}

impl Monitor {
    pub fn new(app: &App) -> Monitor {
        Monitor {
            app: app.handle(),
            id: app.id.clone()
        }
    }

    /// Start the Monitor "process".
    pub fn spawn(app: &App) {
        let mut monitor = Monitor::new(app);

        thread("Monitor").spawn(move || {
            monitor.run();
        }).expect("Monitor spawn failed");
    }

    pub fn run(&mut self) {
        let server = Server::new(&self.id.monitor_addr(), self.app.clone());

        server.accept_loop();
    }
}

impl Server {
    pub fn new(addr: &SocketAddr, app: AppHandle) -> Server {
        println!("Monitor Listening on: {}", addr);

        let listener = TcpListener::bind(addr).expect("monitor::Server cannot bind");

        Server {
            app: app,
            listener: listener
        }
    }

    fn accept_loop(&self) {
        loop {
            let stream = self.listener.accept();

            match stream {
                Ok((stream, addr)) => {
                    // connection succeeded
                    println!("Peer Connection from: {}", addr);

                    self.handle(stream);
                },
                Err(e) => {
                    // connection failed
                    println!("Monitor connection error: {}", e);
                }
            }
        }
    }

    fn handle(&self, mut stream: TcpStream) {
        use serde_json;

        println!("insert HTTP response here");

        let stats = serde_json::to_string_pretty(&*self.app.stats).unwrap();

        stream.write("HTTP/1.1 200 OK\r
Access-Control-Allow-Origin: *\r
\r
".as_bytes()).unwrap();
        let _ = stream.write(stats.as_bytes());
        let _ = stream.flush();
        let _ = stream.shutdown(Shutdown::Both);

        let mut buffer = Vec::new();

        let _ = stream.read_to_end(&mut buffer);
    }
}

fn thread(name: &str) -> Builder {
    Builder::new().name(name.into())
}
