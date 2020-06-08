![zenoh banner](./zenoh-dragon.png)

![CI](https://github.com/eclipse-zenoh/zenoh/workflows/CI/badge.svg)
[![Gitter](https://badges.gitter.im/atolab/zenoh.svg)](https://gitter.im/atolab/zenoh?utm_source=badge&utm_medium=badge&utm_campaign=pr-badge)
[![License](https://img.shields.io/badge/License-EPL%202.0-blue)](https://choosealicense.com/licenses/epl-2.0/)
[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)

# Eclipse zenoh
The Eclipse zenoh: Zero Overhead Pub/sub, Store/Query and Compute.

Eclipse zenoh unifies data in motion, data in-use, data at rest and computations. It carefully blends traditional pub/sub with geo-distributed storages, queries and computations, while retaining a level of time and space efficiency that is well beyond any of the mainstream stacks.

Check the website [zenoh.io](http://zenoh.io) for more detailed information.

-------------------------------
## How to build it

Install [Cargo and Rust](https://doc.rust-lang.org/cargo/getting-started/installation.html) and then build zenoh with:
```bash
$ cargo build --release --all-targets
```

-------------------------------
## How to test it

All the examples are compiled into the `target/release/examples` directory. They can all work in peer-to-peer, or interconnected via the zenoh router (`target/release/zenohd`).

Example of usage with the throughput performance test:

 * In peer-to-peer mode:
     
     * in a first shell start the subscriber:  
       ```bash
       ./target/release/examples/zn_sub_thr
       ```
     * in a second shell start the publisher, making it to publish 1024 bytes payloads:  
       ```bash
       ./target/release/examples/zn_pub_thr 1024
       ```

 * In routed mode:

     * in a first shell start the zenoh router:  
       ```bash
       ./target/release/zenohd
       ```
     * in a second shell start the subscriber:  
       ```bash
       ./target/release/examples/zn_sub_thr
       ```
     * in a third shell start the publisher, making it to publish 1024 bytes payloads:  
       ```bash
       ./target/release/examples/zn_pub_thr 1024
       ```
