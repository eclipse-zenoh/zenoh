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
use clap::{App, Arg};
use async_std::future;
use async_std::sync::Arc;
use async_std::task;
use rand::RngCore;
use log::{debug, trace};
use zenoh_protocol::core::PeerId;
use zenoh_protocol::link::Locator;
use zenoh_protocol::proto::whatami;
use zenoh_protocol::session::{SessionManager, SessionManagerConfig, SessionManagerOptionalConfig};
use zenoh_router::routing::broker::Broker;

fn main() {
    env_logger::init();
    
    let app = App::new("The zenoh router")
        .arg(Arg::from_usage("-l [LOCATOR], --locator \
            'The locator this router will bind to.'")
            .default_value("tcp/127.0.0.1:7447"))
        .arg(Arg::from_usage("-p ... [LOCATOR] --peer \
            'A peer locator this router will try to connect to. \
            Repeat this option to connect to several peers.'"));

    task::block_on(async {
        debug!("Load plugins...");
        let mut plugins_mgr = zenoh_util::plugins::PluginsMgr::new();
        plugins_mgr.search_and_load_plugins("zenoh-", ".plugin").await;
        let args = app.args(&plugins_mgr.get_plugins_args()).get_matches();

        let broker = Arc::new(Broker::new());

        let mut pid = vec![0, 0, 0, 0];
        rand::thread_rng().fill_bytes(&mut pid);
        debug!("Starting broker with PID: {:02x?}", pid);

        let self_locator: Locator = args.value_of("locator").unwrap().parse().unwrap();
        debug!("self_locator: {}", self_locator);

        let config = SessionManagerConfig {
            version: 0,
            whatami: whatami::BROKER,
            id: PeerId{id: pid},
            handler: broker.clone()
        };
        let opt_config = SessionManagerOptionalConfig {
            lease: None,
            keep_alive: None,
            sn_resolution: None,
            batch_size: None,
            timeout: None,
            retries: None,
            max_sessions: None,
            max_links: None 
        };
        trace!("SessionManager::new()");
        let manager = SessionManager::new(config, Some(opt_config));

        trace!("SessionManager::add_locator({})", self_locator);
        if let Err(_err) = manager.add_locator(&self_locator).await {
            log::error!("Unable to open listening {}!", self_locator);
            std::process::exit(-1);
        }

        let attachment = None;
        if  args.occurrences_of("peer") > 0 {
            debug!("peers: {:?}", args.values_of("peer"));
            for locator in args.values_of("peer").unwrap() {
                debug!("Open session to {}", locator);
                if let Err(_err) =  manager.open_session(&locator.parse().unwrap(), &attachment).await {
                    log::error!("Unable to connect to {}!", locator);
                }
            }
        }
        
        debug!("Start plugins...");
        plugins_mgr.start_plugins(&args, &format!("{}", self_locator)).await;

        future::pending::<()>().await;
    });
}