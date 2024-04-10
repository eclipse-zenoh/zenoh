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
use clap::Parser;
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

#[derive(Debug, Parser)]
#[command(version=GIT_VERSION, long_version=LONG_VERSION.as_str(), about="The zenoh router")]
struct Args {
    /// The configuration file. Currently, this file must be a valid JSON5 or YAML file.
    #[arg(short, long, value_name = "PATH")]
    config: Option<String>,
    /// Locators on which this router will listen for incoming sessions. Repeat this option to open several listeners.
    #[arg(short, long, value_name = "ENDPOINT")]
    listen: Vec<String>,
    /// A peer locator this router will try to connect to.
    /// Repeat this option to connect to several peers.
    #[arg(short = 'e', long, value_name = "ENDPOINT")]
    connect: Vec<String>,
    /// The identifier (as an hexadecimal string, with odd number of chars - e.g.: A0B23...) that zenohd must use. If not set, a random unsigned 128bit integer will be used.
    /// WARNING: this identifier must be unique in the system and must be 16 bytes maximum (32 chars)!
    #[arg(short, long)]
    id: Option<String>,
    /// A plugin that MUST be loaded. You can give just the name of the plugin, zenohd will search for a library named 'libzenoh_plugin_<name>.so' (exact name depending the OS). Or you can give such a string: "<plugin_name>:<library_path>
    /// Repeat this option to load several plugins. If loading failed, zenohd will exit.
    #[arg(short = 'P', long)]
    plugin: Vec<String>,
    /// Directory where to search for plugins libraries to load.
    /// Repeat this option to specify several search directories.
    #[arg(long, value_name = "PATH")]
    plugin_search_dir: Vec<String>,
    /// By default zenohd adds a HLC-generated Timestamp to each routed Data if there isn't already one. This option disables this feature.
    #[arg(long)]
    no_timestamp: bool,
    /// By default zenohd replies to multicast scouting messages for being discovered by peers and clients. This option disables this feature.
    #[arg(long)]
    no_multicast_scouting: bool,
    /// Configures HTTP interface for the REST API (enabled by default on port 8000). Accepted values:
    ///   - a port number
    ///   - a string with format `<local_ip>:<port_number>` (to bind the HTTP server to a specific interface)
    ///   - `none` to disable the REST API
    #[arg(long, value_name = "SOCKET")]
    rest_http_port: Option<String>,
    /// Allows arbitrary configuration changes as column-separated KEY:VALUE pairs, where:
    ///   - KEY must be a valid config path.
    ///   - VALUE must be a valid JSON5 string that can be deserialized to the expected type for the KEY field.
    /// Examples:
    /// --cfg='startup/subscribe:["demo/**"]'
    /// --cfg='plugins/storage_manager/storages/demo:{key_expr:"demo/example/**",volume:"memory"}'
    #[arg(long)]
    cfg: Vec<String>,
    /// Configure the read and/or write permissions on the admin space. Default is read only.
    #[arg(long, value_name = "[r|w|rw|none]")]
    adminspace_permissions: Option<String>,
}

fn load_plugin(
    plugin_mgr: &mut PluginsManager,
    name: &str,
    paths: &Option<Vec<String>>,
) -> Result<()> {
    let declared = if let Some(declared) = plugin_mgr.plugin_mut(name) {
        log::warn!("Plugin `{}` was already declared", declared.name());
        declared
    } else if let Some(paths) = paths {
        plugin_mgr.declare_dynamic_plugin_by_paths(name, paths)?
    } else {
        plugin_mgr.declare_dynamic_plugin_by_name(name, name)?
    };

    if let Some(loaded) = declared.loaded_mut() {
        log::warn!(
            "Plugin `{}` was already loaded from {}",
            loaded.name(),
            loaded.path()
        );
    } else {
        let _ = declared.load()?;
    };
    Ok(())
}

fn main() {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
        let mut log_builder =
            env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("z=info"));
        #[cfg(feature = "stats")]
        log_builder.format_timestamp_millis().init();
        #[cfg(not(feature = "stats"))]
        log_builder.init();

        log::info!("zenohd {}", *LONG_VERSION);

        let args = Args::parse();
        let config = config_from_args(&args);
        log::info!("Initial conf: {}", &config);

        let mut plugin_mgr = PluginsManager::dynamic(config.libloader(), "zenoh_plugin_");
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
            if let Err(e) = load_plugin(&mut plugin_mgr, &name, &paths) {
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

        for plugin in plugin_mgr.loaded_plugins_iter_mut() {
            let required = required_plugins.contains(plugin.name());
            log::info!(
                "Starting {req} plugin \"{name}\"",
                req = if required { "required" } else { "" },
                name = plugin.name()
            );
            match plugin.start(&runtime) {
                Ok(_) => {
                    log::info!(
                        "Successfully started plugin {} from {:?}",
                        plugin.name(),
                        plugin.path()
                    );
                }
                Err(e) => {
                    let report = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| e.to_string())) {
                        Ok(s) => s,
                        Err(_) => panic!("Formatting the error from plugin {} ({:?}) failed, this is likely due to ABI unstability.\r\nMake sure your plugin was built with the same version of cargo as zenohd", plugin.name(), plugin.path()),
                    };
                    if required {
                        panic!(
                            "Plugin \"{}\" failed to start: {}",
                            plugin.name(),
                            if report.is_empty() {
                                "no details provided"
                            } else {
                                report.as_str()
                            }
                        );
                    } else {
                        log::error!(
                            "Required plugin \"{}\" failed to start: {}",
                            plugin.name(),
                            if report.is_empty() {
                                "no details provided"
                            } else {
                                report.as_str()
                            }
                        );
                    }
                }
            }
        }
        log::info!("Finished loading plugins");

        AdminSpace::start(&runtime, plugin_mgr, LONG_VERSION.clone()).await;

        future::pending::<()>().await;
    });
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
            // " zenoh/transport_vsock",
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
            // " zenoh/transport_vsock",
            " zenoh/unstable",
            // " zenoh/default",
        )
    );
}
