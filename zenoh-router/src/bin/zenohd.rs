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
use std::env::consts::{DLL_PREFIX, DLL_SUFFIX};
use async_std::future;
use async_std::task;
use clap::{App, Arg};
use zenoh_protocol::link::Locator;
use zenoh_protocol::proto::whatami;
use zenoh_router::plugins::PluginsMgr;
use zenoh_router::runtime::{ AdminSpace, Runtime };


fn main() {
    task::block_on(async {
        env_logger::init();

        let app = App::new("The zenoh router")
        .arg(Arg::from_usage("-l, --locator=[LOCATOR] \
            'The locator this router will bind to.'")
            .default_value("tcp/127.0.0.1:7447"))
        .arg(Arg::from_usage("-p, --peer=[LOCATOR]... \
            'A peer locator this router will try to connect to. \
            Repeat this option to connect to several peers.'"));

        log::debug!("Load plugins...");
        let mut plugins_mgr = PluginsMgr::new();
        plugins_mgr.search_and_load_plugins(&format!("{}zplugin_" ,DLL_PREFIX), DLL_SUFFIX).await;
        let args = app.args(&plugins_mgr.get_plugins_args()).get_matches();

        let runtime = Runtime::new(0, whatami::BROKER);

        let self_locator: Locator = args.value_of("locator").unwrap().parse().unwrap();
        log::trace!("self_locator: {}", self_locator);

        {
            let orchestrator = &mut runtime.write().await.orchestrator;

            if let Err(_err) = orchestrator.add_acceptor(&self_locator).await {
                log::error!("Unable to open listening {}!", self_locator);
                std::process::exit(-1);
            }

            if  args.occurrences_of("peer") > 0 {
                log::debug!("Peers: {:?}", args.values_of("peer").unwrap().collect::<Vec<&str>>());
                for locator in args.values_of("peer").unwrap() {
                    orchestrator.add_peer(&locator.parse().unwrap()).await;
                }
            }
            // release runtime.write() lock for plugins to use Runtime
        }
        
        log::debug!("Start plugins...");
        plugins_mgr.start_plugins(&runtime, &args).await;

        AdminSpace::start(&runtime, plugins_mgr).await;

        future::pending::<()>().await;
    });
}