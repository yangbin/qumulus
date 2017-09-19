Qumulus
=======
![Build Status](https://travis-ci.org/yangbin/qumulus.svg?branch=master)

An experiment in Rust to auto-partition a single large tree of data.

Getting Started
---------------
1. Ensure that you are using rustc version `1.20.0` and above.
```
curl https://sh.rustup.rs -sSf | sh
```

To build it from source, follow instructions on the rust-lang repository (https://github.com/rust-lang/rust#quick-start).

2. Clone qumulus.
```
git clone git@github.com:yangbin/qumulus.git && cd qumulus
```

3. Run.
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
[ 8, "read", ["*"], null ]
[ 9, "read", ["**"], null ]
[ 10, "kill", ["moo", "cow"], null ]
```
