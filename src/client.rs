//! Represents a connected API client. Spins off 2 threads per client.

use std::collections::VecDeque;
use std::io::prelude::*;
use std::io::{BufReader, BufWriter};
use std::mem;
use std::net::TcpStream;
use std::sync::mpsc;
use std::sync::mpsc::{Receiver, Sender};
use std::thread;
use std::time::Duration;

use serde_json;
use serde_json::Value;

use command::Command;
use manager::ManagerHandle;
use node::{DelegatedMatch, Update};
use path::Path;

pub struct Client {
    manager: ManagerHandle,
    stream: TcpStream,
    tx: Sender<String>
}

impl Client {
    /// Creates a new `Client` from a `TcpStream`
    pub fn new(manager: ManagerHandle, stream: TcpStream) {
        let (tx, rx) = mpsc::channel();

        let client = Client {
            manager: manager,
            stream: stream,
            tx: tx
        };

        thread::spawn(move|| {
            // Asynchronously write data to client
            client.create_writer_thread(rx);

            // Handle reads
            client.handle_stream();

            // end
        });
    }

    fn handle_stream(&self) {
        self.tx.send("{ \"hello!\": 1 }".to_string()).unwrap();

        // Asynchronously ping
        pinger(self.tx.clone());

        let reader = BufReader::new(self.stream.try_clone().unwrap());

        // Read loop
        for line in reader.lines() {
            match line {
                Ok(line) => {
                    match Command::from_json(&line) {
                        Ok(command) => {
                            self.process(command);
                        },
                        Err(e) => {
                            self.tx.send("[0,\"error\",\"".to_string() + &e + "\"]").unwrap();
                        }
                    }
                },
                Err(e) => {
                    println!("Connection error: {}", e);
                }
            }
        }

        // Shutdown
    }

    /// Process a single command from client. Recursively dispatch for delegated zones.
    fn process(&self, mut command: Command) {
        let resolved_path = command.path.resolved();
        let (prefix, zone) = self.manager.find_nearest(&resolved_path);

        let c = Command {
            path: command.path.slice(prefix.len()),
            params: mem::replace(&mut command.params, Value::Null),
            ..command
        };

        let mut result = zone.dispatch(c, &self.tx);

        let mut queue: VecDeque<DelegatedMatch> = result.delegated.drain(..).collect();

        reply(&self.tx, command.id, queue.len() as u64, &prefix, result.update);

        if ! command.recursive() {
            return;
        }

        while let Some(delegated) = queue.pop_front() {
            match self.manager.find(&delegated.path) {
                Some(zone) => {
                    let c = Command {
                        path: delegated.match_spec,
                        params: Value::Null,
                        ..command
                    };

                    let result = zone.dispatch(c, &self.tx);

                    for d in result.delegated {
                        queue.push_back(d);
                    }

                    reply(&self.tx, command.id, queue.len() as u64, &delegated.path, result.update);
                },
                None => unimplemented!()
            }
        }

        fn reply(tx: &Sender<String>, id: u64, left: u64, path: &Path, update: Option<Update>) {
            let response = vec![
                serde_json::value::to_value(&id),
                serde_json::value::to_value(&left),
                path.to_json(),
                update.map_or(Value::Null, |u| u.to_json())
            ];

            tx.send(serde_json::to_string(&response).unwrap()).unwrap();
        }
    }

    fn create_writer_thread(&self, channel: Receiver<String>) {
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

fn pinger(tx: Sender<String>) {
    thread::spawn(move|| {
        loop {
            thread::sleep(Duration::from_secs(5000));
            tx.send("{ \"ping\": 1 }".to_string()).unwrap();
        }
    });
}
