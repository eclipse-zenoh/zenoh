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

//! Shared test utilities for the Zenoh workspace.
//!
//! This crate provides [`TestSessions`], a helper that manages the lifecycle of
//! Zenoh sessions used in integration tests:
//!
//! * Dynamic port allocation (`tcp/127.0.0.1:0`) to avoid port collisions during
//!   parallel test execution.
//! * Automatic locator resolution after a listener binds to port `0`.
//! * Deterministic teardown — connectors are closed before listeners.
//!
//! # Quick start
//!
//! ```rust,ignore
//! use zenoh_test::TestSessions;
//!
//! let mut test_sessions = TestSessions::new();
//! let (listener, connector) = test_sessions.open_pairs().await;
//! // … run assertions …
//! test_sessions.close().await;
//! ```

use std::{
    net::{TcpListener, UdpSocket},
    time::Duration,
};

#[cfg(feature = "internal")]
use zenoh::internal::runtime::{Runtime, RuntimeBuilder};
use zenoh::{Session, Wait};
use zenoh_config::{ModeDependentValue, WhatAmI};
use zenoh_core::ztimeout;
use zenoh_link::EndPoint;

/// Default timeout applied to async operations via [`ztimeout!`].
pub const TIMEOUT: Duration = Duration::from_secs(60);

/// Binds to a random TCP port on loopback and returns the assigned port number.
///
/// The socket is dropped (and the port released) before the caller can use it,
/// so there is a tiny theoretical race.  In practice this is negligible on
/// localhost because tests bind immediately after calling this function.
pub fn get_free_tcp_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

/// Binds to a random UDP port on loopback and returns the assigned port number.
///
/// The socket is dropped (and the port released) before the caller can use it,
/// so there is a tiny theoretical race. In practice this is negligible on
/// localhost because tests bind immediately after calling this function.
pub fn get_free_udp_port() -> u16 {
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    socket.local_addr().unwrap().port()
}

/// Returns the first TCP [`EndPoint`] exposed by `session`.
///
/// This is a convenience wrapper around [`get_locators_from_session`] that
/// filters for the `tcp/` protocol — handy when a listener was opened with
/// `tcp/127.0.0.1:0` and you need the resolved address for a connector.
pub async fn get_tcp_locator(session: &Session) -> EndPoint {
    get_locators_from_session(session)
        .await
        .into_iter()
        .find(|ep| ep.to_string().starts_with("tcp/"))
        .expect("Expected a TCP listener endpoint from session")
}

/// Queries `session` for its current locators (async version).
pub async fn get_locators_from_session(session: &Session) -> Vec<EndPoint> {
    session
        .info()
        .locators()
        .await
        .into_iter()
        .map(|l| l.to_endpoint())
        .collect()
}

/// Queries `session` for its current locators (blocking version).
pub fn get_locators_from_session_sync(session: &Session) -> Vec<EndPoint> {
    session
        .info()
        .locators()
        .wait()
        .into_iter()
        .map(|l| l.to_endpoint())
        .collect()
}

/// Closes two sessions in order, printing progress to stdout.
///
/// This is a standalone helper kept for backward compatibility with tests that
/// manage sessions outside of [`TestSessions`].  Prefer
/// [`TestSessions::close`] for new code.
pub async fn close_session(peer01: Session, peer02: Session) {
    println!("[  ][01d] Closing peer01 session");
    ztimeout!(peer01.close()).unwrap();
    println!("[  ][02d] Closing peer02 session");
    ztimeout!(peer02.close()).unwrap();
}

/// Manages session lifecycle for integration tests.
///
/// `TestSessions` keeps track of every listener and connector it opens so that
/// [`close`](Self::close) can tear them down in the right order (connectors
/// first, then listeners).
///
/// # Locator tracking
///
/// Each call to [`open_listener`](Self::open_listener) or
/// [`open_listener_with_cfg`](Self::open_listener_with_cfg) **replaces** the
/// internally stored locators with the ones from the newly opened listener.
/// If you need locators from a previously opened listener, call
/// [`get_locators_from_session`] directly.
pub struct TestSessions {
    locators: Vec<EndPoint>,
    listener_sessions: Vec<Session>,
    connector_sessions: Vec<Session>,
}

impl Default for TestSessions {
    fn default() -> Self {
        Self::new()
    }
}

impl TestSessions {
    /// Creates an empty `TestSessions` with no sessions and no stored locators.
    pub fn new() -> Self {
        TestSessions {
            locators: vec![],
            listener_sessions: vec![],
            connector_sessions: vec![],
        }
    }

    /// Builds a listener [`Config`](zenoh_config::Config) that listens on
    /// `link_num` copies of `endpoint` (typically `"tcp/127.0.0.1:0"`).
    ///
    /// Multicast scouting is disabled and `max_links` is set to `link_num`.
    pub fn get_listener_config(&self, endpoint: &str, link_num: usize) -> zenoh_config::Config {
        let endpoints: Vec<EndPoint> = (0..link_num).map(|_| endpoint.parse().unwrap()).collect();

        let mut config = zenoh_config::Config::default();
        config.scouting.multicast.set_enabled(Some(false)).unwrap();
        config.listen.endpoints.set(endpoints).unwrap();
        config.transport.unicast.set_max_links(link_num).unwrap();

        config
    }

    /// Builds a connector [`Config`](zenoh_config::Config) that connects to
    /// the given `locators`.
    ///
    /// Multicast scouting is disabled and `max_links` matches the number of
    /// locators.
    pub fn get_connector_config_with_endpoint(
        &self,
        locators: Vec<EndPoint>,
    ) -> zenoh_config::Config {
        println!("Connecting to {:?}", locators);
        let mut config = zenoh_config::Config::default();
        config.scouting.multicast.set_enabled(Some(false)).unwrap();
        config
            .transport
            .unicast
            .set_max_links(locators.len())
            .unwrap();
        config
            .connect
            .set_endpoints(ModeDependentValue::Unique(locators))
            .unwrap();

        config
    }

    /// Builds a connector config using the internally stored locators
    /// (set by the most recent [`open_listener`](Self::open_listener) call).
    pub fn get_connector_config(&self) -> zenoh_config::Config {
        self.get_connector_config_with_endpoint(self.locators.clone())
    }

    /// Returns a clone of the internally stored locators.
    pub fn locators(&self) -> Vec<EndPoint> {
        self.locators.clone()
    }

    /// Opens a listener session with the given config, stores it, and updates
    /// the internal locators.
    pub async fn open_listener_with_cfg(&mut self, config: zenoh_config::Config) -> Session {
        let session = ztimeout!(zenoh::open(config)).unwrap();
        let locators = get_locators_from_session(&session).await;

        println!("Listening to {:?}", locators);
        self.listener_sessions.push(session.clone());
        self.locators = locators;

        session
    }

    /// Opens a listener session with the given config (blocking version).
    pub fn open_listener_with_cfg_sync(&mut self, config: zenoh_config::Config) -> Session {
        let session = zenoh::open(config).wait().unwrap();
        let locators = get_locators_from_session_sync(&session);

        println!("Listening to {:?}", locators);
        self.listener_sessions.push(session.clone());
        self.locators = locators;

        session
    }

    /// Opens a TCP listener on a random port (`tcp/127.0.0.1:0`).
    pub async fn open_listener(&mut self) -> Session {
        let config = self.get_listener_config("tcp/127.0.0.1:0", 1);
        self.open_listener_with_cfg(config).await
    }

    /// Opens a connector session with the given config and tracks it for
    /// later teardown.
    pub async fn open_connector_with_cfg(&mut self, config: zenoh_config::Config) -> Session {
        let session = ztimeout!(zenoh::open(config)).unwrap();
        self.connector_sessions.push(session.clone());

        session
    }

    /// Opens a connector session with the given config (blocking version).
    pub fn open_connector_with_cfg_sync(&mut self, config: zenoh_config::Config) -> Session {
        let session = zenoh::open(config).wait().unwrap();
        self.connector_sessions.push(session.clone());

        session
    }

    /// Opens a connector that connects to the internally stored locators.
    pub async fn open_connector(&mut self) -> Session {
        let config = self.get_connector_config();
        self.open_connector_with_cfg(config).await
    }

    /// Opens a listener + connector pair on a random TCP port.
    ///
    /// Returns `(listener_session, connector_session)`.
    pub async fn open_pairs(&mut self) -> (Session, Session) {
        let listener_session = self.open_listener().await;
        let connector_session = self.open_connector().await;
        (listener_session, connector_session)
    }

    /// Opens a listener + client-mode connector pair on a random TCP port.
    ///
    /// Returns `(listener_session, client_session)`.
    pub async fn open_pairs_client(&mut self) -> (Session, Session) {
        let listener_session = self.open_listener().await;

        let mut config = self.get_connector_config();
        config.set_mode(Some(WhatAmI::Client)).unwrap();
        let connector_session = self.open_connector_with_cfg(config).await;

        (listener_session, connector_session)
    }

    /// Opens two sessions joined via multicast on `endpoint` (e.g.
    /// `"udp/224.0.0.1:0"`).
    ///
    /// The first session binds to port `0`; the second session then binds to
    /// the resolved port so both share the same multicast group.
    pub async fn open_pairs_multicast(&mut self, endpoint: &str) -> (Session, Session) {
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
        let config = self.get_listener_config(&locator, 1);
        let session02 = ztimeout!(zenoh::open(config)).unwrap();
        self.listener_sessions.push(session02.clone());

        (session01, session02)
    }

    /// Opens a listener + connector pair at the **runtime** level.
    ///
    /// Useful for tests that need direct access to [`Runtime`] instead of
    /// [`Session`].
    #[cfg(feature = "internal")]
    pub async fn open_pairs_runtime(&mut self) -> (Runtime, Runtime) {
        let config = self.get_listener_config("tcp/127.0.0.1:0", 1);
        let mut listener_runtime = RuntimeBuilder::new(config.into()).build().await.unwrap();
        listener_runtime.start().await.unwrap();

        let locators = listener_runtime
            .get_locators()
            .into_iter()
            .map(|l| l.to_endpoint())
            .collect();

        let config = self.get_connector_config_with_endpoint(locators);
        let mut connector_runtime = RuntimeBuilder::new(config.into()).build().await.unwrap();
        connector_runtime.start().await.unwrap();

        (listener_runtime, connector_runtime)
    }

    /// Closes all tracked sessions: connectors first, then listeners.
    pub async fn close(&mut self) {
        for session in self.connector_sessions.drain(..) {
            println!("Closing connector session");
            ztimeout!(session.close()).unwrap();
        }

        for session in self.listener_sessions.drain(..) {
            println!("Closing listener session");
            ztimeout!(session.close()).unwrap();
        }
    }

    /// Closes all tracked sessions (blocking version).
    pub fn close_sync(&mut self) {
        for session in self.connector_sessions.drain(..) {
            println!("Closing connector session");
            session.close().wait().unwrap();
        }

        for session in self.listener_sessions.drain(..) {
            println!("Closing listener session");
            session.close().wait().unwrap();
        }
    }
}
