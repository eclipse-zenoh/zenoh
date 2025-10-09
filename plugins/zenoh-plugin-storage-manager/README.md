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

Zenoh (pronounced _/zeno/_) unifies data in motion, data at rest, and computations. It carefully blends traditional pub/sub with geo-distributed storages, queries, and computations, while retaining a level of time and space efficiency that is well beyond any of the mainstream stacks.

Check the website [zenoh.io](http://zenoh.io) for more information and [installation instructions](https://zenoh.io/docs/getting-started/installation/).

See also the [roadmap](https://github.com/eclipse-zenoh/roadmap) for more detailed technical information.

# Storage manager plugin for `zenohd` router

A plugin that allows connecting `zenohd` to different storages (e.g., databases). This plugin is a plugin manager
itself that loads its own plugins - `backends` - specific for the external storage API.

## Backends available

- `memory` backend

   Stores data in a hashmap in memory, statically linked to the storage manager.

- [zenoh-backend-filesystem](https://github.com/eclipse-zenoh/zenoh-backend-filesystem/)

   This backend relies on the host's file system to implement the storages.

- [zenoh-backend-s3](https://github.com/eclipse-zenoh/zenoh-backend-s3/)

  This backend relies on [Amazon S3](https://aws.amazon.com/s3/?nc1=h_ls) to implement the storages. It is also compatible with [MinIO](https://min.io/) object storage.

- [zenoh-backend-rocksdb](https://github.com/eclipse-zenoh/zenoh-backend-rocksdb/)

  This backend relies on [RocksDB](https://rocksdb.org/) to implement the storages.

- [zenoh-backend-influxdb](https://github.com/eclipse-zenoh/zenoh-backend-influxdb/)

  This backend relies on [InfluxDB](https://www.influxdata.com/products/influxdb/) server
to implement the storages.

## Configuring storages

The storages are configured in the storage manager plugin configuration in the `plugins` section of the
[config.json](https://docs.rs/zenoh/latest/zenoh/struct.Config.html).

There are two concepts in the storage manager: "volumes" and "storages".

The "volume" is a specific database backend plugin (memory, s3, rocksdb, etc.) plus
a concrete backend configuration (database name, username, password, etc.).

The "storage" is a mapping from a glob key expression to a volume, defining where data
for each path is stored.

Here is a simple example config that sets up two storages. The `__path__` explicitly
sets up paths to dynamic plugins for different platforms.

```json
"plugins": {
    "storage_manager": {
        "volumes": {
            "example": {
                "__path__": ["target/debug/libzenoh_backend_example.so","target/debug/libzenoh_backend_example.dylib"],
            }
        },
        "storages": {
            "memory": {
                "volume": "memory",
                "key_expr": "demo/memory/**"
            },
            "example": {
                "volume": "example",
                "key_expr": "demo/example/**"
            }
        }
    }
}
```

## Usage of storages

Assume that we are in the root of the [zenoh](https://github.com/eclipse-zenoh/zenoh) repository.
Uncomment the following part in the `DEFAULT_CONFIG.json5` which demonstrates using
external plugin config files.

```json
"plugins|": {
  "rest": {
    "__config__": "./plugins/zenoh-plugin-rest/config.json5",
  },
  "storage_manager": {
    "__config__": "./plugins/zenoh-plugin-storage-manager/config.json5",
  }
},
```

Also either change option `timestamping -> enabled -> peer` to `true` or change the `mode` to `router` to enable message
timestamping which is required by storage manager.

Run the `zenohd` router with adminspace write permissions enabled:

```bash
RUST_LOG=info cargo run -- --config DEFAULT_CONFIG.json5 --adminspace-permissions rw
```

The `RUST_LOG=info` is important to enable logging from `zenoh_plugin_storage_manager` itself. Otherwise, the logs from plugins are suppressed.

The adminspace write permissions allow you to manage storages dynamically at runtime via the admin space REST API. See the example at the end.

Verify that the plugin loaded successfully: the output should contain lines like:

```sh
2025-10-02T13:33:38.279758Z  INFO main ThreadId(01) zenoh::api::loader: Loading  plugin "rest"
2025-10-02T13:33:38.530621Z  INFO main ThreadId(01) zenoh::api::loader: Loading  plugin "storage_manager"
2025-10-02T13:33:38.787932Z  INFO main ThreadId(01) zenoh::api::loader: Starting  plugin "rest"
2025-10-02T13:33:38.795413Z  INFO main ThreadId(01) zenoh::api::loader: Successfully started plugin rest from "/Users/milyin/ZS/zenoh/target/debug/libzenoh_plugin_rest.dylib"
2025-10-02T13:33:38.795433Z  INFO main ThreadId(01) zenoh::api::loader: Finished loading plugins
2025-10-02T13:33:38.795441Z  INFO main ThreadId(01) zenoh::api::loader: Starting  plugin "storage_manager"
2025-10-02T13:33:38.801661Z  INFO ThreadId(01) zenoh_plugin_storage_manager: Spawning volume 'memory' with backend 'memory'
2025-10-02T13:33:38.802118Z  INFO ThreadId(01) zenoh_plugin_storage_manager: Spawning volume 'example' with backend 'example'
2025-10-02T13:33:38.925730Z  INFO ThreadId(01) zenoh_plugin_storage_manager: Spawning storage 'example' from volume 'example' with backend 'example'
2025-10-02T13:33:38.926759Z  INFO ThreadId(01) zenoh_plugin_storage_manager: Spawning storage 'memory' from volume 'memory' with backend 'memory'
2025-10-02T13:33:38.926833Z  INFO main ThreadId(01) zenoh::api::loader: Successfully started plugin storage_manager from "/Users/milyin/ZS/zenoh/target/debug/libzenoh_plugin_storage_manager.dylib"
2025-10-02T13:33:38.926860Z  INFO main ThreadId(01) zenoh::api::loader: Finished loading plugins
```

Now we can store and retrieve data. This can be done by zenoh `put` and `get` operations as well as using [REST](https://crates.io/crates/zenoh-plugin-rest) API.

### Store and retrieve data

#### Using zenoh operations

Store some data using put:

```bash
cargo run --example z_put -- -k demo/memory/test -p 'Hello from zenoh put'
```

Then query the stored data:

```bash
cargo run --example z_get -- -s "demo/memory/test"
```

Expected output with the stored data

```console
>> Received ('demo/memory/test': 'Hello from zenoh put')
```

Notice that this would not work without storage enabled. The `put` operation belongs to the publish/subscribe API, while the `get` is part of the query/reply API.
It's the storage that runs a subscriber and querier for its key expression. In other words, storages link pub/sub and query/reply APIs together in
the Zenoh network.

#### Using REST API

Store data via REST PUT:

```bash
curl -X PUT -d '"Hello from REST"' -H "Content-Type: application/json" http://localhost:8080/demo/memory/test
```

Retrieve data via REST GET:

```bash
curl http://localhost:8080/demo/memory/test
```

Expected output:

```json
[{"key":"demo/memory/test","value":"Hello from REST","encoding":"application/json","timestamp":"7556630592622444272/d3f8913b8af0c842078d30652a04bd8f"}]
```

**Note**: Use the `application/json` content type and quoted value in `PUT` to get data as a string value in `GET` json output. Otherwise, the value is base-64 encoded to
ensure correctness of the json format.

Delete stored data with REST DELETE:

```bash
curl -X DELETE http://localhost:8080/demo/memory/test
```

You can verify the deletion by querying the key again:

```bash
curl http://localhost:8080/demo/memory/test
```

Expected output (empty array, indicating no data found):

```json
[]
```

### Using the admin space to manage storages

You can manage storages dynamically via the admin space - the predefined key expression starting with `@/`.

To view current storages:

```bash
curl -s 'http://localhost:8080/@/local/router/**/storages/*' | jq
```

Expected output (showing active storages):

```json
[
  {
    "key": "@/d3f8913b8af0c842078d30652a04bd8f/router/status/plugins/storage_manager/storages/memory",
    "value": {
      "key_expr": "demo/memory/**",
      "volume": "memory"
    },
    "encoding": "application/json",
    "timestamp": null
  },
  {
    "key": "@/d3f8913b8af0c842078d30652a04bd8f/router/status/plugins/storage_manager/storages/example",
    "value": null,
    "encoding": "application/json",
    "timestamp": null
  }
]
```

Add another memory storage dynamically:

```bash
curl -X PUT -H 'content-type:application/json' \
  -d '{"key_expr":"demo/sensors/**","volume":"memory"}' \
  http://localhost:8080/@/local/router/config/plugins/storage_manager/storages/sensors
```

Delete example storage dynamically:

```bash
curl -X DELETE http://localhost:8080/@/local/router/config/plugins/storage_manager/storages/example
```

Verify the new memory storage is created and the example storage is deleted:

```bash
curl -s 'http://localhost:8080/@/local/router/**/storages/*' | jq
```
