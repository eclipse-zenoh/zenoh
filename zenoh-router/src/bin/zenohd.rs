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
use zenoh_protocol::proto::whatami;
use zenoh_router::plugins::PluginsMgr;
use zenoh_router::runtime::{ AdminSpace, Runtime };


fn main() {
    task::block_on(async {
        env_logger::init();

        let app = App::new("The zenoh router")
        .arg(Arg::from_usage("-l, --listener=[LOCATOR]... \
            'A locator on which this router will listen for incoming sessions. \
            Repeat this option to open several listeners.'")
            .default_value("tcp/0.0.0.0:7447"))
        .arg(Arg::from_usage("-e, --peer=[LOCATOR]... \
            'A peer locator this router will try to connect to. \
            Repeat this option to connect to several peers.'"));

        log::debug!("Load plugins...");
        let mut plugins_mgr = PluginsMgr::new();
        plugins_mgr.search_and_load_plugins(&format!("{}zplugin_" ,DLL_PREFIX), DLL_SUFFIX).await;
        let args = app.args(&plugins_mgr.get_plugins_args()).get_matches();
        let listeners = args.values_of("listener").map(|v| v.map(|l| l.parse().unwrap()).collect())
            .or_else(|| Some(vec![])).unwrap();
        let peers = args.values_of("peer").map(|v| v.map(|l| l.parse().unwrap()).collect())
            .or_else(|| Some(vec![])).unwrap();

        let runtime = match Runtime::new(0, whatami::BROKER, listeners, peers, "auto", std::time::Duration::new(0, 200_000_000)).await {
            Ok(runtime) => runtime,
            _ => std::process::exit(-1),
        };
        
        log::debug!("Start plugins...");
        plugins_mgr.start_plugins(&runtime, &args).await;

        AdminSpace::start(&runtime, plugins_mgr).await;

        future::pending::<()>().await;
    });
}