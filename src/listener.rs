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

impl Listener {
    pub fn new(root: Arc<Path>, path: Arc<Path>, tx: &Sender<String>) -> Listener {
        Listener {
            root: root,
            path: path,
            tx: tx.clone()
        }
    }

    pub fn update(&self, update: &Update) -> Result<(), SendError<String>> {
        let req_id = Value::U64(0);
        let root = Value::Array(self.root.path.iter().map(|s| { Value::String(s.clone()) }).collect());
        let update = update.to_json();
        let json = Value::Array(vec![req_id, Value::Null, root, update]);
        let str = serde_json::to_string(&json).unwrap();

        self.tx.send(str)
    }
}
