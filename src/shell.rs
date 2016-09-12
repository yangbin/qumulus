use std::io::prelude::*;
use std::process;

use manager::ManagerHandle;

struct Shell<W> {
    manager: ManagerHandle,
    writer: W
}

pub fn start<R: BufRead, W: Write>(manager: ManagerHandle, reader: R, writer: W) {
    let mut s = Shell {
        manager: manager,
        writer: writer
    };

    s.command_loop(reader);
}

impl<W: Write> Shell<W> {
    pub fn command_loop<R: BufRead>(&mut self, reader: R) {
        self.writer.write(b"> ").unwrap();
        self.writer.flush().unwrap();

        // Read loop
        for line in reader.lines() {
            if let Ok(line) = line {
                match line.as_ref() {
                    "active" => self.active(),
                    "exit" | "quit" | "shutdown" => self.shutdown(),
                    "" => (),
                    _ => println!("Bad command")
                }

                self.writer.write(b"> ").unwrap();
                self.writer.flush().unwrap();
            }
        }
    }

    fn active(&mut self) {
        let active_zones = self.manager.list();
        let len = active_zones.len();

        writeln!(self.writer, "Active Zones:").unwrap();

        for z in active_zones {
            let path = z.path().path.join(".");
            let size = z.size();
            let state = z.state();

            writeln!(self.writer, "{:>8} {:?} {:?}", size, state, path).unwrap();
        }

        writeln!(self.writer, "Total: {} active zones", len).unwrap();
    }

    fn shutdown(&mut self) {
        writeln!(self.writer, "Shutting down...").unwrap();

        // TODO: exit is not clean, destructors not called, files/sockets not flushed
        process::exit(0);
    }
}
