//! Represents a command sent by a client

use serde_json;
use serde_json::Value;
use time;

use path::Path;

#[derive(Clone, Debug, PartialEq)]
pub struct Command {
    pub id: u64,
    pub call: Call,
    pub path: Path,
    pub params: Value,
    pub timestamp: u64
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Call {
    Bind,
    Kill,
    Read,
    Write
}

impl Command {
    pub fn from_json(json: &str) -> Result<Command, String> {
        let data: Value = try!(serde_json::from_str(json).or(Err("Bad JSON")));
        let data = try!(data.as_array().ok_or("Not array"));

        if data.len() != 4 {
            return Err("Wrong number of elements".to_string());
        }

        let id   = try!(data[0].as_u64().ok_or("Bad ID"));
        let call = try!(data[1].as_str().ok_or("Bad call"));
        let path = try!(data[2].as_array().ok_or("Bad path"));

        let mut path_string: Vec<String> = vec![];

        for p in path.iter() {
            path_string.push(try!(p.as_str().ok_or("Bad path")).to_string());
        }

        let params = data[3].clone();

        let call = match call {
            "bind" => Call::Bind,
            "kill" => Call::Kill,
            "read" => Call::Read,
            "write" => Call::Write,
            _ => return Err("Bad call".to_string())
        };

        Ok(Command {
            id: id,
            call: call,
            path: Path { path: path_string },
            params: params,
            timestamp: time::precise_time_ns()
        })
    }

    /// Returns true if delegated data requires separate calls.
    ///
    /// Right now, only `Call::Bind` and Call::Read` fall into this category
    pub fn recursive(&self) -> bool {
        match self.call {
            Call::Bind | Call::Read => true,
            _ => false
        }
    }
}

#[test]
fn test_from_json() {
    let result = Command::from_json("[ 42, [], 42 ]");
    assert!(result.is_err());

    let result = Command::from_json(r#"[ 1, "write", [], 42 ]"#).unwrap();
    assert_eq!(result.call, Call::Write);

    let result = Command::from_json(r#"[ 1, "moo", [], 42 ]"#);
    assert!(result.is_err());

    let result = Command::from_json(r#"[ 1, "bind", [ 42 ], 42 ]"#);
    assert!(result.is_err());

    let result = Command::from_json(r#"[ 1, "bind", [ "moo" ], 42 ]"#).unwrap();
    assert_eq!(result.call, Call::Bind);
    assert_eq!(result.path, Path::new(vec!["moo".to_string()]));

    let result = Command::from_json(r#"[ 1, "bind", [ "moo", 42 ], 42 ]"#);
    assert!(result.is_err());
}
