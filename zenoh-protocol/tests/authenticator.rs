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
use async_std::sync::Arc;
use async_std::task;
use async_trait::async_trait;
use std::collections::HashMap;
use zenoh_protocol::core::{whatami, PeerId};
use zenoh_protocol::link::{Link, Locator};
use zenoh_protocol::proto::ZenohMessage;
use zenoh_protocol::session::authenticator::UserPasswordAuthenticator;
use zenoh_protocol::session::{
    DummySessionEventHandler, Session, SessionEventHandler, SessionHandler, SessionManager,
    SessionManagerConfig, SessionManagerOptionalConfig,
};
use zenoh_util::core::ZResult;

#[cfg(test)]
struct SHRouterAuthenticator;

impl SHRouterAuthenticator {
    fn new() -> Self {
        Self
    }
}

#[async_trait]
impl SessionHandler for SHRouterAuthenticator {
    async fn new_session(
        &self,
        _session: Session,
    ) -> ZResult<Arc<dyn SessionEventHandler + Send + Sync>> {
        Ok(Arc::new(MHRouterAuthenticator::new()))
    }
}

struct MHRouterAuthenticator;

impl MHRouterAuthenticator {
    fn new() -> Self {
        Self
    }
}

#[async_trait]
impl SessionEventHandler for MHRouterAuthenticator {
    async fn handle_message(&self, _msg: ZenohMessage) -> ZResult<()> {
        Ok(())
    }
    async fn new_link(&self, _link: Link) {}
    async fn del_link(&self, _link: Link) {}
    async fn closing(&self) {}
    async fn closed(&self) {}
}

// Session Handler for the client
struct SHClientAuthenticator {}

impl SHClientAuthenticator {
    fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl SessionHandler for SHClientAuthenticator {
    async fn new_session(
        &self,
        _session: Session,
    ) -> ZResult<Arc<dyn SessionEventHandler + Send + Sync>> {
        Ok(Arc::new(DummySessionEventHandler::new()))
    }
}

async fn authenticator_user_password(locator: Locator) {
    /* [CLIENT] */
    let client01_id = PeerId::new(1, [1u8; PeerId::MAX_SIZE]);
    let user01 = "user01".to_string();
    let password01 = "password01".to_string();

    let client02_id = PeerId::new(1, [2u8; PeerId::MAX_SIZE]);
    let user02 = "invalid".to_string();
    let password02 = "invalid".to_string();

    let client03_id = client01_id.clone();
    let user03 = "user03".to_string();
    let password03 = "password03".to_string();

    /* [ROUTER] */
    let router_id = PeerId::new(1, [0u8; PeerId::MAX_SIZE]);
    let router_handler = Arc::new(SHRouterAuthenticator::new());
    // Create the router session manager
    let config = SessionManagerConfig {
        version: 0,
        whatami: whatami::ROUTER,
        id: router_id.clone(),
        handler: router_handler.clone(),
    };

    let mut lookup: HashMap<Vec<u8>, Vec<u8>> = HashMap::new();
    lookup.insert(user01.clone().into(), password01.clone().into());
    lookup.insert(user03.clone().into(), password03.clone().into());

    let peer_authenticator_router = Arc::new(UserPasswordAuthenticator::new(
        lookup,
        ("foo".into(), "foo".into()),
    ));
    let opt_config = SessionManagerOptionalConfig {
        lease: None,
        keep_alive: None,
        sn_resolution: None,
        batch_size: None,
        timeout: None,
        retries: None,
        max_sessions: None,
        max_links: None,
        peer_authenticator: Some(vec![peer_authenticator_router.clone().into()]),
        link_authenticator: None,
        locator_property: None,
    };
    let router_manager = SessionManager::new(config, Some(opt_config));

    // Create the transport session manager for the first client
    let config = SessionManagerConfig {
        version: 0,
        whatami: whatami::CLIENT,
        id: client01_id.clone(),
        handler: Arc::new(SHClientAuthenticator::new()),
    };
    let lookup: HashMap<Vec<u8>, Vec<u8>> = HashMap::new();
    let peer_authenticator_client01 =
        UserPasswordAuthenticator::new(lookup, (user01.clone().into(), password01.clone().into()));
    let opt_config = SessionManagerOptionalConfig {
        lease: None,
        keep_alive: None,
        sn_resolution: None,
        batch_size: None,
        timeout: None,
        retries: None,
        max_sessions: None,
        max_links: None,
        peer_authenticator: Some(vec![peer_authenticator_client01.into()]),
        link_authenticator: None,
        locator_property: None,
    };
    let client01_manager = SessionManager::new(config, Some(opt_config));

    // Create the transport session manager for the second client
    let config = SessionManagerConfig {
        version: 0,
        whatami: whatami::CLIENT,
        id: client02_id.clone(),
        handler: Arc::new(SHClientAuthenticator::new()),
    };
    let lookup: HashMap<Vec<u8>, Vec<u8>> = HashMap::new();
    let peer_authenticator_client02 =
        UserPasswordAuthenticator::new(lookup, (user02.clone().into(), password02.clone().into()));
    let opt_config = SessionManagerOptionalConfig {
        lease: None,
        keep_alive: None,
        sn_resolution: None,
        batch_size: None,
        timeout: None,
        retries: None,
        max_sessions: None,
        max_links: None,
        peer_authenticator: Some(vec![peer_authenticator_client02.into()]),
        link_authenticator: None,
        locator_property: None,
    };
    let client02_manager = SessionManager::new(config, Some(opt_config));

    // Create the transport session manager for the third client
    let config = SessionManagerConfig {
        version: 0,
        whatami: whatami::CLIENT,
        id: client03_id.clone(),
        handler: Arc::new(SHClientAuthenticator::new()),
    };
    let lookup: HashMap<Vec<u8>, Vec<u8>> = HashMap::new();
    let peer_authenticator_client03 =
        UserPasswordAuthenticator::new(lookup, (user03.into(), password03.into()));
    let opt_config = SessionManagerOptionalConfig {
        lease: None,
        keep_alive: None,
        sn_resolution: None,
        batch_size: None,
        timeout: None,
        retries: None,
        max_sessions: None,
        max_links: None,
        peer_authenticator: Some(vec![peer_authenticator_client03.into()]),
        link_authenticator: None,
        locator_property: None,
    };
    let client03_manager = SessionManager::new(config, Some(opt_config));

    /* [1] */
    println!("\nSession Authenticator [1a1]");
    // Add the locator on the router
    let res = router_manager.add_listener(&locator).await;
    println!("Session Authenticator [1a1]: {:?}", res);
    assert!(res.is_ok());
    println!("Session Authenticator [1a2]");
    let locators = router_manager.get_listeners().await;
    println!("Session Authenticator [1a2]: {:?}", locators);
    assert_eq!(locators.len(), 1);

    /* [2] */
    // Open a first session from the client to the router
    // -> This should be accepted
    println!("Session Authenticator [2a1]");
    let res = client01_manager.open_session(&locator).await;
    println!("Session Authenticator [2a1]: {:?}", res);
    assert!(res.is_ok());
    let c_ses1 = res.unwrap();

    /* [3] */
    println!("Session Authenticator [3a1]");
    let res = c_ses1.close().await;
    println!("Session Authenticator [3a1]: {:?}", res);
    assert!(res.is_ok());

    /* [4] */
    // Open a second session from the client to the router
    // -> This should be rejected
    println!("Session Authenticator [4a1]");
    let res = client02_manager.open_session(&locator).await;
    println!("Session Authenticator [4a1]: {:?}", res);
    assert!(res.is_err());

    /* [5] */
    // Open a third session from the client to the router
    // -> This should be accepted
    println!("Session Authenticator [5a1]");
    let res = client01_manager.open_session(&locator).await;
    println!("Session Authenticator [5a1]: {:?}", res);
    assert!(res.is_ok());
    let c_ses1 = res.unwrap();

    /* [6] */
    // Add client02 credentials on the router
    let res = peer_authenticator_router
        .add_user(user02.into(), password02.into())
        .await;
    assert!(res.is_ok());
    // Open a fourth session from the client to the router
    // -> This should be accepted
    println!("Session Authenticator [6a1]");
    let res = client02_manager.open_session(&locator).await;
    println!("Session Authenticator [6a1]: {:?}", res);
    assert!(res.is_ok());
    let c_ses2 = res.unwrap();

    /* [7] */
    // Open a fourth session from the client to the router
    // -> This should be rejected
    println!("Session Authenticator [7a1]");
    let res = client03_manager.open_session(&locator).await;
    println!("Session Authenticator [7a1]: {:?}", res);
    assert!(res.is_err());

    /* [7] */
    println!("Session Authenticator [8a1]");
    let res = c_ses1.close().await;
    println!("Session Authenticator [8a1]: {:?}", res);
    assert!(res.is_ok());
    println!("Session Authenticator [8a2]");
    let res = c_ses2.close().await;
    println!("Session Authenticator [8a2]: {:?}", res);
    assert!(res.is_ok());
}

#[cfg(feature = "transport_tcp")]
#[test]
fn authenticator_tcp() {
    let locator: Locator = "tcp/127.0.0.1:7447".parse().unwrap();
    task::block_on(async {
        authenticator_user_password(locator).await;
    });
}

#[cfg(feature = "transport_udp")]
#[test]
fn authenticator_udp() {
    let locator: Locator = "udp/127.0.0.1:7447".parse().unwrap();
    task::block_on(async {
        authenticator_user_password(locator).await;
    });
}

#[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
#[test]
fn authenticator_unix() {
    let _ = std::fs::remove_file("zenoh-test-unix-socket-10.sock");
    let locator: Locator = "unixsock-stream/zenoh-test-unix-socket-10.sock"
        .parse()
        .unwrap();
    task::block_on(async {
        authenticator_user_password(locator).await;
    });
    let _ = std::fs::remove_file("zenoh-test-unix-socket-10.sock");
    let _ = std::fs::remove_file("zenoh-test-unix-socket-10.sock.lock");
}
