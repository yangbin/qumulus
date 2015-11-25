Qumulus
=======

An experiment in Rust to auto-partition a single large tree of data.

Getting Started
---------------
```
cargo run

telnet localhost 8888

[ "write", ["root", "moo", "cow"], 42 ]
[ "write", ["root", "moo", "moo"], { "moo": { "cow": 42 } } ]
```

