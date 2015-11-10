//! Represents a command sent by a client

use serde_json;
use serde_json::Value;

use path::Path;

#[derive(Debug, PartialEq)]
pub struct Command {
    pub call: Call,
    pub path: Path,
    pub params: Value
}

#[derive(Debug, PartialEq)]
pub enum Call {
    Bind,
    Read,
    Write
}

impl Command {
    pub fn from_json(json: &str) -> Result<Command, String> {
        let data: Value = try!(serde_json::from_str(json).or(Err("Bad JSON")));
        let data = try!(data.as_array().ok_or("Not array"));

        if data.len() != 3 {
            return Err("Wrong number of elements".to_string());
        }

        let call = try!(data[0].as_string().ok_or("Bad call"));
        let path = try!(data[1].as_array().ok_or("Bad path"));

        let mut path_string: Vec<String> = vec![];

        for p in path.iter() {
            path_string.push(try!(p.as_string().ok_or("Bad path")).to_string());
        }

        let params = data[2].clone();

        let call = match call {
            "bind" => Call::Bind,
            "read" => Call::Read,
            "write" => Call::Write,
            _ => return Err("Bad call".to_string())
        };

        Ok(Command {
            call: call,
            path: Path { path: path_string },
            params: params
        })
    }
}

#[test]
fn test_from_json() {
    let result = Command::from_json("[ 42, [], 42 ]");
    assert!(result.is_err());

    let result = Command::from_json(r#"[ "write", [], 42 ]"#).unwrap();
    assert_eq!(result.call, Call::Write);

    let result = Command::from_json(r#"[ "moo", [], 42 ]"#);
    assert!(result.is_err());

    let result = Command::from_json(r#"[ "bind", [ 42 ], 42 ]"#);
    assert!(result.is_err());

    let result = Command::from_json(r#"[ "bind", [ "moo" ], 42 ]"#).unwrap();
    assert_eq!(result.call, Call::Bind);
    assert_eq!(result.path, Path::new(vec!["moo".to_string()]));

    let result = Command::from_json(r#"[ "bind", [ "moo", 42 ], 42 ]"#);
    assert!(result.is_err());
}