//! Replica handling.

use std::net::{AddrParseError,SocketAddr};
use std::str::FromStr;

/// Represents a Replica.
///
/// Replicas are identified by an IP/port combination
#[derive(Clone, Debug, PartialEq)]
pub struct Replica {
    addr: SocketAddr
}

impl Replica {
    pub fn addr(&self) -> &SocketAddr {
        &self.addr
    }
}

impl FromStr for Replica {
    type Err = AddrParseError;
    fn from_str(s: &str) -> Result<Replica, AddrParseError> {
        s.parse().map(|addr| Replica {
            addr: addr
        })
    }
}

#[test]
fn test_replica_parse() {
    let replica: Replica = "127.0.0.1:1000".parse().unwrap();

    assert_eq!(replica.addr, "127.0.0.1:1000".parse().unwrap());
}
