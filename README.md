Qumulus
=======

An experiment in Rust to auto-partition a single large tree of data.

Getting Started
---------------
```
cargo run

telnet localhost 8888

[ 1, "write", ["moo", "cow"], 42 ]
[ 2, "read", ["moo", "cow"], {} ]
[ 3, "write", [], { "moo": { "cow": 42 } } ]
[ 4, "read", ["moo", "cow"], {} ]
[ 5, "read", ["moo", "moo"], {} ]
[ 6, "bind", ["moo", "cow"], {} ]
[ 7, "write", ["moo", "cow"], "moo" ]
[ 8, "kill", ["moo", "cow"], null ]
```
