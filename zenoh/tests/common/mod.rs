//
// Copyright (c) 2025 ZettaScale Technology
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
#![allow(dead_code)] // because every test doesn't use the whole common features
use std::time::Duration;

#[cfg(feature = "internal")]
use zenoh::internal::runtime::{Runtime, RuntimeBuilder};
use zenoh::Session;
use zenoh_config::{ModeDependentValue, WhatAmI};
use zenoh_core::ztimeout;
use zenoh_link::EndPoint;

const TIMEOUT: Duration = Duration::from_secs(60);

pub async fn close_session(peer01: Session, peer02: Session) {
    println!("[  ][01d] Closing peer01 session");
    ztimeout!(peer01.close()).unwrap();
    println!("[  ][02d] Closing peer02 session");
    ztimeout!(peer02.close()).unwrap();
}

pub struct TestSessions {
    locators: Vec<EndPoint>,
    listener_sessions: Vec<Session>,
    connector_sessions: Vec<Session>,
}

impl TestSessions {
    pub fn new() -> Self {
        TestSessions {
            locators: vec![],
            listener_sessions: vec![],
            connector_sessions: vec![],
        }
    }

    fn get_listener_config(&self, endpoint: &str, link_num: usize) -> zenoh_config::Config {
        let endpoints: Vec<EndPoint> = (0..link_num).map(|_| endpoint.parse().unwrap()).collect();

        let mut config = zenoh_config::Config::default();
        // Disable multicast by default
        config.scouting.multicast.set_enabled(Some(false)).unwrap();
        // Listen to port 0 to get a random port
        config.listen.endpoints.set(endpoints).unwrap();
        // Configure link_num
        config.transport.unicast.set_max_links(link_num).unwrap();

        config
    }

    fn get_connector_config_with_endpoint(&self, locators: Vec<EndPoint>) -> zenoh_config::Config {
        println!("Connecting to {:?}", locators);
        let mut config = zenoh_config::Config::default();
        // Disable multicast by default
        config.scouting.multicast.set_enabled(Some(false)).unwrap();
        // Configure link_num
        config
            .transport
            .unicast
            .set_max_links(locators.len())
            .unwrap();
        // Connect to the locator
        config
            .connect
            .set_endpoints(ModeDependentValue::Unique(locators))
            .unwrap();

        config
    }

    fn get_connector_config(&self) -> zenoh_config::Config {
        self.get_connector_config_with_endpoint(self.locators.clone())
    }

    pub async fn get_locators_from_session(session: &Session) -> Vec<EndPoint> {
        session
            .info()
            .locators()
            .await
            .into_iter()
            .map(|l| l.to_endpoint())
            .collect()
    }

    pub async fn open_listener_with_links(&mut self, link_num: usize) -> Session {
        let config = self.get_listener_config("tcp/127.0.0.1:0", link_num);

        let session = ztimeout!(zenoh::open(config)).unwrap();
        // Extract the actual tcp endpoint that session1 is listening on
        let locators = TestSessions::get_locators_from_session(&session).await;

        println!("Listening to {:?}", locators);
        // Store session and locator
        self.listener_sessions.push(session.clone());
        self.locators = locators;

        session
    }

    pub async fn open_listener(&mut self) -> Session {
        self.open_listener_with_links(1).await
    }

    pub async fn open_connector_with_cfg(&mut self, config: zenoh_config::Config) -> Session {
        let session = ztimeout!(zenoh::open(config)).unwrap();
        self.connector_sessions.push(session.clone());

        session
    }

    pub async fn open_connector(&mut self) -> Session {
        let config = self.get_connector_config();
        self.open_connector_with_cfg(config).await
    }

    pub async fn open_pairs(&mut self) -> (Session, Session) {
        let listener_session = self.open_listener().await;
        let connector_session = self.open_connector().await;
        (listener_session, connector_session)
    }

    pub async fn open_pairs_client(&mut self) -> (Session, Session) {
        let listener_session = self.open_listener().await;

        let mut config = self.get_connector_config();
        config.set_mode(Some(WhatAmI::Client)).unwrap();
        let connector_session = self.open_connector_with_cfg(config).await;

        (listener_session, connector_session)
    }

    pub async fn open_pairs_multicast(&mut self, endpoint: &str) -> (Session, Session) {
        // Open 1st listener with port 0
        let config = self.get_listener_config(endpoint, 1);
        let session01 = ztimeout!(zenoh::open(config.clone())).unwrap();
        self.listener_sessions.push(session01.clone());
        let locator = session01
            .info()
            .locators()
            .await
            .into_iter()
            .find(|l| l.protocol().as_str() == "udp")
            .expect("Expected at least one UDP locator")
            .to_string();
        println!("Connecting to {:?}", locator);
        // Open 2nd listener with port got from 1st listener
        let config = self.get_listener_config(&locator, 1);
        let session02 = ztimeout!(zenoh::open(config)).unwrap();
        self.listener_sessions.push(session02.clone());

        (session01, session02)
    }

    #[cfg(feature = "internal")]
    pub async fn open_pairs_runtime(&mut self) -> (Runtime, Runtime) {
        // Create listener runtime
        let config = self.get_listener_config("tcp/127.0.0.1:0", 1);
        let mut listener_runtime = RuntimeBuilder::new(config.into()).build().await.unwrap();
        listener_runtime.start().await.unwrap();

        // Extract the actual tcp endpoint that listener_runtime is listening on
        let locators = listener_runtime
            .get_locators()
            .into_iter()
            .map(|l| l.to_endpoint())
            .collect();

        // Create connector runtime
        let config = self.get_connector_config_with_endpoint(locators);
        let mut connector_runtime = RuntimeBuilder::new(config.into()).build().await.unwrap();
        connector_runtime.start().await.unwrap();

        (listener_runtime, connector_runtime)
    }

    pub async fn close(&mut self) {
        // Close all the connector sessions
        for session in self.connector_sessions.drain(..) {
            println!("Closing connector session");
            ztimeout!(session.close()).unwrap();
        }

        // Close all the listener sessions
        for session in self.listener_sessions.drain(..) {
            println!("Closing listener session");
            ztimeout!(session.close()).unwrap();
        }
    }
}
