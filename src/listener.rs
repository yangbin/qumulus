use std::sync::Mutex;
use std::sync::mpsc::SendError;

use mioco::sync::mpsc::Sender;
use serde_json;

use node::Update;
use path::Path;

pub struct Listener {
    pub path: Path,
    pub tx: Mutex<Sender<String>>
}

impl Listener {
    pub fn new(path: &Path, tx: &Sender<String>) -> Listener {
        Listener {
            path: path.clone(),
            tx: Mutex::new(tx.clone())
        }
    }

    pub fn update(&self, update: &Update) -> Result<(), SendError<String>> {
        let json = update.to_json();
        let str = serde_json::to_string(&json).unwrap();
        let tx = self.tx.lock().unwrap();

        tx.send("[0, ".to_string() + &str + "]")
    }
}
