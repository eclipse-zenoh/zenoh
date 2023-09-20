//
// Copyright (c) 2023 ZettaScale Technology
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
use async_std::prelude::FutureExt;
use async_std::task;
use std::any::Any;
use std::convert::TryFrom;
use std::fmt::Write as _;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use zenoh_core::zasync_executor_init;
use zenoh_link::Link;
use zenoh_protocol::network::NetworkBody;
use zenoh_protocol::{
    core::{CongestionControl, Encoding, EndPoint, Priority, WhatAmI, ZenohId},
    network::{
        push::{
            ext::{NodeIdType, QoSType},
            Push,
        },
        NetworkMessage,
    },
    zenoh::Put,
};
use zenoh_result::ZResult;
use zenoh_transport::{
    TransportEventHandler, TransportManager, TransportMulticast, TransportMulticastEventHandler,
    TransportPeer, TransportPeerEventHandler, TransportUnicast,
};

const TIMEOUT: Duration = Duration::from_secs(60);
const SLEEP: Duration = Duration::from_secs(1);
const SLEEP_COUNT: Duration = Duration::from_millis(10);

const MSG_COUNT: usize = 100;
const MSG_SIZE_ALL: [usize; 2] = [1_024, 131_072];

const PRIORITY_ALL: [Priority; 8] = [
    Priority::Control,
    Priority::RealTime,
    Priority::InteractiveHigh,
    Priority::InteractiveLow,
    Priority::DataHigh,
    Priority::Data,
    Priority::DataLow,
    Priority::Background,
];

macro_rules! ztimeout {
    ($f:expr) => {
        $f.timeout(TIMEOUT).await.unwrap()
    };
}

// Transport Handler for the router
struct SHRouter {
    priority: Arc<AtomicUsize>,
    count: Arc<AtomicUsize>,
}

impl SHRouter {
    fn new() -> Self {
        Self {
            priority: Arc::new(AtomicUsize::new(0)),
            count: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn set_priority(&self, priority: Priority) {
        self.priority.store(priority as usize, Ordering::Relaxed)
    }

    fn reset_count(&self) {
        self.count.store(0, Ordering::Relaxed)
    }

    fn get_count(&self) -> usize {
        self.count.load(Ordering::Relaxed)
    }
}

impl TransportEventHandler for SHRouter {
    fn new_unicast(
        &self,
        _peer: TransportPeer,
        _transport: TransportUnicast,
    ) -> ZResult<Arc<dyn TransportPeerEventHandler>> {
        let arc = Arc::new(SCRouter::new(self.priority.clone(), self.count.clone()));
        Ok(arc)
    }

    fn new_multicast(
        &self,
        _transport: zenoh_transport::TransportMulticast,
    ) -> ZResult<Arc<dyn zenoh_transport::TransportMulticastEventHandler>> {
        panic!();
    }
}

// Transport Callback for the router
pub struct SCRouter {
    priority: Arc<AtomicUsize>,
    count: Arc<AtomicUsize>,
}

impl SCRouter {
    pub fn new(priority: Arc<AtomicUsize>, count: Arc<AtomicUsize>) -> Self {
        Self { priority, count }
    }
}

impl TransportPeerEventHandler for SCRouter {
    fn handle_message(&self, message: NetworkMessage) -> ZResult<()> {
        match &message.body {
            NetworkBody::Push(p) => {
                assert_eq!(
                    self.priority.load(Ordering::Relaxed),
                    p.ext_qos.get_priority() as usize
                );
            }
            _ => panic!("Unexpected message"),
        }
        self.count.fetch_add(1, Ordering::Relaxed);
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

// Transport Handler for the client
struct SHClient;

impl Default for SHClient {
    fn default() -> Self {
        Self
    }
}

impl TransportEventHandler for SHClient {
    fn new_unicast(
        &self,
        _peer: TransportPeer,
        _transport: TransportUnicast,
    ) -> ZResult<Arc<dyn TransportPeerEventHandler>> {
        Ok(Arc::new(SCClient))
    }

    fn new_multicast(
        &self,
        _transport: TransportMulticast,
    ) -> ZResult<Arc<dyn TransportMulticastEventHandler>> {
        panic!();
    }
}

// Transport Callback for the client
pub struct SCClient;

impl Default for SCClient {
    fn default() -> Self {
        Self
    }
}

impl TransportPeerEventHandler for SCClient {
    fn handle_message(&self, _message: NetworkMessage) -> ZResult<()> {
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

async fn open_transport_unicast(
    endpoints: &[EndPoint],
) -> (
    TransportManager,
    Arc<SHRouter>,
    TransportManager,
    TransportUnicast,
) {
    // Define client and router IDs
    let client_id = ZenohId::try_from([1]).unwrap();
    let router_id = ZenohId::try_from([2]).unwrap();

    // Create the router transport manager
    let router_handler = Arc::new(SHRouter::new());
    let router_manager = TransportManager::builder()
        .whatami(WhatAmI::Router)
        .zid(router_id)
        .build(router_handler.clone())
        .unwrap();

    // Create the client transport manager
    let client_manager = TransportManager::builder()
        .whatami(WhatAmI::Client)
        .zid(client_id)
        .build(Arc::new(SHClient))
        .unwrap();

    // Create the listener on the router
    for e in endpoints.iter() {
        println!("Add locator: {e}");
        let _ = ztimeout!(router_manager.add_listener(e.clone())).unwrap();
    }

    // Create an empty transport with the client
    // Open transport -> This should be accepted
    for e in endpoints.iter() {
        println!("Opening transport with {e}");
        let _ = ztimeout!(client_manager.open_transport_unicast(e.clone())).unwrap();
    }

    let client_transport = client_manager
        .get_transport_unicast(&router_id)
        .await
        .unwrap();

    // Return the handlers
    (
        router_manager,
        router_handler,
        client_manager,
        client_transport,
    )
}

async fn close_transport(
    router_manager: TransportManager,
    client_manager: TransportManager,
    client_transport: TransportUnicast,
    endpoints: &[EndPoint],
) {
    // Close the client transport
    let mut ee = String::new();
    for e in endpoints.iter() {
        let _ = write!(ee, "{e} ");
    }
    println!("Closing transport with {ee}");
    ztimeout!(client_transport.close()).unwrap();

    ztimeout!(async {
        while !router_manager.get_transports_unicast().await.is_empty() {
            task::sleep(SLEEP).await;
        }
    });

    // Stop the locators on the manager
    for e in endpoints.iter() {
        println!("Del locator: {e}");
        ztimeout!(router_manager.del_listener(e)).unwrap();
    }

    // Wait a little bit
    task::sleep(SLEEP).await;

    ztimeout!(router_manager.close());
    ztimeout!(client_manager.close());

    // Wait a little bit
    task::sleep(SLEEP).await;
}

async fn single_run(router_handler: Arc<SHRouter>, client_transport: TransportUnicast) {
    for p in PRIORITY_ALL.iter() {
        for ms in MSG_SIZE_ALL.iter() {
            // Reset the counter and set priority on the router
            router_handler.reset_count();
            router_handler.set_priority(*p);

            // Create the message to send
            let message: NetworkMessage = Push {
                wire_expr: "test".into(),
                ext_qos: QoSType::new(*p, CongestionControl::Block, false),
                ext_tstamp: None,
                ext_nodeid: NodeIdType::default(),
                payload: Put {
                    payload: vec![0u8; *ms].into(),
                    timestamp: None,
                    encoding: Encoding::default(),
                    ext_sinfo: None,
                    #[cfg(feature = "shared-memory")]
                    ext_shm: None,
                    ext_unknown: vec![],
                }
                .into(),
            }
            .into();

            println!("Sending {MSG_COUNT} messages... {p:?} {ms}");
            for _ in 0..MSG_COUNT {
                client_transport.schedule(message.clone()).unwrap();
            }

            // Wait for the messages to arrive to the other side
            ztimeout!(async {
                while router_handler.get_count() != MSG_COUNT {
                    task::sleep(SLEEP_COUNT).await;
                }
            });
        }
    }

    // Wait a little bit
    task::sleep(SLEEP).await;
}

async fn run(endpoints: &[EndPoint]) {
    let (router_manager, router_handler, client_manager, client_transport) =
        open_transport_unicast(endpoints).await;
    single_run(router_handler.clone(), client_transport.clone()).await;
    close_transport(router_manager, client_manager, client_transport, endpoints).await;
}

#[cfg(feature = "transport_tcp")]
#[test]
fn priorities_tcp_only() {
    let _ = env_logger::try_init();
    task::block_on(async {
        zasync_executor_init!();
    });
    // Define the locators
    let endpoints: Vec<EndPoint> = vec![format!("tcp/127.0.0.1:{}", 10000).parse().unwrap()];
    // Run
    task::block_on(run(&endpoints));
}

#[cfg(feature = "transport_unixpipe")]
#[test]
#[ignore]
fn conduits_unixpipe_only() {
    let _ = env_logger::try_init();
    task::block_on(async {
        zasync_executor_init!();
    });
    // Define the locators
    let endpoints: Vec<EndPoint> = vec!["unixpipe/conduits_unixpipe_only"
        .to_string()
        .parse()
        .unwrap()];
    // Run
    task::block_on(run(&endpoints));
}

#[cfg(feature = "transport_ws")]
#[test]
fn priorities_ws_only() {
    let _ = env_logger::try_init();
    task::block_on(async {
        zasync_executor_init!();
    });
    // Define the locators
    let endpoints: Vec<EndPoint> = vec![format!("ws/127.0.0.1:{}", 10010).parse().unwrap()];
    // Run
    task::block_on(run(&endpoints));
}
