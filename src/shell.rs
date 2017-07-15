use std::io::prelude::*;
use std::process;

use app::{App, AppHandle};
use path::Path;

struct Shell<W> {
    app: AppHandle,
    writer: W
}

pub fn start<R: BufRead, W: Write>(app: App, reader: R, writer: W) {
    let mut s = Shell {
        app: app.handle(),
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

                let mut line = line.splitn(2, ' ');

                match line.next() {
                    Some("active") => self.active(),
                    Some("cluster.sync") => self.sync(),
                    Some("cluster.sync_all") => self.sync_all(),
                    Some("store.dump") => self.store_dump(line.next().unwrap_or_default()),
                    Some("stats") => self.stats(),
                    Some("zone.dump") => self.zone_dump(line.next().unwrap_or_default()),
                    Some("zone.sync") => self.zone_sync(line.next().unwrap_or_default()),
                    Some("exit") | Some("quit") | Some("shutdown") => self.shutdown(),
                    Some("") => (),
                    _ => writeln!(self.writer, "Bad command").unwrap()
                }

                self.writer.write(b"> ").unwrap();
                self.writer.flush().unwrap();
            }
        }
    }

    fn active(&mut self) {
        let active_zones = self.app.manager.list();
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

    fn stats(&mut self) {
        use serde_json;

        writeln!(self.writer, "{}", serde_json::to_string_pretty(&*self.app.stats).unwrap()).unwrap();
    }

    fn sync(&mut self) {
        writeln!(self.writer, "Synchronizing local data with cluster...").unwrap();
        self.app.cluster.sync();
    }

    fn sync_all(&mut self) {
        writeln!(self.writer, "Synchronizing cluster data...").unwrap();
        self.app.cluster.sync_all();
    }

    fn store_dump(&mut self, path: &str) {
        let path = match path {
            "" => Path::new(vec![]),
            _ => Path::new(path.split('.').map(|s| s.into()).collect())
        };

        match self.app.store.load_data(path.clone()) {
            None => writeln!(self.writer, "Could not load {:?}", path),
            Some(data) => writeln!(self.writer, "Store data: {:?}", data)
        }.unwrap();
    }

    fn zone_dump(&mut self, path: &str) {
        let path = match path {
            "" => Path::new(vec![]),
            _ => Path::new(path.split('.').map(|s| s.into()).collect())
        };

        let zone = self.app.manager.load(&path);

        let data = zone.dump();

        writeln!(self.writer, "Zone data: {:#?}", data).unwrap();
    }

    fn zone_sync(&mut self, path: &str) {
        let path = match path {
            "" => Path::new(vec![]),
            _ => Path::new(path.split('.').map(|s| s.into()).collect())
        };

        writeln!(self.writer, "Synchronizing zone {:#?}...", &path).unwrap();
        self.app.cluster.sync_zone(path);
    }
}
