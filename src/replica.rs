//! Replica handling.

use std::fmt;
use std::net::{AddrParseError,SocketAddr};
use std::str::FromStr;

/// Represents a Replica.
///
/// Replicas are identified by an IP/port combination
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Replica {
    addr: SocketAddr
}

impl Replica {
    pub fn api_addr(&self) -> SocketAddr {
        let addr = self.addr.clone();

        addr
    }

    pub fn peer_addr(&self) -> SocketAddr {
        let mut addr = self.addr.clone();
        let port = addr.port() + 100;

        addr.set_port(port);

        addr
    }
}

impl fmt::Display for Replica {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.addr)
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
