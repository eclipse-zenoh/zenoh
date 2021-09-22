<img src="http://zenoh.io/img/zenoh-dragon-small.png" height="150">

[![CI](https://github.com/eclipse-zenoh/zenoh/workflows/CI/badge.svg)](https://github.com/eclipse-zenoh/zenoh/actions?query=workflow%3A%22CI%22)
[![Documentation Status](https://readthedocs.org/projects/zenoh-rust/badge/?version=latest)](https://zenoh-rust.readthedocs.io/en/latest/?badge=latest)
[![Gitter](https://badges.gitter.im/atolab/zenoh.svg)](https://gitter.im/atolab/zenoh?utm_source=badge&utm_medium=badge&utm_campaign=pr-badge)
[![License](https://img.shields.io/badge/License-EPL%202.0-blue)](https://choosealicense.com/licenses/epl-2.0/)
[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)

# Eclipse zenoh
The Eclipse zenoh: Zero Overhead Pub/sub, Store/Query and Compute.

Eclipse zenoh (pronounce _/zeno/_) unifies data in motion, data in-use, data at rest and computations. It carefully blends traditional pub/sub with geo-distributed storages, queries and computations, while retaining a level of time and space efficiency that is well beyond any of the mainstream stacks.

Check the website [zenoh.io](http://zenoh.io) for more detailed information.

-------------------------------
## How to install and test it

See our _"Getting started"_ tour starting with the [zenoh key concepts](https://zenoh.io/docs/getting-started/key-concepts/).

-------------------------------
## How to build it

Install [Cargo and Rust](https://doc.rust-lang.org/cargo/getting-started/installation.html). Zenoh can be succesfully compiled with Rust stable (>= 1.5.1), so no special configuration is required from your side.  
To build zenoh, just type the following command after having followed the previous instructions:

```bash
$ cargo build --release --all-targets
```

The zenoh router is built as `target/release/zenohd`. All the examples are built into the `target/release/examples` directory. They can all work in peer-to-peer, or interconnected via the zenoh router.

-------------------------------
## Quick tests of your build:

**Peer-to-peer tests:**

 - **pub/sub**
    - run: `./target/release/examples/z_sub`
    - in another shell run: `./target/release/examples/z_put`
    - the subscriber should receive the publication.

 - **get/eval**
    - run: `./target/release/examples/z_eval`
    - in another shell run: `./target/release/examples/z_get`
    - the eval should display the log in it's listener, and the get should receive the eval result.

**Routed tests:**

 - **put/store/get**
    - run the zenoh router with a memory storage:  
      `./target/release/zenohd --mem-storage '/demo/example/**'`
    - in another shell run: `./target/release/examples/z_put`
    - then run `./target/release/examples/z_get`
    - the get should receive the stored publication.

 - **REST API using `curl` tool**
    - run the zenoh router with a memory storage:  
      `./target/release/zenohd --mem-storage '/demo/example/**'`
    - in another shell, do a publication via the REST API:  
      `curl -X PUT -d 'Hello World!' http://localhost:8000/demo/example/test`
    - get it back via the REST API:  
      `curl http://localhost:8000/demo/example/test`

  - **router admin space via the REST API**
    - run the zenoh router with a memory storage:  
      `./target/release/zenohd --mem-storage '/demo/example/**'`
    - in another shell, get info of the zenoh router via the zenoh admin space:  
      `curl http://localhost:8000/@/router/local`
    - get the backends of the router (only memory by default):  
      `curl 'http://localhost:8000/@/router/local/**/backend/*'`
    - get the storages of the local router (the memory storage configured at startup on '/demo/example/**' should be present):  
     `curl 'http://localhost:8000/@/router/local/**/storage/*'`
    - add another memory storage on `/demo/mystore/**`:  
      `curl -X PUT -H 'content-type:application/properties' -d 'path_expr=/demo/mystore/**' http://localhost:8000/@/router/local/plugin/storages/backend/memory/storage/my-storage`
    - check it has been created:  
      `curl 'http://localhost:8000/@/router/local/**/storage/*'`


See other examples of zenoh usage:
 - with the zenoh API in [zenoh/examples/zenoh](https://github.com/eclipse-zenoh/zenoh/tree/master/zenoh/examples/zenoh)
 - with the zenoh-net API in [zenoh/examples/zenoh-net](https://github.com/eclipse-zenoh/zenoh/tree/master/zenoh/examples/zenoh-net)

-------------------------------
## zenoh router command line arguments
`zenohd` accepts the following arguments:

  * `-c, --config <FILE>`: a configuration file containing a list of properties with format `<key>=<value>` (1 per-line).
    The accepted property keys are the same than accepted by the zenoh API and are documented [here](https://docs.rs/zenoh/0.5.0-beta.8/zenoh/net/config/index.html).
  * `-l, --listener <LOCATOR>...`: A locator on which this router will listen for incoming sessions. 
    Repeat this option to open several listeners. By default `tcp/0.0.0.0:7447` is used. The following locators are currently supported:
      - TCP: `tcp/<host_name_or_IPv4>:<port>`
      - UDP: `udp/<host_name_or_IPv4>:<port>`
      - [TCP+TLS](https://zenoh.io/docs/manual/tls/): `tls/<host_name_or_IPv4>:<port>`
      - [QUIC](https://zenoh.io/docs/manual/quic/): `quic/<host_name_or_IPv4>:<port>`
  * `-e, --peer <LOCATOR>...`: A peer locator this router will try to connect to. Repeat this option to connect to several peers.
  * `--no-multicast-scouting`: By default zenohd replies to multicast scouting messages for being discovered by peers and clients.
    This option disables this feature.
  * `-i, --id <hex_string>`: The identifier (as an hexadecimal string - e.g.: 0A0B23...) that zenohd must use.
     **WARNING**: this identifier must be unique in the system! If not set, a random UUIDv4 will be used.
  * `--no-timestamp`: By default zenohd adds a HLC-generated Timestamp to each routed Data if there isn't already one.
    This option disables this feature.
  * `--plugin-nolookup`: When set, zenohd will not look for plugins nor try to load any [plugin](https://zenoh.io/docs/manual/plugins/)
    except the ones explicitely configured with -P or --plugin.
  * `-P, --plugin <PATH_TO_PLUGIN_LIB>...`: A [plugin](https://zenoh.io/docs/manual/plugins/) that must be loaded.
    Repeat this option to load several plugins.
  * `--plugin-search-dir <DIRECTORY>...`:  A directory where to search for [plugins](https://zenoh.io/docs/manual/plugins/) libraries to load.
    Repeat this option to specify several search directories'. By default, the plugins libraries will be searched in:
    `'/usr/local/lib:/usr/lib:~/.zenoh/lib:.'`

By default the zenoh router is delivered or built with 2 plugins that will be loaded at start-up. Each accepts some extra command line arguments:

**[REST plugin](https://zenoh.io/docs/manual/plugin-http/)** (exposing a REST API):
  * `--rest-http-port <rest-http-port>`: The REST plugin's http port [default: 8000]

**[Storages plugin](https://zenoh.io/docs/manual/plugin-storages/)** (managing [backends and storages](https://zenoh.io/docs/manual/backends/))

  * `--no-backend`: If true, no backend (and thus no storage) are created at startup. If false (default) the Memory backend it present at startup.
  * `--mem-storage <PATH_EXPR>...`: A memory storage to be created at start-up. Repeat this option to created several storages
  * `--backend-search-dir <DIRECTORY>...`: A directory where to search for backends libraries to load.
    Repeat this option to specify several search directories'. By default, the backends libraries will be searched in:
    `'/usr/local/lib:/usr/lib:~/.zenoh/lib:.'`

-------------------------------
## Troubleshooting

In case of troubles, please first check on [this page](https://zenoh.io/docs/getting-started/troubleshooting/) if the trouble and cause are already known.  
Otherwise, you can ask a question on the [zenoh Gitter channel](https://gitter.im/atolab/zenoh), or [create an issue](https://github.com/eclipse-zenoh/zenoh/issues).
