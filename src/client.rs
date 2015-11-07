//! Represents a connected API client. Spins off 2 threads per client.

use std::io::prelude::*;
use std::net::{TcpStream};
use std::io::{BufReader, BufWriter};
use std::thread;
use std::sync::mpsc;

pub struct Client;

impl Client {
    /// Creates a new `Client` from a `TcpStream`
    pub fn new(stream: TcpStream) {
        thread::spawn(move|| {
            handle_client(stream);
            // end
        });
    }
}

fn handle_client(stream: TcpStream) {
    let (tx, rx) = mpsc::channel();
    let reader = BufReader::new(stream.try_clone().unwrap());
    let mut writer = BufWriter::new(stream);

    writer.write(b"// hello!\n").unwrap();
    writer.flush().unwrap();

    // Asynchronously write data to client
    async_writer(writer, rx);

    // Asynchronously ping
    pinger(tx.clone());

    // Read loop
    for line in reader.lines() {
        println!("{:?}", line);
        match line {
            Ok(line) => {
                tx.send(line).unwrap();
                // TODO: decode and dispatch to zone
            },
            Err(e) => {
                println!("Connection error: {}", e);
            }
        }
    }

    // Shutdown
}

fn pinger(tx: mpsc::Sender<String>) {
    thread::spawn(move|| {
        loop {
            thread::sleep_ms(5000);
            tx.send("// ping!".to_string()).unwrap();
        }
    });
}

fn async_writer(mut writer: BufWriter<TcpStream>, channel: mpsc::Receiver<String>) {
    thread::spawn(move|| {
        for message in channel {
            // TODO: test socket for writability
            writer.write(message.as_bytes()).unwrap();
            writer.flush().unwrap();
        }

        println!("Write: Hangup");
    });
}
