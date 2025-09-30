<img src="https://raw.githubusercontent.com/eclipse-zenoh/zenoh/master/zenoh-dragon.png" height="150">

# Supporting crates

This directory contains internal crates which `zenoh` depends on. These crates represent logically autonomous
components of `zenoh` and they are not intended to be used directly. Their public API is not stable and can
change at any time.

The only exception is the [zenoh-pinned-deps-1-75](zenoh-pinned-deps-1-75) crate which doesn't contain
any code. It contains external dependencies necessary for `zenoh` locked to their Rust 1.75 compatible
versions. To build `zenoh` with Rust 1.75, this dependency should be added to the user's `Cargo.toml`

```toml
zenoh = "1.5.1"
zenoh-pinned-deps-1-75 = "1.5.1"
```

```sh
cargo +1.75 build
```
