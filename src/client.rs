//! Represents a connected API client. Spins off 2 threads per client.

use std::io::prelude::*;
use std::net::{TcpStream};
use std::io::{BufReader, BufWriter};
use std::thread;
use std::sync::Arc;
use std::sync::mpsc;

use manager::Manager;

pub struct Client {
    manager: Arc<Manager>,
    stream: TcpStream
}

impl Client {
    /// Creates a new `Client` from a `TcpStream`
    pub fn new(manager: Arc<Manager>, stream: TcpStream) {
        let client = Client {
            manager: manager,
            stream: stream
        };

        thread::spawn(move|| {
            client.handle_stream();
            // end
        });
    }

    fn handle_stream(&self) {
        let (tx, rx) = mpsc::channel();

        // Asynchronously write data to client
        self.create_writer_thread(rx);

        tx.send("// hello!".to_string()).unwrap();

        // Asynchronously ping
        pinger(tx.clone());

        let reader = BufReader::new(self.stream.try_clone().unwrap());

        // Read loop
        for line in reader.lines() {
            println!("{:?}", line);
            match line {
                Ok(line) => {
                    let loaded = self.manager.zone_loaded(&line);

                    tx.send(loaded.to_string()).unwrap();

                    // TODO: decode and dispatch to zone
                },
                Err(e) => {
                    println!("Connection error: {}", e);
                }
            }
        }

        // Shutdown
    }

    fn create_writer_thread(&self, channel: mpsc::Receiver<String>) {
        let mut writer = BufWriter::new(self.stream.try_clone().unwrap());

        thread::spawn(move|| {
            for message in channel {
                // TODO: test socket for writability
                writer.write(message.as_bytes()).unwrap();
                writer.write(b"\n").unwrap();
                writer.flush().unwrap();
            }

            println!("Write: Hangup");
        });
    }
}

fn pinger(tx: mpsc::Sender<String>) {
    thread::spawn(move|| {
        loop {
            thread::sleep_ms(5000);
            tx.send("// ping!".to_string()).unwrap();
        }
    });
}
