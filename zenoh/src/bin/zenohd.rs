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
use clap::{App, Arg};
use git_version::git_version;
use validated_struct::ValidatedMap;
use zenoh::config::Config;
use zenoh::net::plugins::{PluginsManager, PLUGIN_PREFIX};
use zenoh::net::runtime::{AdminSpace, Runtime};
use zenoh_util::LibLoader;

const GIT_VERSION: &str = git_version!(prefix = "v", cargo_prefix = "v");

lazy_static::lazy_static!(
    static ref LONG_VERSION: String = format!("{} built with {}", GIT_VERSION, env!("RUSTC_VERSION"));
);

const DEFAULT_LISTENER: &str = "tcp/0.0.0.0:7447";

fn get_plugin_search_dirs_from_args() -> Vec<String> {
    let mut result: Vec<String> = vec![];
    let mut iter = std::env::args();
    while let Some(arg) = iter.next() {
        if arg == "--plugin-search-dir" {
            if let Some(arg2) = iter.next() {
                result.push(arg2);
            }
        } else if let Some(name) = arg.strip_prefix("--plugin-search-dir=") {
            result.push(name.to_string());
        }
    }
    result
}

fn get_plugins_from_args() -> Vec<String> {
    let mut result: Vec<String> = vec![];
    let mut iter = std::env::args();
    while let Some(arg) = iter.next() {
        if arg == "-P" || arg == "--plugin" {
            if let Some(arg2) = iter.next() {
                result.push(arg2);
            }
        } else if let Some(name) = arg.strip_prefix("--plugin=") {
            result.push(name.to_string());
        }
    }
    result
}

fn main() {
    task::block_on(async {
        #[cfg(feature = "stats")]
        env_logger::builder().format_timestamp_millis().init();
        #[cfg(not(feature = "stats"))]
        env_logger::init();

        log::debug!("zenohd {}", *LONG_VERSION);

        let plugin_search_dir_usage = format!(
            "--plugin-search-dir=[DIRECTORY]... \
            'A directory where to search for plugins libraries to load. \
            Repeat this option to specify several search directories'. \
            By default, the plugins libraries will be searched in: '{}' .",
            LibLoader::default_search_paths()
        );

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
            .arg(Arg::from_usage(
                "--plugin-nolookup \
             'When set, zenohd will not look for plugins nor try to load any plugin except the \
             ones explicitely configured with -P or --plugin.'",
            ))
            .arg(Arg::from_usage(&plugin_search_dir_usage).conflicts_with("plugin-nolookup"))
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
            );

        // Get plugins search directories from the command line, and create LibLoader
        let plugin_search_dirs = get_plugin_search_dirs_from_args();
        let lib_loader = if !plugin_search_dirs.is_empty() {
            LibLoader::new(plugin_search_dirs.as_slice(), false)
        } else {
            LibLoader::default()
        };

        let mut plugins = PluginsManager::builder()
            // Static plugins are to be added here, with `.add_static::<PluginType>()`
            .into_dynamic(lib_loader)
            .load_plugins(&get_plugins_from_args(), &PLUGIN_PREFIX);
        // Also search for plugins if no "--plugin-nolookup" arg
        if !std::env::args().any(|arg| arg == "--plugin-nolookup") {
            plugins = plugins.search_and_load_plugins(Some(&PLUGIN_PREFIX));
        }
        let (plugins, expected_args) = plugins.get_requirements();

        // Add plugins' expected args and parse command line
        let args = app.args(&expected_args).get_matches();

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

        let runtime = match Runtime::new(0, config, args.value_of("id")).await {
            Ok(runtime) => runtime,
            Err(e) => {
                println!("{}. Exiting...", e);
                std::process::exit(-1);
            }
        };

        let (handles, failures) = plugins.start(&(runtime.clone(), args));
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
