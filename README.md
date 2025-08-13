<img src="https://raw.githubusercontent.com/eclipse-zenoh/zenoh/master/zenoh-dragon.png" height="150">

[![CI](https://github.com/eclipse-zenoh/zenoh/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/eclipse-zenoh/zenoh/actions?query=workflow%3ACI+branch%3Amain++)
[![Documentation Status](https://readthedocs.org/projects/zenoh-rust/badge/?version=latest)](https://zenoh-rust.readthedocs.io/en/latest/?badge=latest)
[![codecov](https://codecov.io/github/eclipse-zenoh/zenoh/branch/main/graph/badge.svg?token=F8T4C8WPZD)](https://codecov.io/github/eclipse-zenoh/zenoh)
[![Discussion](https://img.shields.io/badge/discussion-on%20github-blue)](https://github.com/eclipse-zenoh/roadmap/discussions)
[![Discord](https://img.shields.io/badge/chat-on%20discord-blue)](https://discord.gg/2GJ958VuHs)
[![License](https://img.shields.io/badge/License-EPL%202.0-blue)](https://choosealicense.com/licenses/epl-2.0/)
[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)

# Eclipse Zenoh

The Eclipse Zenoh: Zero Overhead Pub/sub, Store/Query and Compute.

Zenoh (pronounce _/zeno/_) unifies data in motion, data at rest and computations. It carefully blends traditional pub/sub with geo-distributed storages, queries and computations, while retaining a level of time and space efficiency that is well beyond any of the mainstream stacks.

Check the website [zenoh.io](http://zenoh.io) and the [roadmap](https://github.com/eclipse-zenoh/roadmap) for more detailed information.

-------------------------------

## Getting Started

Zenoh is extremely easy to learn, the best place to master the fundamentals is our [getting started guide](https://zenoh.io/docs/getting-started/first-app/).

-------------------------------

## How to install it

To install the latest release of the Zenoh router (`zenohd`) and its default plugins (REST API plugin and Storages Manager plugin) you can do as follows:

### Manual installation (all platforms)

All release packages can be downloaded from [https://download.eclipse.org/zenoh/zenoh/latest/](https://download.eclipse.org/zenoh/zenoh/latest/).

Each subdirectory has the name of the Rust target. See the platforms each target corresponds to on [https://doc.rust-lang.org/stable/rustc/platform-support.html](https://doc.rust-lang.org/stable/rustc/platform-support.html).

Choose your platform and download the `.zip` file.
Unzip it where you want, and run the extracted `zenohd` binary.

### Linux Debian

Add Eclipse Zenoh private repository to the sources list, and install the `zenoh` package:

```bash
curl -L https://download.eclipse.org/zenoh/debian-repo/zenoh-public-key | sudo gpg --dearmor --yes --output /etc/apt/keyrings/zenoh-public-key.gpg
echo "deb [trusted=yes] https://download.eclipse.org/zenoh/debian-repo/ /" | sudo tee -a /etc/apt/sources.list.d/zenoh.list > /dev/null
sudo apt update
sudo apt install zenoh
```

Then you can start run `zenohd`.

### MacOS

Tap our brew package repository and install the `zenoh` formula:

```bash
brew tap eclipse-zenoh/homebrew-zenoh
brew install zenoh
```

Then you can start run `zenohd`.

-------------------------------

## Rust API

* [Docs.rs for Zenoh](https://docs.rs/zenoh/latest/zenoh/)

-------------------------------

## How to build it

Install [Cargo and Rust](https://doc.rust-lang.org/cargo/getting-started/installation.html). Zenoh can be successfully compiled with Rust stable (>= 1.75.0), so no special configuration is required from your side. If you already have the Rust toolchain installed, make sure it is up-to-date with:

```bash
rustup update
```

To build Zenoh, just type the following command after having followed the previous instructions:

```bash
cargo build --release --all-targets
```

Building all targets may cause an out-of-memory error if memory is limited. Reducing the number of parallel build jobs can help, though it may increase build time. You can fine-tune the job count to balance performance and resource usage.

```bash
cargo build --release --all-targets --jobs=1
```

Zenoh's router is built as `target/release/zenohd`. All the examples are built into the `target/release/examples` directory. They can all work in peer-to-peer, or interconnected via the zenoh router.

-------------------------------

## Quick tests of your build

### Peer-to-peer tests

* **pub/sub**
  * run: `./target/release/examples/z_sub`
  * in another shell run: `./target/release/examples/z_put`
  * the subscriber should receive the publication.

* **get/queryable**
  * run: `./target/release/examples/z_queryable`
  * in another shell run: `./target/release/examples/z_get`
  * the queryable should display the log in its listener, and the get should receive the queryable result.

### Routed tests

> [!NOTE]
> **Windows users**: to properly execute the commands below in PowerShell you need to escape `"` characters as `\"`.

* **put / store / get**
  * run the Zenoh router with a memory storage:

    ```sh
    ./target/release/zenohd --cfg='plugins/storage_manager/storages/demo:{key_expr:"demo/example/**",volume:"memory"}'
    ```

  * in another shell run:
  
    ```sh
    ./target/release/examples/z_put`
    ```

  * then run

    ```sh
    ./target/release/examples/z_get
    ```

  * the get should receive the stored publication.

* **REST API using `curl` tool**
  * run the Zenoh router with a memory storage:

    ```sh
    ./target/release/zenohd --cfg='plugins/storage_manager/storages/demo:{key_expr:"demo/example/**",volume:"memory"}'
    ```

  * in another shell, do a publication via the REST API:

    ```sh
    curl -X PUT -d '"Hello World!"' http://localhost:8000/demo/example/test
    ```

  * get it back via the REST API:

    ```sh
    curl http://localhost:8000/demo/example/test
    ```

* **router admin space via the REST API**
  * run the Zenoh router with permission to perform config changes via the admin space, and with a memory storage:

    ```sh
    ./target/release/zenohd --rest-http-port=8000 --adminspace-permissions=rw --cfg='plugins/storage_manager/storages/demo:{key_expr:"demo/example/**",volume:"memory"}'
    ```

  * in another shell, get info of the zenoh router via the zenoh admin space (you may use `jq` for pretty json formatting):

    ```sh
    curl -s http://localhost:8000/@/local/router | jq
    ```

  * get the volumes of the router (only memory by default):

    ```sh
    curl -s 'http://localhost:8000/@/local/router/**/volumes/*' | jq
    ```

  * get the storages of the local router (the memory storage configured at startup on '/demo/example/**' should be present):

    ```sh
    curl -s 'http://localhost:8000/@/local/router/**/storages/*' | jq
    ```

  * add another memory storage on `/demo/mystore/**`:

    ```sh
    curl -X PUT -H 'content-type:application/json' -d '{"key_expr":"demo/mystore/**","volume":"memory"}' http://localhost:8000/@/local/router/config/plugins/storage_manager/storages/mystore
    ```

  * check it has been created:
  
    ```sh
    curl -s 'http://localhost:8000/@/local/router/**/storages/*' | jq
    ```

### Configuration options

A Zenoh configuration file can be provided via CLI to all Zenoh examples and the Zenoh router.

* `-c, --config <FILE>`: a [JSON5](https://json5.org) configuration file. [DEFAULT_CONFIG.json5](DEFAULT_CONFIG.json5) shows the schema of this file and the available options.

See other examples of Zenoh usage in [examples/](examples)

> [!NOTE]
> **Zenoh Runtime Configuration**: Starting from version 0.11.0-rc, Zenoh allows for configuring the number of worker threads and other advanced options of the runtime. For guidance on utilizing it, please refer to the [doc](https://docs.rs/zenoh-runtime/latest/zenoh_runtime/enum.ZRuntime.html).

-------------------------------

## Zenoh router command line arguments

`zenohd` accepts the following arguments:

* `--adminspace-permissions <[r|w|rw|none]>`: Configure the read and/or write permissions on the admin space. Default is read only.
* `-c, --config <FILE>`: a [JSON5](https://json5.org) configuration file. [DEFAULT_CONFIG.json5](DEFAULT_CONFIG.json5) shows the schema of this file. All properties of this configuration are optional, so you may not need such a large configuration for your use-case.
* `--cfg <KEY>:<VALUE>`: allows you to change specific parts of the configuration right after it has been constructed. VALUE must be a valid JSON5 value, and key must be a path through the configuration file, where each element is separated by a `/`. When inserting in parts of the config that are arrays, you may use indexes, or may use `+` to indicate that you want to append your value to the array. `--cfg` passed values will always override any previously existing value for their key in the configuration.
* `-l, --listen <ENDPOINT>...`: An endpoint on which this router will listen for incoming sessions.
  Repeat this option to open several listeners. By default, `tcp/[::]:7447` is used. The following endpoints are currently supported:
  * TCP: `tcp/<host_name_or_IPv4_or_IPv6>:<port>`
  * UDP: `udp/<host_name_or_IPv4_or_IPv6>:<port>`
  * [TCP+TLS](https://zenoh.io/docs/manual/tls/): `tls/<host_name>:<port>`
  * [QUIC](https://zenoh.io/docs/manual/quic/): `quic/<host_name>:<port>`
* `-e, --connect <ENDPOINT>...`: An endpoint this router will try to connect to. Repeat this option to connect to several peers or routers.
* `--no-multicast-scouting`: By default zenohd replies to multicast scouting messages for being discovered by peers and clients.
  This option disables this feature.
* `-i, --id <hex_string>`: The identifier (as an hexadecimal string - e.g.: A0B23...) that zenohd must use.
   **WARNING**: this identifier must be unique in the system! If not set, a random unsigned 128bit integer will be used.
* `--no-timestamp`: By default zenohd adds a HLC-generated Timestamp to each routed Data if there isn't already one.
  This option disables this feature.
* `-P, --plugin [<PLUGIN_NAME> | <PLUGIN_NAME>:<LIBRARY_PATH>]...`: A [plugin](https://zenoh.io/docs/manual/plugins/) that must be loaded. Accepted values:
  * a plugin name; zenohd will search for a library named `libzenoh_plugin_<name>.so` on Unix, `libzenoh_plugin_<PLUGIN_NAME>.dylib` on MacOS or `zenoh_plugin_<PLUGIN_NAME>.dll` on Windows.
  * `"<PLUGIN_NAME>:<LIBRARY_PATH>"`; the plugin will be loaded from library file at `<LIBRARY_PATH>`.

  Repeat this option to load several plugins.
* `--plugin-search-dir <DIRECTORY>...`: A directory where to search for [plugins](https://zenoh.io/docs/manual/plugins/) libraries to load.
  Repeat this option to specify several search directories'. By default, the plugins libraries will be searched in:
  `'/usr/local/lib:/usr/lib:~/.zenoh/lib:.'`
* `--rest-http-port <rest-http-port>`: Configures the [REST plugin](https://zenoh.io/docs/manual/plugin-http/)'s HTTP port. Accepted values:
  * a port number
  * a string with format `<local_ip>:<port_number>` (to bind the HTTP server to a specific interface)
  * `"None"` to deactivate the REST plugin

  If not specified, the REST plugin will be active on any interface (`[::]`) and port `8000`.

-------------------------------

## Plugins

> [!WARNING]
> As Rust doesn't have a stable ABI, the plugins should be
built with the exact same Rust version as `zenohd`, and using for `zenoh` dependency the same version (or commit number) as `zenohd` with the same
set of features. A plugin compiled with different Rust version or with different set of `zenoh` crate features will be rejected when `zenohd` attempts to load it. Otherwise, incompatibilities in memory mapping of structures shared between `zenohd` and the library could lead to a `"SIGSEGV"` crash.

By default the Zenoh router is delivered or built with 2 plugins. These may be configured through a configuration file, or through individual changes to the configuration via the `--cfg` CLI option or via zenoh puts on individual parts of the configuration.

**[REST plugin](https://zenoh.io/docs/manual/plugin-http/)** (exposing a REST API):
This plugin converts GET and PUT REST requests into Zenoh gets and puts respectively.

Note that to activate the REST plugin on `zenohd` the CLI argument should be passed: `--rest-http-port=8000` (or any other port of your choice).

**[Storages plugin](https://zenoh.io/docs/manual/plugin-storage-manager/)** (managing [backends and storages](https://zenoh.io/docs/manual/plugin-storage-manager/#backends-and-volumes))
This plugin allows you to easily define storages. These will store key-value pairs they subscribed to, and send the most recent ones when queried. Check out [DEFAULT_CONFIG.json5](DEFAULT_CONFIG.json5) for info on how to configure them.

-------------------------------

## Troubleshooting

In case of troubles, please first check on [this page](https://zenoh.io/docs/getting-started/troubleshooting/) if the trouble and cause are already known.
Otherwise, you can ask a question on the [zenoh Discord server](https://discord.gg/vSDSpqnbkm), or [create an issue](https://github.com/eclipse-zenoh/zenoh/issues).
