use std::io::{BufReader, BufWriter};
use std::io::prelude::*;
use std::net::{TcpListener, TcpStream};
use std::thread;

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
    // accept connections and process them, spawning a new thread for each one
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                thread::spawn(move|| {
                    // connection succeeded
                    handle_client(stream);
                });
            }
            Err(e) => {
                // connection failed
                println!("Connection error: {}", e);
            }
        }
    }
}

fn handle_client(stream: TcpStream) {
    let reader = BufReader::new(stream.try_clone().unwrap());
    let mut writer = BufWriter::new(stream);

    writer.write(b"hello world\n").unwrap();
    writer.flush().unwrap();

    for line in reader.lines() {
        println!("{:?}", line);
        writer.write(line.unwrap().as_bytes()).unwrap();
        writer.flush().unwrap();
    }
}
