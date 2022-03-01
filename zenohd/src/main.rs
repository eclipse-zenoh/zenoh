//
// Copyright (c) 2017, 2020 ADLINK Technology Inc.
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ADLINK zenoh team, <zenoh@adlink-labs.tech>
//
use async_std::future;
use async_std::task;
use clap::ArgMatches;
use clap::{App, Arg};
use git_version::git_version;
use validated_struct::ValidatedMap;
use zenoh::config::{Config, EndPoint, PluginLoad};
use zenoh::net::runtime::{AdminSpace, Runtime};
use zenoh::plugins::PluginsManager;

const GIT_VERSION: &str = git_version!(prefix = "v", cargo_prefix = "v");

lazy_static::lazy_static!(
    static ref LONG_VERSION: String = format!("{} built with {}", GIT_VERSION, env!("RUSTC_VERSION"));
);

const DEFAULT_LISTENER: &str = "tcp/0.0.0.0:7447";

fn main() {
    task::block_on(async {
        #[cfg(feature = "stats")]
        env_logger::builder().format_timestamp_millis().init();
        #[cfg(not(feature = "stats"))]
        env_logger::init();

        log::debug!("zenohd {}", *LONG_VERSION);

        let app = App::new("The zenoh router")
            .version(GIT_VERSION)
            .long_version(LONG_VERSION.as_str())
            .arg(Arg::from_usage(
r#"-c, --config=[FILE] \
'The configuration file. Currently, this file must be a valid JSON5 or YAML file.'"#,
            ))
            .arg(Arg::from_usage(
r#"-l, --listen=[ENDPOINT]... \
'A locator on which this router will listen for incoming sessions.
Repeat this option to open several listeners.'"#,
                ),
            )
            .arg(Arg::from_usage(
r#"-e, --connect=[ENDPOINT]... \
'A peer locator this router will try to connect to.
Repeat this option to connect to several peers.'"#,
            ))
            .arg(Arg::from_usage(
r#"-i, --id=[HEX_STRING] \
'The identifier (as an hexadecimal string, with odd number of chars - e.g.: 0A0B23...) that zenohd must use.
WARNING: this identifier must be unique in the system and must be 16 bytes maximum (32 chars)!
If not set, a random UUIDv4 will be used.'"#,
            ))
            .arg(Arg::from_usage(
r#"-P, --plugin=[PLUGIN_NAME | PLUGIN_NAME:LIB_PATH]... \
'A plugin that MUST be loaded. You can give just the name of the plugin, zenohd will search for a library \
named 'libzplugin_<name>.so' (exact name depending the OS). Or you can give such a string: "<plugin_name>:<library_path>".
Repeat this option to load several plugins. If loading failed, zenohd will exit.'"#,
            ))
            .arg(Arg::from_usage(
r#"--plugin-search-dir=[DIRECTORY]... \
'A directory where to search for plugins libraries to load.
Repeat this option to specify several search directories.'"#))
            .arg(Arg::from_usage(
r#"--no-timestamp \
'By default zenohd adds a HLC-generated Timestamp to each routed Data if there isn't already one. \
This option disables this feature.'"#,
            )).arg(Arg::from_usage(
r#"--no-multicast-scouting \
'By default zenohd replies to multicast scouting messages for being discovered by peers and clients.
This option disables this feature.'"#,
            )).arg(Arg::from_usage(
r#"--cfg=[KEY:VALUE]... \
'Allows arbitrary configuration changes.
KEY must be a valid config path.
VALUE must be a valid JSON5 string that can be deserialized to the expected type for the KEY field.
Examples: `--cfg='join_on_startup/subscriptions:["/demo/**"]']` , or `--cfg='plugins/storages/backends/influxdb:{url: "localhost:1337", db: "myDB"}'`'"#
            )).arg(Arg::from_usage(
r#"--rest-http-port=[PORT | IP:PORT | none] \
'Configures HTTP interface for the REST API (enabled by default). Accepted values:
- either a port number
- either a string with format `<local_ip>:<port_number>` (to bind the HTTP server to a specific interface)
- either `none` to disable the REST API
'"#
            ).default_value("8000"));

        let args = app.get_matches();
        let config = config_from_args(&args);
        log::info!("Initial conf: {}", &config);

        let mut plugins = PluginsManager::new(config.libloader());
        // Static plugins are to be added here, with `.add_static::<PluginType>()`
        for plugin_load in config.plugins().load_requests() {
            let PluginLoad {
                name,
                paths,
                required,
            } = plugin_load;
            if let Err(e) = match paths {
                None => plugins.load_plugin_by_name(name),
                Some(paths) => plugins.load_plugin_by_paths(name, &paths),
            } {
                if required {
                    panic!("Plugin load failure: {}", e)
                } else {
                    log::error!("Plugin load failure: {}", e)
                }
            }
        }

        let runtime = match Runtime::new(config).await {
            Ok(runtime) => runtime,
            Err(e) => {
                println!("{}. Exiting...", e);
                std::process::exit(-1);
            }
        };

        for (name, path, start_result) in plugins.start_all(&runtime) {
            match start_result {
                Ok(Some(_)) => log::info!("Successfully started plugin {} from {:?}", name, path),
                Ok(None) => log::warn!("Plugin {} from {:?} wasn't loaded, as an other plugin by the same name is already running", name, path),
                Err(e) => {
                    let report = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| e.to_string())) {
                        Ok(s) => s,
                        Err(_) => panic!("Formatting the error from plugin {} ({:?}) failed, this is likely due to ABI unstability.\r\nMake sure your plugin was built with the same version of cargo as zenohd", name, path),
                    };
                    log::error!("Plugin start failure: {}", if report.is_empty() {"no details provided"} else {report.as_str()});
                }
            }
        }
        log::info!("Finished loading plugins");

        {
            let mut config_guard = runtime.config.lock();
            for (name, (_, plugin)) in plugins.running_plugins() {
                let hook = plugin.config_checker();
                config_guard.add_plugin_validator(name, hook)
            }
        }

        AdminSpace::start(&runtime, plugins, LONG_VERSION.clone()).await;

        future::pending::<()>().await;
    });
}

fn config_from_args(args: &ArgMatches) -> Config {
    let mut config = args
        .value_of("config")
        .map_or_else(Config::default, |conf_file| {
            Config::from_file(conf_file).unwrap()
        });
    if config.mode().is_none() {
        config
            .set_mode(Some(zenoh::config::WhatAmI::Router))
            .unwrap();
    }
    if args.occurrences_of("id") > 0 {
        config
            .set_id(args.value_of("id").map(|s| s.to_string()))
            .unwrap();
    }
    // apply '--rest-http-port' to config only if explicitly set (overwritting config),
    // or if no config file is set (to apply its default value)
    if args.occurrences_of("rest-http-port") > 0 || args.occurrences_of("config") == 0 {
        let value = args.value_of("rest-http-port").unwrap();
        if !value.eq_ignore_ascii_case("none") {
            config
                .insert_json5("plugins/rest/http_port", &format!(r#""{}""#, value))
                .unwrap();
        }
    }
    if let Some(plugins_search_dirs) = args.values_of("plugin-search-dir") {
        config
            .set_plugins_search_dirs(plugins_search_dirs.map(|c| c.to_owned()).collect())
            .unwrap();
    }
    if let Some(plugins) = args.values_of("plugin") {
        for plugin in plugins {
            match plugin.split_once(':') {
                Some((name, path)) => {
                    config
                        .insert_json5(format!("plugins/{}/__required__", name), "true")
                        .unwrap();
                    config
                        .insert_json5(
                            format!("plugins/{}/__path__", name),
                            &format!("\"{}\"", path),
                        )
                        .unwrap();
                }
                None => config
                    .insert_json5(format!("plugins/{}/__required__", plugin), "true")
                    .unwrap(),
            }
        }
    }
    if let Some(peers) = args.values_of("connect") {
        config
            .connect
            .set_endpoints(
                peers
                    .map(|v| match v.parse::<EndPoint>() {
                        Ok(v) => v,
                        Err(e) => {
                            panic!("Couldn't parse option --peer={} into Locator: {}", v, e);
                        }
                    })
                    .collect(),
            )
            .unwrap();
    }
    if let Some(listeners) = args.values_of("listen") {
        config
            .listen
            .set_endpoints(
                listeners
                    .map(|v| match v.parse::<EndPoint>() {
                        Ok(v) => v,
                        Err(e) => {
                            panic!("Couldn't parse option --listen={} into Locator: {}", v, e);
                        }
                    })
                    .collect(),
            )
            .unwrap();
    }
    if config.listen.endpoints.is_empty() {
        config
            .listen
            .endpoints
            .push(DEFAULT_LISTENER.parse().unwrap())
    }
    match (
        config.add_timestamp().is_none(),
        args.is_present("no-timestamp"),
    ) {
        (_, true) => {
            config.set_add_timestamp(Some(false)).unwrap();
        }
        (true, false) => {
            config.set_add_timestamp(Some(true)).unwrap();
        }
        (false, false) => {}
    };
    match (
        config.scouting.multicast.enabled().is_none(),
        args.is_present("no-multicast-scouting"),
    ) {
        (_, true) => {
            config.scouting.multicast.set_enabled(Some(false)).unwrap();
        }
        (true, false) => {
            config.scouting.multicast.set_enabled(Some(true)).unwrap();
        }
        (false, false) => {}
    };
    for json in args.values_of("cfg").unwrap_or_default() {
        if let Some((key, value)) = json.split_once(':') {
            match json5::Deserializer::from_str(value) {
                Ok(mut deserializer) => {
                    if let Err(e) =
                        config.insert(key.strip_prefix('/').unwrap_or(key), &mut deserializer)
                    {
                        log::warn!("Couldn't perform configuration {}: {}", json, e);
                    }
                }
                Err(e) => log::warn!("Couldn't perform configuration {}: {}", json, e),
            }
        }
    }
    log::debug!("Config: {:?}", &config);
    config
}
