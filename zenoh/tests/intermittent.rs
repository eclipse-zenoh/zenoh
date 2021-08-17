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
use async_std::prelude::*;
use async_std::sync::Arc;
use async_std::task;
use std::any::Any;
use std::io::Write;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;
use zenoh::net::link::{Link, Locator, LocatorProperty};
use zenoh::net::protocol::core::{
    whatami, Channel, CongestionControl, PeerId, Priority, Reliability, ResKey,
};
use zenoh::net::protocol::io::ZBuf;
use zenoh::net::protocol::proto::ZenohMessage;
use zenoh::net::transport::{
    DummySessionEventHandler, Session, SessionEventHandler, SessionHandler, SessionManager,
    SessionManagerConfig, SessionManagerConfigUnicast,
};
use zenoh_util::core::ZResult;
use zenoh_util::zasync_executor_init;

const MSG_SIZE: usize = 8;
const MSG_COUNT: usize = 100_000;
const TIMEOUT: Duration = Duration::from_secs(300);
const SLEEP: Duration = Duration::from_millis(100);
const USLEEP: Duration = Duration::from_millis(1);

#[cfg(test)]
struct SHRouterIntermittent;

impl SHRouterIntermittent {
    fn new() -> Self {
        Self
    }
}

impl SessionHandler for SHRouterIntermittent {
    fn new_session(&self, _session: Session) -> ZResult<Arc<dyn SessionEventHandler>> {
        Ok(Arc::new(DummySessionEventHandler::default()))
    }
}

// Session Handler for the intermittent clients
struct SHClientIntermittent;

impl SHClientIntermittent {
    fn new() -> Self {
        Self
    }
}

impl SessionHandler for SHClientIntermittent {
    fn new_session(&self, _session: Session) -> ZResult<Arc<dyn SessionEventHandler>> {
        Ok(Arc::new(DummySessionEventHandler::default()))
    }
}

// Session Handler for the stable client
struct SHClientStable {
    counter: Arc<AtomicUsize>,
}

impl SHClientStable {
    fn new(counter: Arc<AtomicUsize>) -> Self {
        Self { counter }
    }
}

impl SessionHandler for SHClientStable {
    fn new_session(&self, _session: Session) -> ZResult<Arc<dyn SessionEventHandler>> {
        Ok(Arc::new(SCClient::new(self.counter.clone())))
    }
}

// Session Callback for the client
pub struct SCClient {
    counter: Arc<AtomicUsize>,
}

impl SCClient {
    pub fn new(counter: Arc<AtomicUsize>) -> Self {
        Self { counter }
    }
}

impl SessionEventHandler for SCClient {
    fn handle_message(&self, _message: ZenohMessage) -> ZResult<()> {
        self.counter.fetch_add(1, Ordering::AcqRel);
        Ok(())
    }

    fn new_link(&self, _link: Link) {}
    fn del_link(&self, _link: Link) {}
    fn closing(&self) {}
    fn closed(&self) {}

    fn as_any(&self) -> &dyn Any {
        self
    }
}

async fn session_intermittent(locator: Locator, locator_property: Option<Vec<LocatorProperty>>) {
    /* [ROUTER] */
    let router_id = PeerId::new(1, [0u8; PeerId::MAX_SIZE]);

    let router_handler = Arc::new(SHRouterIntermittent::new());
    // Create the router session manager
    let config = SessionManagerConfig::builder()
        .whatami(whatami::ROUTER)
        .pid(router_id.clone())
        .locator_property(locator_property.clone().unwrap_or_else(Vec::new))
        .unicast(
            SessionManagerConfigUnicast::builder()
                .max_sessions(3)
                .max_links(1)
                .build(),
        )
        .build(router_handler.clone());
    let router_manager = SessionManager::new(config);

    /* [CLIENT] */
    let client01_id = PeerId::new(1, [1u8; PeerId::MAX_SIZE]);
    let client02_id = PeerId::new(1, [2u8; PeerId::MAX_SIZE]);
    let client03_id = PeerId::new(1, [3u8; PeerId::MAX_SIZE]);

    // Create the transport session manager for the first client
    let counter = Arc::new(AtomicUsize::new(0));
    let config = SessionManagerConfig::builder()
        .whatami(whatami::CLIENT)
        .pid(client01_id.clone())
        .locator_property(locator_property.clone().unwrap_or_else(Vec::new))
        .unicast(
            SessionManagerConfigUnicast::builder()
                .max_sessions(1)
                .max_links(1)
                .build(),
        )
        .build(Arc::new(SHClientStable::new(counter.clone())));
    let client01_manager = SessionManager::new(config);

    // Create the transport session manager for the second client
    let config = SessionManagerConfig::builder()
        .whatami(whatami::CLIENT)
        .pid(client02_id.clone())
        .locator_property(locator_property.clone().unwrap_or_else(Vec::new))
        .unicast(
            SessionManagerConfigUnicast::builder()
                .max_sessions(1)
                .max_links(1)
                .build(),
        )
        .build(Arc::new(SHClientIntermittent::new()));
    let client02_manager = SessionManager::new(config);

    // Create the transport session manager for the third client
    let config = SessionManagerConfig::builder()
        .whatami(whatami::CLIENT)
        .pid(client03_id.clone())
        .locator_property(locator_property.unwrap_or_else(Vec::new))
        .unicast(
            SessionManagerConfigUnicast::builder()
                .max_sessions(1)
                .max_links(1)
                .build(),
        )
        .build(Arc::new(SHClientIntermittent::new()));
    let client03_manager = SessionManager::new(config);

    /* [1] */
    // Add a listener to the router
    println!("\nSession Intermittent [1a1]");
    let _ = router_manager.add_listener(&locator).await.unwrap();
    let locators = router_manager.get_listeners();
    println!("Session Intermittent [1a2]: {:?}", locators);
    assert_eq!(locators.len(), 1);

    /* [2] */
    // Open a session from client01 to the router
    let c_ses1 = client01_manager.open_session(&locator).await.unwrap();
    assert_eq!(c_ses1.get_links().unwrap().len(), 1);
    assert_eq!(client01_manager.get_sessions().len(), 1);
    assert_eq!(c_ses1.get_pid().unwrap(), router_id);

    /* [3] */
    // Continously open and close session from client02 and client03 to the router
    let c_client02_manager = client02_manager.clone();
    let c_locator = locator.clone();
    let c_router_id = router_id.clone();
    let c2_handle = task::spawn(async move {
        loop {
            print!("+");
            std::io::stdout().flush().unwrap();

            let c_ses2 = c_client02_manager.open_session(&c_locator).await.unwrap();
            assert_eq!(c_ses2.get_links().unwrap().len(), 1);
            assert_eq!(c_client02_manager.get_sessions().len(), 1);
            assert_eq!(c_ses2.get_pid().unwrap(), c_router_id);

            task::sleep(SLEEP).await;

            print!("-");
            std::io::stdout().flush().unwrap();

            c_ses2.close().timeout(TIMEOUT).await.unwrap().unwrap();

            task::sleep(SLEEP).await;
        }
    });

    let c_client03_manager = client03_manager.clone();
    let c_locator = locator.clone();
    let c_router_id = router_id.clone();
    let c3_handle = task::spawn(async move {
        loop {
            print!("*");
            std::io::stdout().flush().unwrap();

            let c_ses3 = c_client03_manager.open_session(&c_locator).await.unwrap();
            assert_eq!(c_ses3.get_links().unwrap().len(), 1);
            assert_eq!(c_client03_manager.get_sessions().len(), 1);
            assert_eq!(c_ses3.get_pid().unwrap(), c_router_id);

            task::sleep(SLEEP).await;

            print!("/");
            std::io::stdout().flush().unwrap();

            c_ses3.close().timeout(TIMEOUT).await.unwrap().unwrap();

            task::sleep(SLEEP).await;
        }
    });

    /* [4] */
    println!("Session Intermittent [4a1]");
    let c_router_manager = router_manager.clone();
    let res = task::spawn_blocking(move || {
        // Create the message to send
        let key = ResKey::RName("/test".to_string());
        let payload = ZBuf::from(vec![0u8; MSG_SIZE]);
        let channel = Channel {
            priority: Priority::default(),
            reliability: Reliability::Reliable,
        };
        let congestion_control = CongestionControl::Block;
        let data_info = None;
        let routing_context = None;
        let reply_context = None;
        let attachment = None;

        let message = ZenohMessage::make_data(
            key,
            payload,
            channel,
            congestion_control,
            data_info,
            routing_context,
            reply_context,
            attachment,
        );

        let mut ticks: Vec<usize> = (0..=MSG_COUNT).step_by(MSG_COUNT / 10).collect();
        ticks.remove(0);

        let mut count = 0;
        while count < MSG_COUNT {
            if count == ticks[0] {
                println!("\nScheduled {}", count);
                ticks.remove(0);
            }
            let sessions = c_router_manager.get_sessions();
            if !sessions.is_empty() {
                for s in sessions.iter() {
                    if let Ok(ll) = s.get_links() {
                        if ll.is_empty() {
                            print!("#");
                        } else {
                            assert_eq!(ll.len(), 1);
                        }
                    }
                    let res = s.schedule(message.clone());
                    if res.is_err() {
                        print!("X");
                        std::io::stdout().flush().unwrap();
                    }
                }
                count += 1;
            } else {
                print!("O");
                thread::sleep(USLEEP);
            }
        }
    })
    .timeout(TIMEOUT)
    .await
    .unwrap();
    // Stop the tasks
    c2_handle.cancel().await;
    c3_handle.cancel().await;
    println!("\nSession Intermittent [4a2]: {:?}", res);

    // Check that client01 received all the messages
    println!("Session Intermittent [4b1]");
    let check = async {
        loop {
            let c = counter.load(Ordering::Acquire);
            if c == MSG_COUNT {
                break;
            }
            println!("Session Intermittent [4b2]: Received {}/{}", c, MSG_COUNT);
            task::sleep(SLEEP).await;
        }
    };
    let res = check.timeout(TIMEOUT).await.unwrap();
    println!("Session Intermittent [4b3]: {:?}", res);

    /* [5] */
    // Close the open session on the client
    println!("Session Intermittent [5a1]");
    for s in client01_manager.get_sessions().iter() {
        s.close().timeout(TIMEOUT).await.unwrap().unwrap();
    }
    println!("Session Intermittent [5a2]");
    for s in client02_manager.get_sessions().iter() {
        s.close().timeout(TIMEOUT).await.unwrap().unwrap();
    }
    println!("Session Intermittent [5a3]");
    for s in client03_manager.get_sessions().iter() {
        s.close().timeout(TIMEOUT).await.unwrap().unwrap();
    }

    /* [6] */
    // Verify that the session has been closed also on the router
    println!("Session Intermittent [6a1]");
    let check = async {
        loop {
            let sessions = router_manager.get_sessions();
            if sessions.is_empty() {
                break;
            }
            task::sleep(SLEEP).await;
        }
    };
    let res = check.timeout(TIMEOUT).await.unwrap();
    println!("Session Intermittent [6a2]: {:?}", res);

    /* [7] */
    // Perform clean up of the open locators
    println!("\nSession Intermittent [7a1]");
    let res = router_manager.del_listener(&locator).await;
    println!("Session Intermittent [7a2]: {:?}", res);
    assert!(res.is_ok());

    task::sleep(SLEEP).await;
}

#[cfg(feature = "transport_tcp")]
#[test]
fn session_tcp_intermittent() {
    env_logger::init();

    task::block_on(async {
        zasync_executor_init!();
    });

    let locator = "tcp/127.0.0.1:12447".parse().unwrap();
    task::block_on(session_intermittent(locator, None));
}
