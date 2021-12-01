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
use zenoh::config::Config;
use zenoh::config::PluginLoad;
use zenoh::net::plugins::PluginsManager;
use zenoh::net::runtime::{AdminSpace, Runtime};
use zenoh_util::LibLoader;

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
                "-c, --config=[FILE] \
             'The configuration file. Currently, this file must be a valid JSON5 file.'",
            ))
            .arg(Arg::from_usage(
                "-l, --listener=[LOCATOR]... \
             'A locator on which this router will listen for incoming sessions. \
             Repeat this option to open several listeners.'",
                ).default_value(DEFAULT_LISTENER),
            )
            .arg(Arg::from_usage(
                "-e, --peer=[LOCATOR]... \
            'A peer locator this router will try to connect to. \
            Repeat this option to connect to several peers.'",
            ))
            .arg(Arg::from_usage(
                "-i, --id=[hex_string] \
            'The identifier (as an hexadecimal string - e.g.: 0A0B23...) that zenohd must use. \
            WARNING: this identifier must be unique in the system! \
            If not set, a random UUIDv4 will be used.'",
            ))
            .arg(Arg::from_usage(
                "-P, --plugin=[PATH_TO_PLUGIN_LIB]... \
             'A plugin that must be loaded. Repeat this option to load several plugins.'",
            ))
            .arg(Arg::from_usage("--plugin-search-dir=[DIRECTORY]... \
            'A directory where to search for plugins libraries to load. \
            Repeat this option to specify several search directories'.").conflicts_with("plugin-nolookup"))
            .arg(Arg::from_usage(
                "--no-timestamp \
             'By default zenohd adds a HLC-generated Timestamp to each routed Data if there isn't already one. \
             This option disables this feature.'",
            )).arg(Arg::from_usage(
                "--no-multicast-scouting \
                'By default zenohd replies to multicast scouting messages for being discovered by peers and clients. 
                This option disables this feature.'",
            )).arg(Arg::with_name("cfg")
                .long("cfg")
                .required(false)
                .takes_value(true)
                .value_name("KEY:VALUE")
                .help("Allows arbitrary configuration changes.\r\nKEY must be a valid config path.\r\nVALUE must be a valid JSON5 string that can be deserialized to the expected type for the KEY field.")
            ).arg(Arg::with_name("rest-port")
                .long("port")
                .required(false)
                .takes_value(true)
                .value_name("PORT")
                .default_value("8000")
                .help("Maps to `--cfg=/plugins/rest/port:PORT`. To disable the rest plugin, pass `--port=None`")
            ).arg(Arg::with_name("domain-id")
                .long("domain-id")
                .required(false)
                .takes_value(true)
                .value_name("DOMAIN")
                .help("Maps to --cfg=/plugins/dds/domain-id:\"DOMAIN\"")
            );

        let args = app.get_matches();
        let config = config_from_args(&args);

        let mut plugins = PluginsManager::builder()
            // Static plugins are to be added here, with `.add_static::<PluginType>()`
            .into_dynamic(config.libloader());
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

        let runtime = match Runtime::new(0, config, args.value_of("id")).await {
            Ok(runtime) => runtime,
            Err(e) => {
                println!("{}. Exiting...", e);
                std::process::exit(-1);
            }
        };

        let (handles, failures) = plugins.build().start(&runtime);
        for p in handles.plugins() {
            log::debug!("loaded plugin: {} from {:?}", p.name, p.path);
        }
        for f in failures {
            log::debug!("plugin_failure: {}", f);
        }

        AdminSpace::start(&runtime, handles, LONG_VERSION.clone()).await;

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
    if let Some(value) = args.value_of("rest-port") {
        if value.parse::<usize>().is_ok() {
            config.insert_json5("plugins/rest/port", value).unwrap();
        }
    }
    if let Some(value) = args.value_of("domain-id") {
        config
            .insert_json5("plugins/dds/domain-id", &format!("\"{}\"", value))
            .unwrap();
    }
    if let Some(plugins_search_dirs) = args.values_of("plugin-search-dir") {
        config
            .set_plugins_search_dirs(plugins_search_dirs.map(|c| c.to_owned()).collect())
            .unwrap();
    }
    if !args.is_present("plugin-nolookup") {
        config.set_plugins_search_dirs(Vec::new()).unwrap();
    }
    if let Some(paths) = args.values_of("plugin") {
        for path in paths {
            if path.contains('.') || path.contains('/') {
                let name = LibLoader::plugin_name(&path).unwrap();
                config
                    .insert_json5(
                        format!("plugins/{}/__path__", name),
                        &format!("\"{}\"", path),
                    )
                    .unwrap();
                config
                    .insert_json5(format!("plugins/{}/__required__", name), "true")
                    .unwrap();
            } else {
                config
                    .insert_json5(format!("plugins/{}/__required__", path), "true")
                    .unwrap();
            }
        }
    }
    config
        .peers
        .extend(
            args.values_of("peer")
                .unwrap_or_default()
                .filter_map(|v| match v.parse() {
                    Ok(v) => Some(v),
                    Err(e) => {
                        log::warn!("Couldn't parse {} into Locator: {}", v, e);
                        None
                    }
                }),
        );
    config.listeners.extend(
        args.values_of("listener")
            .unwrap_or_default()
            .filter_map(|v| match v.parse() {
                Ok(v) => Some(v),
                Err(e) => {
                    log::warn!("Couldn't parse {} into Locator: {}", v, e);
                    None
                }
            }),
    );
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
                    if let Err(e) = config.insert(key, &mut deserializer) {
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
