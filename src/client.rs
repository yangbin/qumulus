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

use app::AppHandle;
use command::Command;
use node::{DelegatedMatch, Update};
use path::Path;

pub struct Client {
    app: AppHandle,
    stream: TcpStream,
    tx: Sender<String>
}

impl Client {
    /// Creates a new `Client` from a `TcpStream`
    pub fn new(app: AppHandle, stream: TcpStream) {
        let (tx, rx) = channel();

        let client = Client {
            app: app,
            stream: stream,
            tx: tx
        };

        mioco::spawn(move|| {
            client.app.stats.clients.connects.increment();

            // Asynchronously write data to client
            client.create_writer_thread(rx);

            // Handle reads
            client.handle_stream();

            // end
            client.app.stats.clients.disconnects.increment();
        });
    }

    fn handle_stream(&self) {
        self.tx.send("{ \"hello!\": 1 }".to_string()).unwrap();

        // Asynchronously ping
        pinger(self.tx.clone());

        let reader = BufReader::new(self.stream.try_clone().unwrap());

        let (commands_tx, commands_rx) = mioco::sync::mpsc::channel::<Command>();

        let commands_rx = Arc::new(Mutex::new(commands_rx));

        // Pipeline up to 1000 commands at a time
        for _ in 0..1000 {
            let commands_rx = commands_rx.clone();
            let app = self.app.clone();
            let tx = self.tx.clone();

            mioco::spawn(move|| {
                loop {
                    // quit if disconnected
                    let command = match commands_rx.lock() {
                        Ok(c) => match c.recv() {
                            Ok(c) => c,
                            Err(_) => return
                        },
                        Err(_) => return
                    };

                    process(&app, &tx, command);
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
        // command_tx is dropped here, threads using command_rx will panic
    }

    fn create_writer_thread(&self, channel: Receiver<String>) {
        let mut writer = self.stream.try_clone().unwrap();

        mioco::spawn(move|| {
            loop {
                let message = match channel.recv() {
                    Ok(message) => message,
                    Err(_) => return
                };


                // TODO: test socket for writability
                if let Err(_) = writer.write_all(message.as_bytes()) {
                    return;
                }

                if let Err(_) = writer.write(b"\n") {
                    return;
                }
            }
        });
    }
}

/// Process a single command from client. Recursively dispatch for delegated zones.
fn process(app: &AppHandle, tx: &Sender<String>, mut command: Command) {
    let resolved_path = command.path.resolved();
    let (prefix, zone) = app.manager.find_nearest(&resolved_path);

    let c = Command {
        path: command.path.slice(prefix.len()),
        params: mem::replace(&mut command.params, Value::Null),
        ..command
    };

    app.stats.clients.commands.increment(&c.call);

    let mut result = zone.dispatch(c, tx);

    let mut queue: VecDeque<DelegatedMatch> = VecDeque::new();

    for mut d in result.delegated.drain(..) {
        // Convert relative paths to absolute
        let mut path = prefix.clone();

        path.append(&mut d.path);
        d.path = path;
        queue.push_back(d);
    }

    reply(app, tx, command.id, queue.len() as u64, &prefix, result.update);

    if ! command.recursive() {
        return;
    }

    while let Some(delegated) = queue.pop_front() {
        let zone = app.manager.load(&delegated.path);

        let c = Command {
            path: delegated.match_spec,
            params: Value::Null,
            ..command
        };

        let result = zone.dispatch(c, tx);

        for mut d in result.delegated {
            let mut path = delegated.path.clone();

            path.append(&mut d.path);
            d.path = path;
            queue.push_back(d);
        }

        reply(app, tx, command.id, queue.len() as u64, &delegated.path, result.update);
    }

    fn reply(app: &AppHandle, tx: &Sender<String>, id: u64, left: u64, path: &Path, update: Option<Update>) {
        let response = vec![
            id.into(),
            left.into(),
            path.to_json(),
            update.map_or(Value::Null, |u| u.to_json())
        ];

        app.stats.clients.replies.increment();

        // TODO stop processing if unable to reply, otherwise we're just wasting cycles
        tx.send(serde_json::to_string(&response).unwrap()).unwrap_or_default();
    }
}

fn pinger(tx: Sender<String>) {
    mioco::spawn(move|| {
        loop {
            mioco::sleep(Duration::from_secs(60));

            if let Err(_) = tx.send("{ \"ping\": 1 }".to_string()) {
                // hung up
                return;
            }
        }
    });
}
