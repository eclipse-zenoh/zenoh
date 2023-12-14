//
// Copyright (c) 2023 ZettaScale Technology
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ZettaScale Zenoh Team, <zenoh@zettascale.tech>
//
use clap::{ArgMatches, Command};
use futures::future;
use git_version::git_version;
use std::collections::HashSet;
use zenoh::config::{Config, ModeDependentValue, PermissionsConf, PluginLoad, ValidatedMap};
use zenoh::plugins::PluginsManager;
use zenoh::prelude::{EndPoint, WhatAmI};
use zenoh::runtime::{AdminSpace, Runtime};
use zenoh::Result;

const GIT_VERSION: &str = git_version!(prefix = "v", cargo_prefix = "v");

lazy_static::lazy_static!(
    static ref LONG_VERSION: String = format!("{} built with {}", GIT_VERSION, env!("RUSTC_VERSION"));
);

const DEFAULT_LISTENER: &str = "tcp/[::]:7447";

#[tokio::main]
async fn main() {
    let mut log_builder =
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("z=info"));
    #[cfg(feature = "stats")]
    log_builder.format_timestamp_millis().init();
    #[cfg(not(feature = "stats"))]
    log_builder.init();

    log::info!("zenohd {}", *LONG_VERSION);

    let app = Command::new("The zenoh router")
        .version(GIT_VERSION)
        .long_version(LONG_VERSION.as_str()).args(
            &[
clap::arg!(-c --config [FILE] "The configuration file. Currently, this file must be a valid JSON5 or YAML file."),
clap::Arg::new("listen").short('l').long("listen").value_name("ENDPOINT").help(r"A locator on which this router will listen for incoming sessions.
Repeat this option to open several listeners.").takes_value(true).multiple_occurrences(true),
clap::Arg::new("connect").short('e').long("connect").value_name("ENDPOINT").help(r"A peer locator this router will try to connect to.
Repeat this option to connect to several peers.").takes_value(true).multiple_occurrences(true),
clap::Arg::new("id").short('i').long("id").value_name("HEX_STRING").help(r"The identifier (as an hexadecimal string, with odd number of chars - e.g.: A0B23...) that zenohd must use. If not set, a random unsigned 128bit integer will be used.
WARNING: this identifier must be unique in the system and must be 16 bytes maximum (32 chars)!").multiple_values(false).multiple_occurrences(false),
clap::Arg::new("plugin").short('P').long("plugin").value_name("PLUGIN").takes_value(true).multiple_occurrences(true).help(r#"A plugin that MUST be loaded. You can give just the name of the plugin, zenohd will search for a library named 'libzenoh_plugin_<name>.so' (exact name depending the OS). Or you can give such a string: "<plugin_name>:<library_path>".
Repeat this option to load several plugins. If loading failed, zenohd will exit."#),
clap::Arg::new("plugin-search-dir").long("plugin-search-dir").takes_value(true).multiple_occurrences(true).value_name("DIRECTORY").help(r"A directory where to search for plugins libraries to load.
Repeat this option to specify several search directories."),
clap::arg!(--"no-timestamp" r"By default zenohd adds a HLC-generated Timestamp to each routed Data if there isn't already one. This option disables this feature."),
clap::arg!(--"no-multicast-scouting" r"By default zenohd replies to multicast scouting messages for being discovered by peers and clients. This option disables this feature."),
clap::arg!(--"rest-http-port" [SOCKET] r"Configures HTTP interface for the REST API (enabled by default). Accepted values:
- a port number
- a string with format `<local_ip>:<port_number>` (to bind the HTTP server to a specific interface)
- `none` to disable the REST API
").default_value("8000").multiple_values(false).multiple_occurrences(false),
clap::Arg::new("cfg").long("cfg").takes_value(true).multiple_occurrences(true).value_name("KEY:VALUE").help(
r#"Allows arbitrary configuration changes as column-separated KEY:VALUE pairs, where:
- KEY must be a valid config path.
- VALUE must be a valid JSON5 string that can be deserialized to the expected type for the KEY field.
Examples:
--cfg='startup/subscribe:["demo/**"]'
--cfg='plugins/storage_manager/storages/demo:{key_expr:"demo/example/**",volume:"memory"}'"#),
clap::Arg::new("adminspace-permissions").long("adminspace-permissions").value_name("[r|w|rw|none]").help(r"Configure the read and/or write permissions on the admin space. Default is read only."),
            ]
        );
    let args = app.get_matches();
    let config = config_from_args(&args);
    log::info!("Initial conf: {}", &config);

    let mut plugins = PluginsManager::dynamic(config.libloader());
    // Static plugins are to be added here, with `.add_static::<PluginType>()`
    let mut required_plugins = HashSet::new();
    for plugin_load in config.plugins().load_requests() {
        let PluginLoad {
            name,
            paths,
            required,
        } = plugin_load;
        log::info!(
            "Loading {req} plugin \"{name}\"",
            req = if required { "required" } else { "" }
        );
        if let Err(e) = match paths {
            None => plugins.load_plugin_by_name(name.clone()),
            Some(paths) => plugins.load_plugin_by_paths(name.clone(), &paths),
        } {
            if required {
                panic!("Plugin load failure: {}", e)
            } else {
                log::error!("Plugin load failure: {}", e)
            }
        }
        if required {
            required_plugins.insert(name);
        }
    }

    let runtime = match Runtime::new(config).await {
        Ok(runtime) => runtime,
        Err(e) => {
            println!("{e}. Exiting...");
            std::process::exit(-1);
        }
    };

    for (name, path, start_result) in plugins.start_all(&runtime) {
        let required = required_plugins.contains(name);
        log::info!(
            "Starting {req} plugin \"{name}\"",
            req = if required { "required" } else { "" }
        );
        match start_result {
            Ok(Some(_)) => log::info!("Successfully started plugin {} from {:?}", name, path),
            Ok(None) => log::warn!("Plugin {} from {:?} wasn't loaded, as an other plugin by the same name is already running", name, path),
            Err(e) => {
                let report = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| e.to_string())) {
                    Ok(s) => s,
                    Err(_) => panic!("Formatting the error from plugin {} ({:?}) failed, this is likely due to ABI unstability.\r\nMake sure your plugin was built with the same version of cargo as zenohd", name, path),
                };
                if required {
                    panic!("Plugin \"{name}\" failed to start: {}", if report.is_empty() {"no details provided"} else {report.as_str()});
                }else {
                    log::error!("Required plugin \"{name}\" failed to start: {}", if report.is_empty() {"no details provided"} else {report.as_str()});
                }
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
}

fn config_from_args(args: &Args) -> Config {
    let mut config = args
        .config
        .as_ref()
        .map_or_else(Config::default, |conf_file| {
            Config::from_file(conf_file).unwrap()
        });

    if config.mode().is_none() {
        config.set_mode(Some(WhatAmI::Router)).unwrap();
    }
    if let Some(id) = &args.id {
        config.set_id(id.parse().unwrap()).unwrap();
    }
    // apply '--rest-http-port' to config only if explicitly set (overwritting config),
    // or if no config file is set (to apply its default value)
    if args.rest_http_port.is_some() || args.config.is_none() {
        let value = args.rest_http_port.as_deref().unwrap_or("8000");
        if !value.eq_ignore_ascii_case("none") {
            config
                .insert_json5("plugins/rest/http_port", &format!(r#""{value}""#))
                .unwrap();
            config
                .insert_json5("plugins/rest/__required__", "true")
                .unwrap();
        }
    }
    if !args.plugin_search_dir.is_empty() {
        config
            .set_plugins_search_dirs(args.plugin_search_dir.clone())
            .unwrap();
    }
    for plugin in &args.plugin {
        match plugin.split_once(':') {
            Some((name, path)) => {
                config
                    .insert_json5(&format!("plugins/{name}/__required__"), "true")
                    .unwrap();
                config
                    .insert_json5(&format!("plugins/{name}/__path__"), &format!("\"{path}\""))
                    .unwrap();
            }
            None => config
                .insert_json5(&format!("plugins/{plugin}/__required__"), "true")
                .unwrap(),
        }
    }
    if !args.connect.is_empty() {
        config
            .connect
            .set_endpoints(
                args.connect
                    .iter()
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
    if !args.listen.is_empty() {
        config
            .listen
            .set_endpoints(
                args.listen
                    .iter()
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
    if args.no_timestamp {
        config
            .timestamping
            .set_enabled(Some(ModeDependentValue::Unique(false)))
            .unwrap();
    };
    match (
        config.scouting.multicast.enabled().is_none(),
        args.no_multicast_scouting,
    ) {
        (_, true) => {
            config.scouting.multicast.set_enabled(Some(false)).unwrap();
        }
        (true, false) => {
            config.scouting.multicast.set_enabled(Some(true)).unwrap();
        }
        (false, false) => {}
    };
    if let Some(adminspace_permissions) = &args.adminspace_permissions {
        match adminspace_permissions.as_str() {
            "r" => config
                .adminspace
                .set_permissions(PermissionsConf {
                    read: true,
                    write: false,
                })
                .unwrap(),
            "w" => config
                .adminspace
                .set_permissions(PermissionsConf {
                    read: false,
                    write: true,
                })
                .unwrap(),
            "rw" => config
                .adminspace
                .set_permissions(PermissionsConf {
                    read: true,
                    write: true,
                })
                .unwrap(),
            "none" => config
                .adminspace
                .set_permissions(PermissionsConf {
                    read: false,
                    write: false,
                })
                .unwrap(),
            s => panic!(
                r#"Invalid option: --adminspace-permissions={} - Accepted values: "r", "w", "rw" or "none""#,
                s
            ),
        };
    }
    for json in &args.cfg {
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

#[test]
#[cfg(feature = "default")]
fn test_default_features() {
    assert_eq!(
        zenoh::FEATURES,
        concat!(
            " zenoh/auth_pubkey",
            " zenoh/auth_usrpwd",
            // " zenoh/complete_n",
            // " zenoh/shared-memory",
            // " zenoh/stats",
            " zenoh/transport_multilink",
            " zenoh/transport_quic",
            // " zenoh/transport_serial",
            // " zenoh/transport_unixpipe",
            " zenoh/transport_tcp",
            " zenoh/transport_tls",
            " zenoh/transport_udp",
            " zenoh/transport_unixsock-stream",
            " zenoh/transport_ws",
            " zenoh/unstable",
            " zenoh/default",
        )
    );
}

#[test]
#[cfg(not(feature = "default"))]
fn test_no_default_features() {
    assert_eq!(
        zenoh::FEATURES,
        concat!(
            // " zenoh/auth_pubkey",
            // " zenoh/auth_usrpwd",
            // " zenoh/complete_n",
            // " zenoh/shared-memory",
            // " zenoh/stats",
            // " zenoh/transport_multilink",
            // " zenoh/transport_quic",
            // " zenoh/transport_serial",
            // " zenoh/transport_unixpipe",
            // " zenoh/transport_tcp",
            // " zenoh/transport_tls",
            // " zenoh/transport_udp",
            // " zenoh/transport_unixsock-stream",
            // " zenoh/transport_ws",
            " zenoh/unstable",
            // " zenoh/default",
        )
    );
}
