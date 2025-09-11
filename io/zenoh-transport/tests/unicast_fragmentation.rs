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
use std::{
    any::Any,
    convert::TryFrom,
    fmt::Write as _,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use lazy_static::lazy_static;
use zenoh_core::ztimeout;
use zenoh_link::Link;
use zenoh_protocol::{
    core::{CongestionControl, EndPoint, Priority, WhatAmI, ZenohIdProto},
    network::{push::ext::QoSType, NetworkMessage, NetworkMessageExt, NetworkMessageMut, Push},
};
use zenoh_result::ZResult;
use zenoh_transport::{
    multicast::TransportMulticast,
    unicast::{test_helpers::make_transport_manager_builder, TransportUnicast},
    TransportEventHandler, TransportManager, TransportMulticastEventHandler, TransportPeer,
    TransportPeerEventHandler,
};

const TIMEOUT: Duration = Duration::from_secs(60);
const SLEEP: Duration = Duration::from_secs(1);
const SLEEP_SEND: Duration = Duration::from_millis(1);

const MSG_COUNT: usize = 100;
lazy_static! {
    #[derive(Debug)]
    static ref MSG: NetworkMessage =  NetworkMessage::from(Push {
        wire_expr: "test".into(),
        ext_qos: QoSType::new(Priority::DEFAULT, CongestionControl::Drop, false),
        // 10 MB payload to stress fragmentation
        ..Push::from(Vec::from_iter((0..10_000_000).map(|b| b as u8)))
    });
}

// Transport Handler for the router
struct SHRouter {
    count: Arc<AtomicUsize>,
}

impl Default for SHRouter {
    fn default() -> Self {
        Self {
            count: Arc::new(AtomicUsize::new(0)),
        }
    }
}

impl SHRouter {
    fn get_count(&self) -> usize {
        self.count.load(Ordering::SeqCst)
    }
}

impl TransportEventHandler for SHRouter {
    fn new_unicast(
        &self,
        _peer: TransportPeer,
        _transport: TransportUnicast,
    ) -> ZResult<Arc<dyn TransportPeerEventHandler>> {
        let arc = Arc::new(SCRouter::new(self.count.clone()));
        Ok(arc)
    }

    fn new_multicast(
        &self,
        _transport: TransportMulticast,
    ) -> ZResult<Arc<dyn TransportMulticastEventHandler>> {
        panic!();
    }
}

// Transport Callback for the router
pub struct SCRouter {
    count: Arc<AtomicUsize>,
}

impl SCRouter {
    pub fn new(count: Arc<AtomicUsize>) -> Self {
        Self { count }
    }
}

impl TransportPeerEventHandler for SCRouter {
    fn handle_message(&self, message: NetworkMessageMut) -> ZResult<()> {
        assert_eq!(message.as_ref(), MSG.as_ref());
        self.count.fetch_add(1, Ordering::SeqCst);
        std::thread::sleep(2 * SLEEP_SEND);
        Ok(())
    }

    fn new_link(&self, _link: Link) {}
    fn del_link(&self, _link: Link) {}
    fn closed(&self) {}

    fn as_any(&self) -> &dyn Any {
        self
    }
}

// Transport Handler for the client
#[derive(Default)]
struct SHClient;

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
#[derive(Default)]
pub struct SCClient;

impl TransportPeerEventHandler for SCClient {
    fn handle_message(&self, _message: NetworkMessageMut) -> ZResult<()> {
        Ok(())
    }

    fn new_link(&self, _link: Link) {}
    fn del_link(&self, _link: Link) {}
    fn closed(&self) {}

    fn as_any(&self) -> &dyn Any {
        self
    }
}

async fn open_transport_unicast(
    client_endpoints: &[EndPoint],
    server_endpoints: &[EndPoint],
) -> (
    TransportManager,
    Arc<SHRouter>,
    TransportManager,
    TransportUnicast,
) {
    // Define client and router IDs
    let client_id = ZenohIdProto::try_from([1]).unwrap();
    let router_id = ZenohIdProto::try_from([2]).unwrap();

    // Create the router transport manager
    let router_handler = Arc::new(SHRouter::default());
    let unicast = make_transport_manager_builder(
        #[cfg(feature = "transport_multilink")]
        server_endpoints.len(),
        false,
    );
    let router_manager = TransportManager::builder()
        .zid(router_id)
        .whatami(WhatAmI::Router)
        .unicast(unicast)
        .build(router_handler.clone())
        .unwrap();

    // Create the listener on the router
    for e in server_endpoints.iter() {
        println!("Add endpoint: {e}");
        let _ = ztimeout!(router_manager.add_listener(e.clone())).unwrap();
    }

    // Create the client transport manager
    let unicast = make_transport_manager_builder(
        #[cfg(feature = "transport_multilink")]
        client_endpoints.len(),
        false,
    );
    let client_manager = TransportManager::builder()
        .whatami(WhatAmI::Client)
        .zid(client_id)
        .unicast(unicast)
        .build(Arc::new(SHClient))
        .unwrap();

    // Create an empty transport with the client
    // Open transport -> This should be accepted
    for e in client_endpoints.iter() {
        println!("Opening transport with {e}");
        let _ = ztimeout!(client_manager.open_transport_unicast(e.clone())).unwrap();
    }

    let client_transport = ztimeout!(client_manager.get_transport_unicast(&router_id)).unwrap();

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
            tokio::time::sleep(SLEEP).await;
        }
    });

    // Stop the locators on the manager
    for e in endpoints.iter() {
        println!("Del locator: {e}");
        ztimeout!(router_manager.del_listener(e)).unwrap();
    }

    ztimeout!(async {
        while !router_manager.get_listeners().await.is_empty() {
            tokio::time::sleep(SLEEP).await;
        }
    });

    // Wait a little bit
    tokio::time::sleep(SLEEP).await;

    ztimeout!(router_manager.close());
    ztimeout!(client_manager.close());

    // Wait a little bit
    tokio::time::sleep(SLEEP).await;
}

async fn test_transport(router_handler: Arc<SHRouter>, client_transport: TransportUnicast) {
    println!("Sending {MSG_COUNT} messages...");

    ztimeout!(async {
        let mut sent = 0;
        while router_handler.get_count() < MSG_COUNT {
            if client_transport.schedule(MSG.clone().as_mut()).is_ok() {
                sent += 1;
                println!(
                    "Sent: {sent}. Received: {}/{MSG_COUNT}",
                    router_handler.get_count()
                );
            }
        }
    });

    // Wait a little bit
    tokio::time::sleep(SLEEP).await;
}

async fn run_single(client_endpoints: &[EndPoint], server_endpoints: &[EndPoint]) {
    println!("\n>>> Running test for:  {client_endpoints:?}, {server_endpoints:?}",);

    #[allow(unused_variables)] // Used when stats feature is enabled
    let (router_manager, router_handler, client_manager, client_transport) =
        open_transport_unicast(client_endpoints, server_endpoints).await;

    test_transport(router_handler.clone(), client_transport.clone()).await;

    #[cfg(feature = "stats")]
    {
        let c_stats = client_transport.get_stats().unwrap().report();
        println!("\tClient: {c_stats:?}");
        let r_stats = ztimeout!(router_manager.get_transport_unicast(&client_manager.config.zid))
            .unwrap()
            .get_stats()
            .map(|s| s.report())
            .unwrap();
        println!("\tRouter: {r_stats:?}");
    }

    close_transport(
        router_manager,
        client_manager,
        client_transport,
        client_endpoints,
    )
    .await;
}

#[cfg(feature = "transport_tcp")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn fragmentation_unicast_tcp_only() {
    zenoh_util::init_log_from_env_or("error");

    // Define the locators
    let endpoints: Vec<EndPoint> = vec![format!("tcp/127.0.0.1:{}", 16800).parse().unwrap()];
    // Run
    run_single(&endpoints, &endpoints).await;
}
