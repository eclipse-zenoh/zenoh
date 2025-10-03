<img src="https://raw.githubusercontent.com/eclipse-zenoh/zenoh/master/zenoh-dragon.png" height="150">

[![CI](https://github.com/eclipse-zenoh/zenoh/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/eclipse-zenoh/zenoh/actions?query=workflow%3ACI+branch%3Amain++)
[![Documentation Status](https://readthedocs.org/projects/zenoh-rust/badge/?version=latest)](https://zenoh-rust.readthedocs.io/en/latest/?badge=latest)
[![codecov](https://codecov.io/github/eclipse-zenoh/zenoh/branch/main/graph/badge.svg?token=F8T4C8WPZD)](https://codecov.io/github/eclipse-zenoh/zenoh)
[![Discussion](https://img.shields.io/badge/discussion-on%20github-blue)](https://github.com/eclipse-zenoh/roadmap/discussions)
[![Discord](https://img.shields.io/badge/chat-on%20discord-blue)](https://discord.gg/2GJ958VuHs)
[![License](https://img.shields.io/badge/License-EPL%202.0-blue)](https://choosealicense.com/licenses/epl-2.0/)
[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)

# Eclipse Zenoh

The Eclipse Zenoh: Zero Overhead Pub/Sub, Store/Query and Compute.

Zenoh (pronounce _/zeno/_) unifies data in motion, data at rest and computations. It carefully blends traditional pub/sub with geo-distributed storages, queries and computations, while retaining a level of time and space efficiency that is well beyond any of the mainstream stacks.

Check the website [zenoh.io](http://zenoh.io) for more information and [installation instructions](https://zenoh.io/docs/getting-started/installation/).

See also the [roadmap](https://github.com/eclipse-zenoh/roadmap) for more detailed technical information.

# REST Plugin for `zenohd` router

The plugin exposes a [REST API](https://zenoh.io/docs/apis/rest/) in [zenohd](https://crates.io/crates/zenohd).
The `zenohd` links this plugin statically, it's not necessary to install it. To activate the REST plugin
the CLI argument `--rest-http-port=<port>` should be passed to `zenohd`.

The port can also be configured in the plugin configuration in the `plugins` section
of the [config.json](https://docs.rs/zenoh/latest/zenoh/struct.Config.html).

```json
"plugins": {
  "rest": {
    "http_port": 8000,
  }
}
```

This plugin translates `PUT` and `DELETE` REST operations into the pub/sub API and `GET` operations to the query/reply API.

For example:

```bash
cargo run -- --config DEFAULT_CONFIG.json5 --rest-http-port 8000
```

```bash
cargo run --example z_sub -- -k foo/bar
```

```bash
curl -X PUT -d '"Hello World!"' http://localhost:8080/foo/bar
```

The subscriber in `z_sub` example prints

```bash
>> [Subscriber] Received PUT ('foo/bar': '"Hello World!"')
```

```bash
cargo run --example z_queryable -- -k foo/bar
```

```bash
curl http://localhost:8080/foo/bar
```

The queryable `z_get` prints

```bash
>> [Queryable ] Received Query 'foo/bar'
>> [Queryable ] Responding ('foo/bar': 'Queryable from Rust!')
```

and `curl` prints

```bash
[{"key":"foo/bar","value":"UXVlcnlhYmxlIGZyb20gUnVzdCE=","encoding":"zenoh/bytes","timestamp":null}]
```

See also examples of using REST API for storages in the [zenoh-plugin-storage-manager](https://crates.io/crates/zenoh-plugin-storage-manager).
