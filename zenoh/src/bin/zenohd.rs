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
use zenoh::net::plugins::*;
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
             'The configuration file.'",
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
            )).arg(Arg::from_usage(
                "--json=[<key>:<value>]\
                'Allows changing arbitrary parts of the configuration by passing a slash-separated path to the 
                property as the key, and a JSON deserializable value (don't forget the quotes on strings).'"
            ));

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

        let mut config = if let Some(conf_file) = args.value_of("config") {
            Config::from_file(conf_file).unwrap()
        } else {
            Config::default()
        };

        config
            .set_mode(Some(zenoh::config::WhatAmI::Router))
            .unwrap();

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

        config
            .listeners
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

        config
            .set_add_timestamp(Some(!args.is_present("no-timestamp")))
            .unwrap();
        config
            .scouting
            .multicast
            .set_enable(Some(!args.is_present("no-multicast-scouting")))
            .unwrap();

        for json in args.values_of("json").unwrap_or_default() {
            if let Some((key, value)) = json.split_once(':') {
                if let Err(e) = config.insert(key, &mut serde_json::Deserializer::from_str(value)) {
                    log::warn!("Couldn't perform configuration {}: {}", json, e)
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
