//! Represents a path to a subtree / node. Ordered so we can iterate through paths in a BTreeMap

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Path {
    pub path: Vec<String>
}

impl Path {
    pub fn new(path: Vec<String>) -> Path {
        Path { path: path }
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
