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
    
    task::block_on(async {
        let mut args = std::env::args();
        args.next(); // skip exe name
    
        let broker = Arc::new(Broker::new());

        let mut pid = vec![0, 0, 0, 0];
        rand::thread_rng().fill_bytes(&mut pid);
        debug!("Starting broker with PID: {:02x?}", pid);

        let batch_size: Option<usize> = match args.next() { 
            Some(size) => Some(size.parse().unwrap()),
            None => None
        };

        let self_locator: Locator = match args.next() { 
            Some(port) => {
                let mut s = "tcp/127.0.0.1:".to_string();
                s.push_str(&port);
                s.parse().unwrap()
            },
            None => "tcp/127.0.0.1:7447".parse().unwrap()
        };
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
            batch_size: batch_size,
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
        for locator in args {
            debug!("Open session to {}", locator);
            if let Err(_err) =  manager.open_session(&locator.parse().unwrap(), &attachment).await {
                log::error!("Unable to connect to {}!", locator);
                std::process::exit(-1);
            }
        }
        
        debug!("Load plugins...");
        let mut plugins_mgr = zenoh_util::plugins::PluginsMgr::new();
        plugins_mgr.search_and_load_plugins("zenoh-", ".plugin").await;

        future::pending::<()>().await;
    });
}