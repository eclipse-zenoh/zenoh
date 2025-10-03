<img src="https://raw.githubusercontent.com/eclipse-zenoh/zenoh/master/zenoh-dragon.png" height="150">

[![CI](https://github.com/eclipse-zenoh/zenoh/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/eclipse-zenoh/zenoh/actions?query=workflow%3ACI+branch%3Amain++)
[![Documentation Status](https://readthedocs.org/projects/zenoh-rust/badge/?version=latest)](https://zenoh-rust.readthedocs.io/en/latest/?badge=latest)
[![codecov](https://codecov.io/github/eclipse-zenoh/zenoh/branch/main/graph/badge.svg?token=F8T4C8WPZD)](https://codecov.io/github/eclipse-zenoh/zenoh)
[![Discussion](https://img.shields.io/badge/discussion-on%20github-blue)](https://github.com/eclipse-zenoh/roadmap/discussions)
[![Discord](https://img.shields.io/badge/chat-on%20discord-blue)](https://discord.gg/2GJ958VuHs)
[![License](https://img.shields.io/badge/License-EPL%202.0-blue)](https://choosealicense.com/licenses/epl-2.0/)
[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)

# Eclipse Zenoh

Eclipse Zenoh: Zero Overhead Pub/Sub, Store/Query and Compute.

Zenoh (pronounce _/zeno/_) unifies data in motion, data at rest and computations. It carefully blends traditional pub/sub with geo-distributed storages, queries and computations, while retaining a level of time and space efficiency that is well beyond any of the mainstream stacks.

Check the website [zenoh.io](http://zenoh.io) for more information and [installation instructions](https://zenoh.io/docs/getting-started/installation/).

See also the [roadmap](https://github.com/eclipse-zenoh/roadmap) for more detailed technical information.

# Zenoh router

`zenohd` is a daemon router whose purpose is to build Zenoh infrastructure. Technically, `zenohd` is the Zenoh runtime with a plugin manager.

## Command Line Arguments

### Core Options

- **`-c, --config <PATH>`**  
  The configuration file. Currently, this file must be a valid JSON5 or YAML file.

  The commented example configuration file for `zenohd` is in the [documentation](https://docs.rs/zenoh/latest/zenoh/config/struct.Config.html).

- **`-l, --listen <ENDPOINT>`**  
  Locators on which this router will listen for incoming sessions. Repeat this option to open several listeners.

- **`-e, --connect <ENDPOINT>`**  
  A peer locator this router will try to connect to. Repeat this option to connect to several peers.

- **`-i, --id <ID>`**  
  The identifier (as a hexadecimal string, with an odd number of chars - e.g.: A0B23...) that zenohd must use. If not set, a random unsigned 128-bit integer will be used. WARNING: this identifier must be unique in the system and must be 16 bytes maximum (32 chars)!

### Plugin Management

- **`-P, --plugin <PLUGIN>`**  
  A plugin that MUST be loaded. You can give just the name of the plugin; zenohd will search for a library named `libzenoh_plugin_<name>.so` (exact name depending on the OS). Alternatively, you can provide a string in this format: `<plugin_name>:<library_path>`. Repeat this option to load several plugins. If loading fails, zenohd will exit.

- **`--plugin-search-dir <PATH>`**  
  Directory in which to search for plugin libraries to load. Repeat this option to specify several search directories.

- **`--rest-http-port <SOCKET>`**  
  Enables REST API plugin and configures HTTP interface for it. Accepted values:
  - a port number
  - a string with format `<local_ip>:<port_number>` (to bind the HTTP server to a specific interface)  

### Behavioral Options

- **`--no-timestamp`**  
  By default, zenohd adds a HLC-generated Timestamp to each routed Data if there isn't already one. This option disables this feature.

- **`--no-multicast-scouting`**  
  By default, zenohd replies to multicast scouting messages to be discovered by peers and clients. This option disables this feature.

### Advanced Configuration

- **`--cfg <CFG>`**  
  Allows arbitrary configuration changes as colon-separated KEY:VALUE pairs, where the empty key is used to represent the entire configuration:
  - KEY must be a valid config path, or an empty string if the whole configuration is defined
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
> As Rust doesn't have a stable ABI, plugins should be built with the exact same Rust version as `zenohd`, and should use the same version (or commit number) of the `zenoh` dependency as `zenohd` with the same set of features. A plugin compiled with a different Rust version or with a different set of `zenoh` crate features will be rejected when `zenohd` attempts to load it. Otherwise, incompatibilities in memory mapping of structures shared between `zenohd` and the library could lead to a `"SIGSEGV"` crash.

The Zenoh router is delivered with two statically linked plugins:

- [zenoh-plugin-rest](../plugins/zenoh-plugin-rest) is the plugin exposing a [REST API](https://zenoh.io/docs/apis/rest/)

- [zenoh-plugin-storage-manager](../plugins/zenoh-plugin-storage-manager) is the plugin that manages
  [backends and storages](https://zenoh.io/docs/manual/plugin-storage-manager/#backends-and-volumes)

There are other plugins in independent repositories:

- [zenoh-plugin-dds](https://github.com/eclipse-zenoh/zenoh-plugin-dds/)

  Bridges [DDS](https://www.dds-foundation.org/) with Zenoh.

- [zenoh-plugin-ros2dds](https://github.com/eclipse-zenoh/zenoh-plugin-ros2dds/)

  Bridges all [ROS 2](https://ros.org/) communications using [DDS](https://www.dds-foundation.org/) over Zenoh.

- [zenoh-plugin-webserver](https://github.com/eclipse-zenoh/zenoh-plugin-webserver/)

  Implements an HTTP server mapping URLs to zenoh keys.

- [zenoh-plugin-remote-api](https://github.com/eclipse-zenoh/zenoh-ts/)

  The WebSocket server supporting TypeScript API
