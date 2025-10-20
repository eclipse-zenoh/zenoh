<img src="https://raw.githubusercontent.com/eclipse-zenoh/zenoh/master/zenoh-dragon.png" height="150">

# Eclipse Zenoh

Eclipse Zenoh: Zero Overhead Pub/Sub, Store/Query and Compute.

Zenoh (pronounce _/zeno/_) unifies data in motion, data at rest and computations. It carefully blends traditional pub/sub with geo-distributed storages, queries and computations, while retaining a level of time and space efficiency that is well beyond any of the mainstream stacks.

Check the website [zenoh.io](http://zenoh.io) for more information and [installation instructions](https://zenoh.io/docs/getting-started/installation/)

See also the [roadmap](https://github.com/eclipse-zenoh/roadmap) for more detailed technical information.

# Contents

- [zenoh-plugin-trait](zenoh-plugin-trait)

  The zenoh plugin [API](https://docs.rs/zenoh-plugin-trait/latest/zenoh_plugin_trait/).

  This crate introduces a common plugin library which provides:
  - the API to implement plugins
  - the API to load, start, and stop plugins and get their status

  The application-specific, functional part of plugins is implemented outside of this API, in the types passed as type
  arguments `StartArgs` and `Instance`.
  For example, the plugins for `zenohd` should implement the trait `ZenohPlugin` from the `zenoh` crate (under the `internal` feature) with
  `DynamicRuntime` and `RunningPlugin` types provided by `zenoh`.

  ```rust
  pub trait ZenohPlugin: Plugin<StartArgs = DynamicRuntime, Instance = RunningPlugin> {}
  ```

- [zenoh-plugin-example](zenoh-plugin-example)

  A simple example plugin for `zenohd`

- [zenoh-plugin-rest](zenoh-plugin-rest)

  The plugin implementing the [REST API](https://zenoh.io/docs/apis/rest/) for `zenohd`.

- [zenoh-plugin-storage-manager](zenoh-plugin-storage-manager)

  The plugin which allows connecting `zenohd` to different storages (e.g., databases). This plugin is a plugin manager itself which loads its own plugins - `backends` -
  specific for external storage APIs.

- [zenoh-backend-traits](zenoh-backend-traits)

  The backend API for the storage manager. It exports types `VolumeConfig` and `VolumeInstance` which are used by backends as the `Plugin` trait's type arguments.

- [zenoh-backend-example](zenoh-backend-example)

  A simple example backend plugin for `zenoh-plugin-storage-manager`
