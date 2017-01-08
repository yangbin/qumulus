//! Represents a path to a subtree / node. Ordered so we can iterate through paths in a BTreeMap

use serde_json::Value;

#[derive(Clone, Debug, Default, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct Path {
    pub path: Vec<String>
}

macro_rules! path {
    ( $($p:tt).* ) => {
        {
            Path {
                path: vec![
                $(
                    match stringify!($p) {
                        "%" => "**".to_string(),
                        p => p.to_string()
                    },
                )*
                ]
            }
        }
    }
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

    pub fn delegate(&self, d_path: &Path) -> (bool, Option<Path>) {
        let mut iter = self.path.iter();
        let mut retain = false;

        for d in &d_path.path {
            let p = iter.next();

            match p {
                None => {
                    // Listener path shorter then delegate path
                    return (true, None);
                },
                Some(p) if p == d => {
                    // Part matches, so continue
                    continue;
                },
                Some(p) if &*p == "*" => {
                    // Wildcard matches, retain and continue
                    retain = true;
                    continue;
                },
                Some(p) if &*p == "**" => {
                    // Recursiive, retain and return delegated
                    let listener = path!(%);
                    return (true, Some(listener));
                },
                _ => {
                    // Not matching, just retain
                    return (true, None);
                }
            }
        }

        let mut path = vec![];

        path.extend(iter.cloned());
        (retain, Some(Path::new(path)))
    }

    pub fn slice(&self, n: usize) -> Path {
        Path::new(self.path[n..].to_vec())
    }

    pub fn to_json(&self) -> Value {
        Value::Array(self.path.iter().map(|p| Value::String(p.clone())).collect())
    }
}

#[test]
fn test_macro() {
    assert_eq!(path(vec!["root"]), path!(root));
    assert_eq!(path(vec!["root", "moo"]), path!(root.moo));
    assert_eq!(path(vec!["*"]), path!(*));
    assert_eq!(path(vec!["**"]), path!(%));
    assert_eq!(path(vec!["root", "*"]), path!(root.*));
    assert_eq!(path(vec!["root", "*", "moo"]), path!(root.*.moo));
    assert_eq!(path(vec!["root", "**"]), path!(root.%));

    fn path(path: Vec<&str>) -> Path {
        Path { path: path.iter().map(|p| p.to_string()).collect() }
    }
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

#[test]
fn test_delegate_match() {
    use mioco::sync::mpsc::channel;

    let (r, p) = d(path!(root.moo.cow), path!(root.moo));
    assert!(!r);
    assert_eq!(p.unwrap(), path!(cow));

    let (r, p) = d(path!(root.moo.cow), path!(root.cow));
    assert!(r);
    assert!(p.is_none());

    let (r, p) = d(path!(root.moo), path!(root.moo.cow));
    assert!(r);
    assert!(p.is_none());

    let (r, p) = d(path!(root.*), path!(root.moo));
    assert!(r);
    assert_eq!(p.unwrap(), path!());

    let (r, p) = d(path!(root.*), path!(root.moo.cow));
    assert!(r);
    assert!(p.is_none());

    let (r, p) = d(path!(*), path!(moo));
    assert!(r);
    assert_eq!(p.unwrap(), path!());

    let (r, p) = d(path!(*), path!(moo.cow));
    assert!(r);
    assert!(p.is_none());

    let (r, p) = d(path!(*.moo), path!(moo));
    assert!(r);
    assert_eq!(p.unwrap(), path!(moo));

    let (r, p) = d(path!(*.moo), path!(moo.cow));
    assert!(r);
    assert!(p.is_none());

    let (r, p) = d(path!(*.moo), path!(moo.moo));
    assert!(r);
    assert_eq!(p.unwrap(), path!());

    // % means **
    let (r, p) = d(path!(%), path!(moo));
    assert!(r);
    assert_eq!(p.unwrap(), path!(%));

    let (r, p) = d(path!(%), path!(moo.moo));
    assert!(r);
    assert_eq!(p.unwrap(), path!(%));

    let (r, p) = d(path!(moo.%), path!(moo.moo));
    assert!(r);
    assert_eq!(p.unwrap(), path!(%));

    let (r, p) = d(path!(moo.%), path!(cow));
    assert!(r);
    assert!(p.is_none());

    let (r, p) = d(path!(moo.cow.%), path!(moo));
    assert!(!r);
    assert_eq!(p.unwrap(), path!(cow.%));

    fn d(listener: Path, delegated: Path) -> (bool, Option<Path>) {
        let (retain, d_listener) = listener.delegate(&delegated);

        (retain, d_listener)
    }
}
