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
use zenoh::net::link::{EndPoint, Link};
use zenoh::net::protocol::core::{
    Channel, CongestionControl, Priority, Reliability, WhatAmI, ZenohId,
};
use zenoh::net::protocol::io::ZBuf;
use zenoh::net::protocol::message::ZenohMessage;
use zenoh::net::transport::{
    DummyTransportPeerEventHandler, TransportEventHandler, TransportManager, TransportMulticast,
    TransportMulticastEventHandler, TransportPeer, TransportPeerEventHandler, TransportUnicast,
};
use zenoh_util::core::Result as ZResult;
use zenoh_util::zasync_executor_init;

const MSG_SIZE: usize = 8;
const MSG_COUNT: usize = 100_000;
const TIMEOUT: Duration = Duration::from_secs(300);
const SLEEP: Duration = Duration::from_millis(100);
const USLEEP: Duration = Duration::from_millis(1);

macro_rules! ztimeout {
    ($f:expr) => {
        $f.timeout(TIMEOUT).await.unwrap()
    };
}
#[cfg(test)]
#[derive(Default)]
struct SHRouterIntermittent;

impl TransportEventHandler for SHRouterIntermittent {
    fn new_unicast(
        &self,
        _peer: TransportPeer,
        _transport: TransportUnicast,
    ) -> ZResult<Arc<dyn TransportPeerEventHandler>> {
        Ok(Arc::new(DummyTransportPeerEventHandler::default()))
    }

    fn new_multicast(
        &self,
        _transport: TransportMulticast,
    ) -> ZResult<Arc<dyn TransportMulticastEventHandler>> {
        panic!();
    }
}

// Transport Handler for the intermittent clients
#[derive(Default)]
struct SHClientIntermittent;

impl TransportEventHandler for SHClientIntermittent {
    fn new_unicast(
        &self,
        _peer: TransportPeer,
        _transport: TransportUnicast,
    ) -> ZResult<Arc<dyn TransportPeerEventHandler>> {
        Ok(Arc::new(DummyTransportPeerEventHandler::default()))
    }

    fn new_multicast(
        &self,
        _transport: TransportMulticast,
    ) -> ZResult<Arc<dyn TransportMulticastEventHandler>> {
        panic!();
    }
}

// Transport Handler for the stable client
struct SHClientStable {
    counter: Arc<AtomicUsize>,
}

impl SHClientStable {
    fn new(counter: Arc<AtomicUsize>) -> Self {
        Self { counter }
    }
}

impl TransportEventHandler for SHClientStable {
    fn new_unicast(
        &self,
        _peer: TransportPeer,
        _transport: TransportUnicast,
    ) -> ZResult<Arc<dyn TransportPeerEventHandler>> {
        Ok(Arc::new(SCClient::new(self.counter.clone())))
    }

    fn new_multicast(
        &self,
        _transport: TransportMulticast,
    ) -> ZResult<Arc<dyn TransportMulticastEventHandler>> {
        panic!();
    }
}

// Transport Callback for the client
pub struct SCClient {
    counter: Arc<AtomicUsize>,
}

impl SCClient {
    pub fn new(counter: Arc<AtomicUsize>) -> Self {
        Self { counter }
    }
}

impl TransportPeerEventHandler for SCClient {
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

async fn transport_intermittent(endpoint: &EndPoint) {
    /* [ROUTER] */
    let router_id = ZenohId::new(1, [0_u8; ZenohId::MAX_SIZE]);

    let router_handler = Arc::new(SHRouterIntermittent::default());
    // Create the router transport manager
    let unicast = TransportManager::config_unicast()
        .max_links(1)
        .max_sessions(3);
    let router_manager = TransportManager::builder()
        .whatami(WhatAmI::Router)
        .zid(router_id)
        .unicast(unicast)
        .build(router_handler.clone())
        .unwrap();

    /* [CLIENT] */
    let client01_id = ZenohId::new(1, [1_u8; ZenohId::MAX_SIZE]);
    let client02_id = ZenohId::new(1, [2_u8; ZenohId::MAX_SIZE]);
    let client03_id = ZenohId::new(1, [3_u8; ZenohId::MAX_SIZE]);

    // Create the transport transport manager for the first client
    let counter = Arc::new(AtomicUsize::new(0));
    let unicast = TransportManager::config_unicast()
        .max_links(1)
        .max_sessions(3);
    let client01_manager = TransportManager::builder()
        .whatami(WhatAmI::Client)
        .zid(client01_id)
        .unicast(unicast)
        .build(Arc::new(SHClientStable::new(counter.clone())))
        .unwrap();

    // Create the transport transport manager for the second client
    let unicast = TransportManager::config_unicast()
        .max_links(1)
        .max_sessions(1);
    let client02_manager = TransportManager::builder()
        .whatami(WhatAmI::Client)
        .zid(client02_id)
        .unicast(unicast)
        .build(Arc::new(SHClientIntermittent::default()))
        .unwrap();

    // Create the transport transport manager for the third client
    let unicast = TransportManager::config_unicast()
        .max_links(1)
        .max_sessions(1);
    let client03_manager = TransportManager::builder()
        .whatami(WhatAmI::Client)
        .zid(client03_id)
        .unicast(unicast)
        .build(Arc::new(SHClientIntermittent::default()))
        .unwrap();

    /* [1] */
    // Add a listener to the router
    println!("\nTransport Intermittent [1a1]");
    let _ = ztimeout!(router_manager.add_listener(endpoint.clone())).unwrap();
    let locators = router_manager.get_listeners();
    println!("Transport Intermittent [1a2]: {:?}", locators);
    assert_eq!(locators.len(), 1);

    /* [2] */
    // Open a transport from client01 to the router
    let c_ses1 = ztimeout!(client01_manager.open_transport(endpoint.clone())).unwrap();
    assert_eq!(c_ses1.get_links().unwrap().len(), 1);
    assert_eq!(client01_manager.get_transports().len(), 1);
    assert_eq!(c_ses1.get_zid().unwrap(), router_id);

    /* [3] */
    // Continously open and close transport from client02 and client03 to the router
    let c_client02_manager = client02_manager.clone();
    let c_endpoint = endpoint.clone();
    let c_router_id = router_id;
    let c2_handle = task::spawn(async move {
        loop {
            print!("+");
            std::io::stdout().flush().unwrap();

            let c_ses2 = ztimeout!(c_client02_manager.open_transport(c_endpoint.clone())).unwrap();
            assert_eq!(c_ses2.get_links().unwrap().len(), 1);
            assert_eq!(c_client02_manager.get_transports().len(), 1);
            assert_eq!(c_ses2.get_zid().unwrap(), c_router_id);

            task::sleep(SLEEP).await;

            print!("-");
            std::io::stdout().flush().unwrap();

            ztimeout!(c_ses2.close()).unwrap();

            task::sleep(SLEEP).await;
        }
    });

    let c_client03_manager = client03_manager.clone();
    let c_endpoint = endpoint.clone();
    let c_router_id = router_id;
    let c3_handle = task::spawn(async move {
        loop {
            print!("*");
            std::io::stdout().flush().unwrap();

            let c_ses3 = ztimeout!(c_client03_manager.open_transport(c_endpoint.clone())).unwrap();
            assert_eq!(c_ses3.get_links().unwrap().len(), 1);
            assert_eq!(c_client03_manager.get_transports().len(), 1);
            assert_eq!(c_ses3.get_zid().unwrap(), c_router_id);

            task::sleep(SLEEP).await;

            print!("/");
            std::io::stdout().flush().unwrap();

            ztimeout!(c_ses3.close()).unwrap();

            task::sleep(SLEEP).await;
        }
    });

    /* [4] */
    println!("Transport Intermittent [4a1]");
    let c_router_manager = router_manager.clone();
    let res = ztimeout!(task::spawn_blocking(move || {
        // Create the message to send
        let key = "/test".into();
        let payload = ZBuf::from(vec![0_u8; MSG_SIZE]);
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
            let transports = c_router_manager.get_transports();
            if !transports.is_empty() {
                for s in transports.iter() {
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
    }));

    // Stop the tasks
    ztimeout!(c2_handle.cancel());
    ztimeout!(c3_handle.cancel());
    println!("\nTransport Intermittent [4a2]: {:?}", res);

    // Check that client01 received all the messages
    println!("Transport Intermittent [4b1]");
    let res = ztimeout!(async {
        loop {
            let c = counter.load(Ordering::Acquire);
            if c == MSG_COUNT {
                break;
            }
            println!("Transport Intermittent [4b2]: Received {}/{}", c, MSG_COUNT);
            task::sleep(SLEEP).await;
        }
    });
    println!("Transport Intermittent [4b3]: {:?}", res);

    /* [5] */
    // Close the open transport on the client
    println!("Transport Intermittent [5a1]");
    for s in client01_manager.get_transports().iter() {
        ztimeout!(s.close()).unwrap();
    }
    println!("Transport Intermittent [5a2]");
    for s in client02_manager.get_transports().iter() {
        ztimeout!(s.close()).unwrap();
    }
    println!("Transport Intermittent [5a3]");
    for s in client03_manager.get_transports().iter() {
        ztimeout!(s.close()).unwrap();
    }

    /* [6] */
    // Verify that the transport has been closed also on the router
    println!("Transport Intermittent [6a1]");
    let res = ztimeout!(async {
        loop {
            let transports = router_manager.get_transports();
            if transports.is_empty() {
                break;
            }
            task::sleep(SLEEP).await;
        }
    });
    println!("Transport Intermittent [6a2]: {:?}", res);

    /* [7] */
    // Perform clean up of the open locators
    println!("\nTransport Intermittent [7a1]");
    let res = ztimeout!(router_manager.del_listener(endpoint));
    println!("Transport Intermittent [7a2]: {:?}", res);
    assert!(res.is_ok());

    // Wait a little bit
    task::sleep(SLEEP).await;

    ztimeout!(router_manager.close());
    ztimeout!(client01_manager.close());
    ztimeout!(client02_manager.close());
    ztimeout!(client03_manager.close());

    // Wait a little bit
    task::sleep(SLEEP).await;
}

#[cfg(feature = "transport_tcp")]
#[test]
fn transport_tcp_intermittent() {
    env_logger::init();

    task::block_on(async {
        zasync_executor_init!();
    });

    let endpoint: EndPoint = "tcp/127.0.0.1:12447".parse().unwrap();
    task::block_on(transport_intermittent(&endpoint));
}
