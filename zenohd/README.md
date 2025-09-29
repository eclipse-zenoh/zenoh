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

Check the website [zenoh.io](http://zenoh.io) for more information and [installation instructions](https://zenoh.io/docs/getting-started/installation/).

See also the [roadmap](https://github.com/eclipse-zenoh/roadmap) for more detailed technical information.

# Zenoh router

The `zenohd` is a daemon router which purpose is to build zenoh infrastructure. Technically the `zenohd` is the zenoh runtime with a plugin manager.

## Command Line Arguments

### Core Options

- **`-c, --config <PATH>`**  
  The configuration file. Currently, this file must be a valid JSON5 or YAML file.

  The commented example configuration file for `zenohd` is in the [documentation](https://docs.rs/zenoh/latest/zenoh/config/struct.Config.html)

- **`-l, --listen <ENDPOINT>`**  
  Locators on which this router will listen for incoming sessions. Repeat this option to open several listeners.

- **`-e, --connect <ENDPOINT>`**  
  A peer locator this router will try to connect to. Repeat this option to connect to several peers.

- **`-i, --id <ID>`**  
  The identifier (as an hexadecimal string, with odd number of chars - e.g.: A0B23...) that zenohd must use. If not set, a random unsigned 128bit integer will be used. WARNING: this identifier must be unique in the system and must be 16 bytes maximum (32 chars)!

### Plugin Management

- **`-P, --plugin <PLUGIN>`**  
  A plugin that MUST be loaded. You can give just the name of the plugin, zenohd will search for a library named `libzenoh_plugin_<name>.so` (exact name depending the OS). Or you can give such a string: `<plugin_name>:<library_path>`. Repeat this option to load several plugins. If loading failed, zenohd will exit.

- **`--plugin-search-dir <PATH>`**  
  Directory where to search for plugins libraries to load. Repeat this option to specify several search directories.

### Behavioral Options

- **`--no-timestamp`**  
  By default zenohd adds a HLC-generated Timestamp to each routed Data if there isn't already one. This option disables this feature.

- **`--no-multicast-scouting`**  
  By default zenohd replies to multicast scouting messages for being discovered by peers and clients. This option disables this feature.

### Network & API Configuration

- **`--rest-http-port <SOCKET>`**  
  Configures HTTP interface for the REST API (enabled by default on port 8000). Accepted values:
  - a port number
  - a string with format `<local_ip>:<port_number>` (to bind the HTTP server to a specific interface)  
  - `none` to disable the REST API

### Advanced Configuration

- **`--cfg <CFG>`**  
  Allows arbitrary configuration changes as column-separated KEY:VALUE pairs, where the empty key is used to represent the entire configuration:
  - KEY must be a valid config path, or empty string if the whole configuration is defined
  - VALUE must be a valid JSON5 string that can be deserialized to the expected type for the KEY field
  
  Examples:
  - `--cfg='startup/subscribe:["demo/**"]'`
  - `--cfg='plugins/storage_manager/storages/demo:{key_expr:"demo/example/**",volume:"memory"}'`
  - `--cfg=':{metadata:{name:"My App"},adminspace:{enabled:true,permissions:{read:true,write:true}}'`

- **`--adminspace-permissions <[r|w|rw|none]>`**  
  Configure the read and/or write permissions on the admin space. Default is read only.

### Help & Version

- **`-h, --help`**  
  Print help (see a summary with '-h').

- **`-V, --version`**  
  Print version.

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
