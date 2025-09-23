Build zenoh with proper config:
```
RUSTFLAGS="--cfg tokio_unstable" cargo build --release --examples
```

Run z_pong:
```
target/release/examples/z_pong
```

Run z_ping:
```
TOKIO_CONSOLE_BIND=127.0.0.1:7777 target/release/examples/z_ping 8 -n 100000000
```


Run tokio-console:
```
tokio-console
```

in console:

io/zenoh-transport/src/unicast/universal/link.rs:168:22 - RX task

io/zenoh-transport/src/unicast/universal/link.rs:122:22 - TX task