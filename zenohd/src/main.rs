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
use git_version::git_version;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use zenoh::{config::WhatAmI, Config, Result, Wait};
use zenoh_config::{EndPoint, ModeDependentValue, PermissionsConf};
use zenoh_util::LibSearchDirs;

const GIT_VERSION: &str = git_version!(prefix = "v", cargo_prefix = "v");

lazy_static::lazy_static!(
    static ref LONG_VERSION: String = format!("{} built with {}", GIT_VERSION, env!("RUSTC_VERSION"));
);

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
    /// A plugin that MUST be loaded. You can give just the name of the plugin, zenohd will search for a library named 'libzenoh_plugin_\<name\>.so' (exact name depending the OS). Or you can give such a string: "\<plugin_name\>:\<library_path\>"
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
    /// Enables REST API plugin and configures HTTP interface for it. Accepted values:
    ///   - a port number
    ///   - a string with format `<local_ip>:<port_number>` (to bind the HTTP server to a specific interface)
    #[arg(long, value_name = "SOCKET")]
    rest_http_port: Option<String>,
    /// Allows arbitrary configuration changes as column-separated KEY:VALUE pairs,
    /// where the empty key is used to represent the entire configuration:
    ///   - KEY must be a valid config path, or empty string if the whole configuration is defined.
    ///   - VALUE must be a valid JSON5 string that can be deserialized to the expected type for the KEY field.
    ///
    /// Examples:
    /// - `--cfg='startup/subscribe:["demo/**"]'`
    /// - `--cfg='plugins/storage_manager/storages/demo:{key_expr:"demo/example/**",volume:"memory"}'`
    /// - `--cfg=':{metadata:{name:"My App"},adminspace:{enabled:true,permissions:{read:true,write:true}}'`
    #[arg(long)]
    cfg: Vec<String>,
    /// Configure the read and/or write permissions on the admin space. Default is read only.
    #[arg(long, value_name = "[r|w|rw|none]")]
    adminspace_permissions: Option<String>,
}

fn main() {
    if let Err(e) = init_logging() {
        eprintln!("{e}. Exiting...");
        std::process::exit(-1);
    }

    tracing::info!("zenohd {}", *LONG_VERSION);

    let args = Args::parse();
    let config = config_from_args(&args);
    tracing::info!("Initial conf: {}", &config);

    let _session = match zenoh::open(config).wait() {
        Ok(runtime) => runtime,
        Err(e) => {
            eprintln!("{e}. Exiting...");
            std::process::exit(-1);
        }
    };

    std::thread::park();
}

fn config_from_args(args: &Args) -> Config {
    let mut inline_config = None;
    for json in &args.cfg {
        if let Some(("", cfg)) = json.split_once(':') {
            inline_config = Some(cfg);
        }
    }

    let mut config = if let Some(cfg) = inline_config {
        Config::from_json5(cfg).expect("Invalid Zenoh config")
    } else if let Some(fname) = args.config.as_ref() {
        Config::from_file(fname).expect("Failed to load config file")
    } else {
        Config::default()
    };

    if config.mode().is_none() {
        config.set_mode(Some(WhatAmI::Router)).unwrap();
    }
    if let Some(id) = &args.id {
        config.set_id(Some(id.parse().unwrap())).unwrap();
    }
    // apply '--rest-http-port' to config only if explicitly set (overwriting config)
    if args.rest_http_port.is_some() {
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
    config.adminspace.set_enabled(true).unwrap();
    config.plugins_loading.set_enabled(true).unwrap();
    if !args.plugin_search_dir.is_empty() {
        config
            .plugins_loading
            // REVIEW: Should this append to search_dirs instead? As there is no way to pass the new
            // `current_exe_parent` unless we change the format of the argument and this overrides
            // the one set from the default config. 
            // Also, --cfg plugins_loading/search_dirs=[...] makes this argument superfluous.
            .set_search_dirs(LibSearchDirs::from_paths(&args.plugin_search_dir))
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
            .endpoints
            .set(
                args.connect
                    .iter()
                    .map(|v| match v.parse::<EndPoint>() {
                        Ok(v) => v,
                        Err(e) => {
                            panic!("Couldn't parse option --peer={v} into Locator: {e}");
                        }
                    })
                    .collect(),
            )
            .unwrap();
    }
    if !args.listen.is_empty() {
        config
            .listen
            .endpoints
            .set(
                args.listen
                    .iter()
                    .map(|v| match v.parse::<EndPoint>() {
                        Ok(v) => v,
                        Err(e) => {
                            panic!("Couldn't parse option --listen={v} into Locator: {e}");
                        }
                    })
                    .collect(),
            )
            .unwrap();
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
                r#"Invalid option: --adminspace-permissions={s} - Accepted values: "r", "w", "rw" or "none""#
            ),
        };
    }
    for json in &args.cfg {
        if let Some((key, value)) = json.split_once(':') {
            if !key.is_empty() {
                match json5::Deserializer::from_str(value) {
                    Ok(mut deserializer) => {
                        if let Err(e) =
                            config.insert(key.strip_prefix('/').unwrap_or(key), &mut deserializer)
                        {
                            tracing::warn!("Couldn't perform configuration {}: {}", json, e);
                        }
                    }
                    Err(e) => tracing::warn!("Couldn't perform configuration {}: {}", json, e),
                }
            }
        } else {
            panic!("--cfg accepts KEY:VALUE pairs. {json} is not a valid KEY:VALUE pair.")
        }
    }
    tracing::debug!("Config: {:?}", &config);
    config
}

fn init_logging() -> Result<()> {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("z=info"));

    let fmt_layer = tracing_subscriber::fmt::Layer::new()
        .with_thread_ids(true)
        .with_thread_names(true)
        .with_level(true)
        .with_target(true);

    let tracing_sub = tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt_layer);

    tracing_sub.init();
    Ok(())
}

#[test]
#[cfg(feature = "default")]
fn test_default_features() {
    assert_eq!(
        zenoh::FEATURES,
        concat!(
            " zenoh/auth_pubkey",
            " zenoh/auth_usrpwd",
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
