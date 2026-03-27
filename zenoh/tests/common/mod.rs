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

use zenoh::Session;
use zenoh_core::ztimeout;
use zenoh_link::EndPoint;

const TIMEOUT: Duration = Duration::from_secs(60);

pub async fn open_session_listen(endpoints: &[&str]) -> Session {
    let mut config = zenoh_config::Config::default();
    config
        .listen
        .endpoints
        .set(
            endpoints
                .iter()
                .map(|e| e.parse().unwrap())
                .collect::<Vec<_>>(),
        )
        .unwrap();
    config.scouting.multicast.set_enabled(Some(false)).unwrap();
    ztimeout!(zenoh::open(config)).unwrap()
}

pub async fn open_session_connect(endpoints: &[&str]) -> Session {
    let mut config = zenoh_config::Config::default();
    config
        .connect
        .endpoints
        .set(
            endpoints
                .iter()
                .map(|e| e.parse().unwrap())
                .collect::<Vec<_>>(),
        )
        .unwrap();
    config.scouting.multicast.set_enabled(Some(false)).unwrap();
    ztimeout!(zenoh::open(config)).unwrap()
}

pub async fn open_session_unicast(endpoints: &[&str]) -> (Session, Session) {
    println!("[  ][01a] Opening peer01 session: {endpoints:?}");
    let peer01 = open_session_listen(endpoints).await;
    println!("[  ][02a] Opening peer02 session: {endpoints:?}");
    let peer02 = open_session_connect(endpoints).await;
    (peer01, peer02)
}

#[allow(dead_code)]
pub async fn open_session_multicast(endpoint01: &str, endpoint02: &str) -> (Session, Session) {
    println!("[  ][01a] Opening peer01 session: {endpoint01}");
    let peer01 = open_session_listen(&[endpoint01]).await;
    println!("[  ][02a] Opening peer02 session: {endpoint02}");
    let peer02 = open_session_listen(&[endpoint02]).await;
    (peer01, peer02)
}

pub async fn close_session(peer01: Session, peer02: Session) {
    println!("[  ][01d] Closing peer01 session");
    ztimeout!(peer01.close()).unwrap();
    println!("[  ][02d] Closing peer02 session");
    ztimeout!(peer02.close()).unwrap();
}

/// Open sessions configured for multilink (max_links > 1)
#[allow(dead_code)]
pub async fn open_session_multilink(
    listen_endpoints: &[&str],
    connect_endpoints: &[&str],
) -> (Session, Session) {
    // Session1: listener with multilink enabled
    let mut config1 = zenoh_config::Config::default();
    config1
        .listen
        .endpoints
        .set(
            listen_endpoints
                .iter()
                .map(|e| e.parse().unwrap())
                .collect::<Vec<_>>(),
        )
        .unwrap();
    config1.scouting.multicast.set_enabled(Some(false)).unwrap();
    config1
        .transport
        .unicast
        .set_max_links(listen_endpoints.len())
        .unwrap();
    let session1 = ztimeout!(zenoh::open(config1)).unwrap();

    // Session2: connector with multilink enabled
    let mut config2 = zenoh_config::Config::default();
    config2
        .connect
        .endpoints
        .set(
            connect_endpoints
                .iter()
                .map(|e| e.parse().unwrap())
                .collect::<Vec<_>>(),
        )
        .unwrap();
    config2.scouting.multicast.set_enabled(Some(false)).unwrap();
    config2
        .transport
        .unicast
        .set_max_links(connect_endpoints.len())
        .unwrap();
    let session2 = ztimeout!(zenoh::open(config2)).unwrap();

    (session1, session2)
}

fn get_available_endpoints(num: usize) -> Vec<EndPoint> {
    let ports = (0..num)
        .map(|_| {
            let listener =
                std::net::TcpListener::bind("127.0.0.1:0").expect("Failed to bind port 0");
            listener.local_addr().unwrap().port()
        })
        .collect::<Vec<_>>();

    ports
        .iter()
        .map(|p| format!("tcp/127.0.0.1:{}", p).parse::<EndPoint>().unwrap())
        .collect::<Vec<_>>()
}

pub enum ConfigType {
    Listener,
    Connector,
}

pub struct TestScenarioBuilder {
    link_num: usize,
    listener_config: zenoh_config::Config,
    connector_config: zenoh_config::Config,
}

impl TestScenarioBuilder {
    pub fn new() -> TestScenarioBuilder {
        let mut listener_config = zenoh_config::Config::default();
        let mut connector_config = zenoh_config::Config::default();

        // Disable multicast by default
        listener_config
            .scouting
            .multicast
            .set_enabled(Some(false))
            .unwrap();
        connector_config
            .scouting
            .multicast
            .set_enabled(Some(false))
            .unwrap();

        TestScenarioBuilder {
            link_num: 1,
            listener_config,
            connector_config,
        }
    }

    pub fn with_multilink(mut self, link_num: usize) -> TestScenarioBuilder {
        self.link_num = link_num;
        self.listener_config
            .transport
            .unicast
            .set_max_links(link_num)
            .unwrap();
        self.connector_config
            .transport
            .unicast
            .set_max_links(link_num)
            .unwrap();
        self
    }

    pub fn with_config(
        mut self,
        config_type: ConfigType,
        config: zenoh_config::Config,
    ) -> TestScenarioBuilder {
        match config_type {
            ConfigType::Listener => self.listener_config = config,
            ConfigType::Connector => self.connector_config = config,
        }
        self
    }

    pub async fn build(mut self) -> TestScenario {
        let endpoints = get_available_endpoints(self.link_num);

        // Create listener session
        self.listener_config
            .listen
            .endpoints
            .set(endpoints.clone())
            .unwrap();

        // Create connector session
        self.connector_config
            .connect
            .endpoints
            .set(endpoints)
            .unwrap();

        TestScenario {
            listener_config: self.listener_config,
            connector_config: self.connector_config,
            listener_session: None,
            connector_sessions: vec![],
        }
    }
}

pub struct TestScenario {
    listener_config: zenoh_config::Config,
    connector_config: zenoh_config::Config,
    listener_session: Option<Session>,
    connector_sessions: Vec<Session>,
}

impl TestScenario {
    //pub fn get_session(&self) -> (Session, Session) {
    //    (
    //        self.listener_session.clone(),
    //        self.connector_session.clone(),
    //    )
    //}

    pub async fn open_listener(&mut self) -> Session {
        let session = ztimeout!(zenoh::open(self.listener_config.clone())).unwrap();
        self.listener_session = Some(session.clone());
        session
    }

    pub async fn open_connector(&mut self) -> Session {
        let session = ztimeout!(zenoh::open(self.connector_config.clone())).unwrap();
        self.connector_sessions.push(session.clone());
        session
    }

    pub async fn open_pairs(&mut self) -> (Session, Session) {
        let listener_session = self.open_listener().await;
        let connector_session = self.open_connector().await;
        (listener_session, connector_session)
    }

    pub async fn close(&mut self) {
        // Close all the connector sessions
        for session in self.connector_sessions.drain(..) {
            println!("Closing connector session");
            ztimeout!(session.close()).unwrap();
        }

        // If self.listener_session is not None, close it and set it to None
        if let Some(session) = self.listener_session.take() {
            println!("Closing listener session");
            ztimeout!(session.close()).unwrap();
        }
    }
}
