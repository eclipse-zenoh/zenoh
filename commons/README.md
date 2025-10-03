<img src="https://raw.githubusercontent.com/eclipse-zenoh/zenoh/master/zenoh-dragon.png" height="150">

# Supporting crates

This directory contains internal crates which `zenoh` depends on. These crates represent logically autonomous
components of `zenoh` and they are not intended to be used directly. Their public API is not stable and can
change at any time.

The only exception is the [zenoh-pinned-deps-1-75](zenoh-pinned-deps-1-75) which is supposed to be used by
projects which need to support Rust 1.75.
