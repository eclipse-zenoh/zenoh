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
use clap::{App, Arg, Values};
use git_version::git_version;
use zenoh_router::plugins::PluginsMgr;
use zenoh_router::runtime::{config, AdminSpace, Runtime};

const GIT_VERSION: &str = git_version!(prefix = "v");

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
        env_logger::init();

        let app = App::new("The zenoh router")
            .version(GIT_VERSION)
            .arg(
                Arg::from_usage(
                    "-l, --listener=[LOCATOR]... \
            'A locator on which this router will listen for incoming sessions. \
            Repeat this option to open several listeners.'",
                )
                .default_value("tcp/0.0.0.0:7447"),
            )
            .arg(Arg::from_usage(
                "-e, --peer=[LOCATOR]... \
            'A peer locator this router will try to connect to. \
            Repeat this option to connect to several peers.'",
            ))
            .arg(Arg::from_usage(
                "-i, --id=[hex_string]... \
            'The identifier (as an hexadecimal string - e.g.: 0A0B23...) that zenohd must use. \
            WARNING: this identifier must be unique in the system! \
            If not set, a random UUIDv4 will be used.'",
            ))
            .arg(Arg::from_usage(
                "-P, --plugin=[PATH_TO_PLUGIN]... \
             'A plugin that must be loaded. Repeat this option to load several plugins.'",
            ))
            .arg(Arg::from_usage(
                "--plugin-nolookup \
             'When set, zenohd will not look for plugins nor try to load any plugin except the \
             ones explicitely configured with -P or --plugin.'",
            ))
            .arg(Arg::from_usage(
                "--no-timestamp \
             'By default zenohd adds a HLC-generated Timestamp to each routed Data if there isn't already one. \
             This option desactivates this feature.'",
            ));

        let mut plugins_mgr = PluginsMgr::new();
        // Get specified plugins from command line
        let plugins = get_plugins_from_args();
        plugins_mgr.load_plugins(plugins);
        if !std::env::args().any(|arg| arg == "--plugin-nolookup") {
            plugins_mgr.search_and_load_plugins().await;
        }

        // Add plugins' expected args and parse command line
        let args = app.args(&plugins_mgr.get_plugins_args()).get_matches();

        let mut config = config::empty();
        config.push((config::ZN_MODE_KEY, b"router".to_vec()));
        for peer in args
            .values_of("peer")
            .or_else(|| Some(Values::default()))
            .unwrap()
        {
            config.push((config::ZN_PEER_KEY, peer.as_bytes().to_vec()));
        }
        for listener in args
            .values_of("listener")
            .or_else(|| Some(Values::default()))
            .unwrap()
        {
            config.push((config::ZN_LISTENER_KEY, listener.as_bytes().to_vec()));
        }
        config.push((
            config::ZN_ADD_TIMESTAMP_KEY,
            if std::env::args().any(|arg| arg == "--no-timestamp") {
                config::ZN_FALSE.to_vec()
            } else {
                config::ZN_TRUE.to_vec()
            },
        ));

        log::debug!("Config: {}", config::to_string(&config));

        let runtime = match Runtime::new(0, config, args.value_of("id")).await {
            Ok(runtime) => runtime,
            Err(e) => {
                println!("{}. Exiting...", e);
                std::process::exit(-1);
            }
        };

        plugins_mgr.start_plugins(&runtime, &args).await;

        AdminSpace::start(&runtime, plugins_mgr).await;

        future::pending::<()>().await;
    });
}
