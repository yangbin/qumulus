Qumulus
=======

An experiment in Rust to auto-partition a single large tree of data.

Getting Started
---------------
```
cargo run

telnet localhost 8888

[ 1, "write", ["root", "moo", "cow"], 42 ]
[ 2, "write", [], { "moo": { "cow": 42 } } ]
[ 3, "read", ["moo", "cow"], {} ]
[ 4, "read", ["root", "moo", "moo"], {} ]
[ 5, "write", [], 42 ]
[ 6, "read", [], {} ]
```
