use std::sync::Arc;
use std::sync::mpsc::SendError;

use mioco::sync::mpsc::Sender;
use serde_json;
use serde_json::value::Value;

use node::Update;
use path::Path;

pub struct Listener {
    pub root: Arc<Path>,
    pub path: Arc<Path>,
    pub tx: Sender<String>
}

/// A Relative Listeer
pub struct RListener {
    pub path: Path,
    pub tx: Sender<String>
}

impl Listener {
    pub fn new(root: Arc<Path>, path: Arc<Path>, tx: Sender<String>) -> Listener {
        Listener {
            root: root,
            path: path,
            tx: tx
        }
    }

    pub fn update(&self, update: &Update) -> Result<(), SendError<String>> {
        let req_id: Value = 0.into();
        let root = Value::Array(self.root.path.iter().map(|s| { Value::String(s.clone()) }).collect());
        let update = update.filter(&self.path.path[..]);

        if update == Value::Null {
            return Ok(());
        }

        let json = Value::Array(vec![req_id, Value::Null, root, update]);
        let str = serde_json::to_string(&json).unwrap();

        self.tx.send(str)
    }

    /// Computes whether listener is retained and/or delegated
    pub fn delegate(&self, d_path: &Path) -> (bool, Option<RListener>) {
        let (retain, path) = self.path.delegate(d_path);
        let d_listener = path.map(|p| RListener::new(p, &self.tx.clone()));

        (retain, d_listener)
    }
}

impl RListener {
    pub fn new(path: Path, tx: &Sender<String>) -> RListener {
        RListener {
            path: path,
            tx: tx.clone()
        }
    }

    pub fn to_absolute(self, path: Arc<Path>) -> Listener {
        Listener::new(path, Arc::new(self.path), self.tx)
    }
}
