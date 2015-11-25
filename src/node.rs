use std::collections::BTreeMap;

use serde_json::Value;

#[derive(Debug, PartialEq)]
pub struct Node {
    created: u64,
    updated: u64,
    deleted: u64,
    value:  Value,
    keys: Option<BTreeMap<String, Node>>
}

impl Default for Node {
    fn default() -> Node {
        Node {
            created: 0,
            updated: 0,
            deleted: 1,
            value: Value::Null,
            keys: None
        }
    }
}

impl Node {
    pub fn expand(data: &Value, timestamp: u64) -> Node {
        if let Value::Array(ref arr) = *data {
            let keys = arr.iter().enumerate().map(|(k, v)|
                (k.to_string(), Node::expand(&v, timestamp))
            ).collect();

            Node {
                created: timestamp,
                keys: Some(keys),
                ..Default::default()
            }
        }
        else if let Value::Object(ref obj) = *data {
            let keys = obj.iter().map(|(k, v)|
                (k.clone(), Node::expand(&v, timestamp))
            ).collect();

            Node {
                created: timestamp,
                keys: Some(keys),
                ..Default::default()
            }
        }
        else {
            Node {
                created: timestamp,
                updated: timestamp,
                value: data.clone(),
                ..Default::default()
            }
        }
    }
}

macro_rules! map(
    { $($key:expr => $value:expr),+ } => {
        {
            let mut m = BTreeMap::new();
            $(
                m.insert($key, $value);
            )+
            m
        }
    };
);

#[cfg(test)]
use serde_json;

#[test]
fn test_expand() {
    let data: Value = serde_json::from_str(r#"
        {
            "moo": 42
        }
    "#).unwrap();

    let node = Node::expand(&data, 1000);

    let expected = Node {
        created: 1000,
        updated: 0,
        deleted: 1,
        value:  Value::Null,
        keys: Some(map! {
            "moo".to_string() => Node {
                created: 1000,
                updated: 1000,
                deleted: 1,
                value: serde_json::to_value(&42),
                keys: None
            }
        })
    };

    assert_eq!(node, expected);
}
