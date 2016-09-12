//! Represents a connected API client. Spins off 2 threads per client.

use std::collections::VecDeque;
use std::io::prelude::*;
use std::io::BufReader;
use std::mem;
use std::sync::Arc;
use std::time::Duration;

use mioco::sync::mpsc::{channel, Receiver, Sender};
use mioco::sync::Mutex;
use mioco::tcp::TcpStream;
use mioco;
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
        let (tx, rx) = channel();

        let client = Client {
            manager: manager,
            stream: stream,
            tx: tx
        };

        mioco::spawn(move|| {
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

        let (commands_tx, commands_rx) = mioco::sync::mpsc::channel::<Command>();

        let commands_rx = Arc::new(Mutex::new(commands_rx));

        // Pipeline up to 5 commands at a time
        for _ in 0..5 {
            let commands_rx = commands_rx.clone();
            let manager = self.manager.clone();
            let tx = self.tx.clone();

            mioco::spawn(move|| {
                loop {
                    let command = { commands_rx.lock().unwrap().recv() }.unwrap();

                    process(&manager, &tx, command);
                }
            });
        }

        // Read loop, push decoded commands into queue
        for line in reader.lines() {
            match line {
                Ok(line) => {
                    match Command::from_json(&line) {
                        Ok(command) => {
                            commands_tx.send(command).unwrap();
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

    fn create_writer_thread(&self, channel: Receiver<String>) {
        let mut writer = self.stream.try_clone().unwrap();

        mioco::spawn(move|| {
            loop {
                let message = channel.recv().unwrap();

                // TODO: test socket for writability
                writer.write_all(message.as_bytes()).unwrap();
                writer.write(b"\n").unwrap();
            }
        });
    }
}

/// Process a single command from client. Recursively dispatch for delegated zones.
fn process(manager: &ManagerHandle, tx: &Sender<String>, mut command: Command) {
    let resolved_path = command.path.resolved();
    let (prefix, zone) = manager.find_nearest(&resolved_path);

    let c = Command {
        path: command.path.slice(prefix.len()),
        params: mem::replace(&mut command.params, Value::Null),
        ..command
    };

    let mut result = zone.dispatch(c, tx);

    let mut queue: VecDeque<DelegatedMatch> = result.delegated.drain(..).collect();

    reply(tx, command.id, queue.len() as u64, &prefix, result.update);

    if ! command.recursive() {
        return;
    }

    while let Some(delegated) = queue.pop_front() {
        match manager.find(&delegated.path) {
            Some(zone) => {
                let c = Command {
                    path: delegated.match_spec,
                    params: Value::Null,
                    ..command
                };

                let result = zone.dispatch(c, tx);

                for d in result.delegated {
                    queue.push_back(d);
                }

                reply(tx, command.id, queue.len() as u64, &delegated.path, result.update);
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

fn pinger(tx: Sender<String>) {
    mioco::spawn(move|| {
        loop {
            mioco::sleep(Duration::from_secs(60));
            tx.send("{ \"ping\": 1 }".to_string()).unwrap();
        }
    });
}
