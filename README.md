<img src="https://raw.githubusercontent.com/eclipse-zenoh/zenoh/master/zenoh-dragon.png" height="150">

[![CI](https://github.com/eclipse-zenoh/zenoh/workflows/CI/badge.svg)](https://github.com/eclipse-zenoh/zenoh/actions?query=workflow%3A%22CI%22)
[![Documentation Status](https://readthedocs.org/projects/zenoh-rust/badge/?version=latest)](https://zenoh-rust.readthedocs.io/en/latest/?badge=latest)
[![Discussion](https://img.shields.io/badge/discussion-on%20github-blue)](https://github.com/eclipse-zenoh/roadmap/discussions)
[![Discord](https://img.shields.io/badge/chat-on%20discord-blue)](https://discord.gg/2GJ958VuHs)
[![License](https://img.shields.io/badge/License-EPL%202.0-blue)](https://choosealicense.com/licenses/epl-2.0/)
[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)

# Eclipse zenoh
The Eclipse zenoh: Zero Overhead Pub/sub, Store/Query and Compute.

Eclipse zenoh (pronounce _/zeno/_) unifies data in motion, data in-use, data at rest and computations. It carefully blends traditional pub/sub with geo-distributed storages, queries and computations, while retaining a level of time and space efficiency that is well beyond any of the mainstream stacks.

Check the website [zenoh.io](http://zenoh.io) and the [roadmap](https://github.com/eclipse-zenoh/roadmap) for more detailed information.

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
## Previous 0.5 API:
The following documentation pertains to the v0.6 API, which comes many changes to the behaviour and configuration of Zenoh. 

To access the v0.5 version of the code and matching README, please go to the [0.5.0-beta.9](https://github.com/eclipse-zenoh/zenoh/tree/0.5.0-beta.9) tagged version.

-------------------------------
## Quick tests of your build:

**Peer-to-peer tests:**

 - **pub/sub**
    - run: `./target/release/examples/z_sub`
    - in another shell run: `./target/release/examples/z_put`
    - the subscriber should receive the publication.

 - **get/queryable**
    - run: `./target/release/examples/z_queryable`
    - in another shell run: `./target/release/examples/z_get`
    - the queryable should display the log in its listener, and the get should receive the queryable result.

**Routed tests:**

 - **put/store/get**
    - run the zenoh router with a memory storage:  
      `./target/release/zenohd --cfg='plugins/storage_manager/storages/demo:{key_expr:"demo/example/**",volume:"memory"}'`
    - in another shell run: `./target/release/examples/z_put`
    - then run `./target/release/examples/z_get`
    - the get should receive the stored publication.

 - **REST API using `curl` tool**
    - run the zenoh router with a memory storage:  
      `./target/release/zenohd --cfg='plugins/storage_manager/storages/demo:{key_expr:"demo/example/**",volume:"memory"}'`
    - in another shell, do a publication via the REST API:  
      `curl -X PUT -d 'Hello World!' http://localhost:8000/demo/example/test`
    - get it back via the REST API:  
      `curl http://localhost:8000/demo/example/test`

  - **router admin space via the REST API**
    - run the zenoh router with permission to perform config changes via the admin space, and with a memory storage:  
      `./target/release/zenohd --adminspace-permissions=rw --cfg='plugins/storage_manager/storages/demo:{key_expr:"demo/example/**",volume:"memory"}'`
    - in another shell, get info of the zenoh router via the zenoh admin space:  
      `curl http://localhost:8000/@/router/local`
    - get the volumes of the router (only memory by default):  
      `curl 'http://localhost:8000/@/router/local/**/volumes/*'`
    - get the storages of the local router (the memory storage configured at startup on '/demo/example/**' should be present):  
     `curl 'http://localhost:8000/@/router/local/**/storages/*'`
    - add another memory storage on `/demo/mystore/**`:  
      `curl -X PUT -H 'content-type:application/json' -d '{"key_expr":"demo/mystore/**","volume":"memory"}' http://localhost:8000/@/router/local/config/plugins/storage_manager/storages/mystore`
    - check it has been created:  
      `curl 'http://localhost:8000/@/router/local/**/storages/*'`


See other examples of zenoh usage in [examples/](examples)

-------------------------------
## zenoh router command line arguments
`zenohd` accepts the following arguments:

  * `--adminspace-permissions <[r|w|rw|none]>`: Configure the read and/or write permissions on the admin space. Default is read only.
  * `-c, --config <FILE>`: a [JSON5](https://json5.org) configuration file. [DEFAULT_CONFIG.json5](DEFAULT_CONFIG.json5) shows the schema of this file. All properties of this configuration are optional, so you may not need such a large configuration for your use-case.
  * `--cfg <KEY>:<VALUE>` : allows you to change specific parts of the configuration right after it has been constructed. VALUE must be a valid JSON5 value, and key must be a path through the configuration file, where each element is separated by a `/`. When inserting in parts of the config that are arrays, you may use indexes, or may use `+` to indicate that you want to append your value to the array. `--cfg` passed values will always override any previously existing value for their key in the configuration.
  * `-l, --listen <ENDPOINT>...`: An endpoint on which this router will listen for incoming sessions. 
    Repeat this option to open several listeners. By default, `tcp/[::]:7447` is used. The following endpoints are currently supported:
      - TCP: `tcp/<host_name_or_IPv4_or_IPv6>:<port>`
      - UDP: `udp/<host_name_or_IPv4_or_IPv6>:<port>`
      - [TCP+TLS](https://zenoh.io/docs/manual/tls/): `tls/<host_name>:<port>`
      - [QUIC](https://zenoh.io/docs/manual/quic/): `quic/<host_name>:<port>`
  * `-e, --connect <ENDPOINT>...`: An endpoint this router will try to connect to. Repeat this option to connect to several peers or routers.
  * `--no-multicast-scouting`: By default zenohd replies to multicast scouting messages for being discovered by peers and clients.
    This option disables this feature.
  * `-i, --id <hex_string>`: The identifier (as an hexadecimal string - e.g.: 0A0B23...) that zenohd must use.
     **WARNING**: this identifier must be unique in the system! If not set, a random UUIDv4 will be used.
  * `--no-timestamp`: By default zenohd adds a HLC-generated Timestamp to each routed Data if there isn't already one.
    This option disables this feature.
  * `-P, --plugin [<PLUGIN_NAME> | <PLUGIN_NAME>:<LIBRARY_PATH>]...`: A [plugin](https://zenoh.io/docs/manual/plugins/) that must be loaded. Accepted values:
     - a plugin name; zenohd will search for a library named `libzplugin_<name>.so` on Unix, `libzplugin_<PLUGIN_NAME>.dylib` on MacOS or `zplugin_<PLUGIN_NAME>.dll` on Windows.
     - `"<PLUGIN_NAME>:<LIBRARY_PATH>"`; the plugin will be loaded from library file at `<LIBRARY_PATH>`.

    Repeat this option to load several plugins.
  * `--plugin-search-dir <DIRECTORY>...`: A directory where to search for [plugins](https://zenoh.io/docs/manual/plugins/) libraries to load.
    Repeat this option to specify several search directories'. By default, the plugins libraries will be searched in:
    `'/usr/local/lib:/usr/lib:~/.zenoh/lib:.'`
  * `--rest-http-port <rest-http-port>`: Configures the [REST plugin](https://zenoh.io/docs/manual/plugin-http/)'s HTTP port. Accepted values:
      - a port number
      - a string with format `<local_ip>:<port_number>` (to bind the HTTP server to a specific interface)
      - `"None"` to desactivate the REST plugin

    If not specified, the REST plugin will be active on any interface (`[::]`) and port `8000`.

-------------------------------
## Plugins
By default the zenoh router is delivered or built with 2 plugins. These may be configured through a configuration file, or through individual changes to the configuration via the `--cfg` CLI option or via zenoh puts on individual parts of the configuration.

WARNING: since `v0.6`, `zenohd` no longer loads every available plugin at startup. Instead, only configured plugins are loaded (after processing `--cfg` and `--plugin` options). Once `zenohd` is running, plugins can be hot-loaded and, if they support it, reconfigured at runtime by editing their configuration through the adminspace.  

Note that the REST plugin is added to the configuration by the default value of the `--rest-http-port` CLI argument.

**[REST plugin](https://zenoh.io/docs/manual/plugin-http/)** (exposing a REST API):
This plugin converts GET and PUT REST requests into Zenoh gets and puts respectively.

**[Storages plugin](https://zenoh.io/docs/manual/plugin-storages/)** (managing [backends and storages](https://zenoh.io/docs/manual/backends/))
This plugin allows you to easily define storages. These will store key-value pairs they subscribed to, and send the most recent ones when queried. Check out [DEFAULT_CONFIG.json5](DEFAULT_CONFIG.json5) for info on how to configure them.

-------------------------------
## Troubleshooting

In case of troubles, please first check on [this page](https://zenoh.io/docs/getting-started/troubleshooting/) if the trouble and cause are already known.  
Otherwise, you can ask a question on the [zenoh Discord server](https://discord.gg/vSDSpqnbkm), or [create an issue](https://github.com/eclipse-zenoh/zenoh/issues).
