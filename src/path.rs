//! Represents a path to a subtree / node. Ordered so we can iterate through paths in a BTreeMap

use serde_json::Value;

#[derive(Clone, Debug, Default, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct Path {
    pub path: Vec<String>
}

impl Path {
    pub fn empty() -> Path {
        Path { path: vec![] }
    }

    pub fn new(path: Vec<String>) -> Path {
        Path { path: path }
    }

    pub fn append(&mut self, path: &mut Path) {
        self.path.append(&mut path.path);
    }

    pub fn push(&mut self, component: &String) {
        // TODO: unncessary copy
        self.path.push(component.clone());
    }

    pub fn pop(&mut self) -> Option<String> {
        self.path.pop()
    }

    pub fn len(&self) -> usize {
        self.path.len()
    }

    pub fn truncate(&mut self, len: usize) {
        self.path.truncate(len);
    }

    /// Returns a new `Path` prefix that is fully resolved, i.e. no wildcards
    pub fn resolved(&self) -> Path {
        let prefix = self.path.iter().take_while(|p| p.starts_with("#"));

        Path::new(prefix.cloned().collect())
    }

    pub fn slice(&self, n: usize) -> Path {
        Path::new(self.path[n..].to_vec())
    }

    pub fn to_json(&self) -> Value {
        Value::Array(self.path.iter().map(|p| Value::String(p.clone())).collect())
    }
}

macro_rules! path {
    ( $($p:ident).+ ) => {
        {
            Path {
                path: vec![
                $(
                    stringify!($p).to_string(),
                )+
                ]
            }
        }
    }
}

#[test]
fn test_macro() {
    assert_eq!(Path { path: vec!["root".to_string()] }, path!(root));
    assert_eq!(Path { path: vec!["root".to_string(), "moo".to_string()] }, path!(root.moo));
}

#[test]
fn test_push() {
    let mut p = path!(root);

    p.push(&"moo".to_string());

    assert_eq!(p, path!(root.moo));
}

#[test]
fn test_pop() {
    let mut p = path!(root.moo.cow);

    assert_eq!(p.pop(), Some("cow".to_string()));
    assert_eq!(p.pop(), Some("moo".to_string()));
}
