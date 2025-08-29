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
    io::Write,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use zenoh_core::ztimeout;
use zenoh_link::Link;
use zenoh_protocol::{
    core::{CongestionControl, EndPoint, Priority, WhatAmI, ZenohIdProto},
    network::{
        push::{ext::QoSType, Push},
        NetworkMessage, NetworkMessageMut,
    },
};
use zenoh_result::ZResult;
use zenoh_transport::{
    multicast::TransportMulticast,
    unicast::{test_helpers::make_transport_manager_builder, TransportUnicast},
    DummyTransportPeerEventHandler, TransportEventHandler, TransportManager,
    TransportMulticastEventHandler, TransportPeer, TransportPeerEventHandler,
};

const MSG_SIZE: usize = 8;
const MSG_COUNT: usize = 100_000;
const TIMEOUT: Duration = Duration::from_secs(300);
const SLEEP: Duration = Duration::from_millis(100);
const USLEEP: Duration = Duration::from_millis(1);

#[cfg(test)]
#[derive(Default)]
struct SHRouterIntermittent;

impl TransportEventHandler for SHRouterIntermittent {
    fn new_unicast(
        &self,
        _peer: TransportPeer,
        _transport: TransportUnicast,
    ) -> ZResult<Arc<dyn TransportPeerEventHandler>> {
        Ok(Arc::new(DummyTransportPeerEventHandler))
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
        Ok(Arc::new(DummyTransportPeerEventHandler))
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
    fn handle_message(&self, _message: NetworkMessageMut) -> ZResult<()> {
        self.counter.fetch_add(1, Ordering::AcqRel);
        Ok(())
    }

    fn new_link(&self, _link: Link) {}
    fn del_link(&self, _link: Link) {}
    fn closed(&self) {}

    fn as_any(&self) -> &dyn Any {
        self
    }
}

async fn transport_intermittent(endpoint: &EndPoint, lowlatency_transport: bool) {
    /* [ROUTER] */
    let router_id = ZenohIdProto::try_from([1]).unwrap();

    let router_handler = Arc::new(SHRouterIntermittent);
    // Create the router transport manager
    let unicast = make_transport_manager_builder(
        #[cfg(feature = "transport_multilink")]
        1,
        lowlatency_transport,
    )
    .max_sessions(3);
    let router_manager = TransportManager::builder()
        .whatami(WhatAmI::Router)
        .zid(router_id)
        .unicast(unicast)
        .build(router_handler.clone())
        .unwrap();

    /* [CLIENT] */
    let client01_id = ZenohIdProto::try_from([2]).unwrap();
    let client02_id = ZenohIdProto::try_from([3]).unwrap();
    let client03_id = ZenohIdProto::try_from([4]).unwrap();

    // Create the transport transport manager for the first client
    let counter = Arc::new(AtomicUsize::new(0));
    let unicast = make_transport_manager_builder(
        #[cfg(feature = "transport_multilink")]
        1,
        lowlatency_transport,
    )
    .max_sessions(3);
    let client01_manager = TransportManager::builder()
        .whatami(WhatAmI::Client)
        .zid(client01_id)
        .unicast(unicast)
        .build(Arc::new(SHClientStable::new(counter.clone())))
        .unwrap();

    // Create the transport transport manager for the second client
    let unicast = make_transport_manager_builder(
        #[cfg(feature = "transport_multilink")]
        1,
        lowlatency_transport,
    )
    .max_sessions(1);
    let client02_manager = TransportManager::builder()
        .whatami(WhatAmI::Client)
        .zid(client02_id)
        .unicast(unicast)
        .build(Arc::new(SHClientIntermittent))
        .unwrap();

    // Create the transport transport manager for the third client
    let unicast = make_transport_manager_builder(
        #[cfg(feature = "transport_multilink")]
        1,
        lowlatency_transport,
    )
    .max_sessions(1);
    let client03_manager = TransportManager::builder()
        .whatami(WhatAmI::Client)
        .zid(client03_id)
        .unicast(unicast)
        .build(Arc::new(SHClientIntermittent))
        .unwrap();

    /* [1] */
    // Add a listener to the router
    println!("\nTransport Intermittent [1a1]");
    let _ = ztimeout!(router_manager.add_listener(endpoint.clone())).unwrap();
    let locators = ztimeout!(router_manager.get_listeners());
    println!("Transport Intermittent [1a2]: {locators:?}");
    assert_eq!(locators.len(), 1);

    /* [2] */
    // Open a transport from client01 to the router
    let c_ses1 = ztimeout!(client01_manager.open_transport_unicast(endpoint.clone())).unwrap();
    assert_eq!(c_ses1.get_links().unwrap().len(), 1);
    assert_eq!(
        ztimeout!(client01_manager.get_transports_unicast()).len(),
        1
    );
    assert_eq!(c_ses1.get_zid().unwrap(), router_id);

    /* [3] */
    // Continously open and close transport from client02 and client03 to the router
    let c_client02_manager = client02_manager.clone();
    let c_endpoint = endpoint.clone();
    let c_router_id = router_id;
    let c2_handle = tokio::task::spawn(async move {
        loop {
            print!("+");
            std::io::stdout().flush().unwrap();

            let c_ses2 =
                ztimeout!(c_client02_manager.open_transport_unicast(c_endpoint.clone())).unwrap();
            assert_eq!(c_ses2.get_links().unwrap().len(), 1);
            assert_eq!(
                ztimeout!(c_client02_manager.get_transports_unicast()).len(),
                1
            );
            assert_eq!(c_ses2.get_zid().unwrap(), c_router_id);

            tokio::time::sleep(SLEEP).await;

            print!("-");
            std::io::stdout().flush().unwrap();

            ztimeout!(c_ses2.close()).unwrap();

            tokio::time::sleep(SLEEP).await;
        }
    });

    let c_client03_manager = client03_manager.clone();
    let c_endpoint = endpoint.clone();
    let c_router_id = router_id;
    let c3_handle = tokio::task::spawn(async move {
        loop {
            print!("*");
            std::io::stdout().flush().unwrap();

            let c_ses3 =
                ztimeout!(c_client03_manager.open_transport_unicast(c_endpoint.clone())).unwrap();
            assert_eq!(c_ses3.get_links().unwrap().len(), 1);
            assert_eq!(
                ztimeout!(c_client03_manager.get_transports_unicast()).len(),
                1
            );
            assert_eq!(c_ses3.get_zid().unwrap(), c_router_id);

            tokio::time::sleep(SLEEP).await;

            print!("");
            std::io::stdout().flush().unwrap();

            ztimeout!(c_ses3.close()).unwrap();

            tokio::time::sleep(SLEEP).await;
        }
    });

    /* [4] */
    println!("Transport Intermittent [4a1]");
    let c_router_manager = router_manager.clone();
    let rt = tokio::runtime::Handle::current();
    let _ = ztimeout!(tokio::task::spawn_blocking(move || rt.block_on(async {
        // Create the message to send
        let message = NetworkMessage::from(Push {
            wire_expr: "test".into(),
            ext_qos: QoSType::new(Priority::DEFAULT, CongestionControl::Block, false),
            ..Push::from(vec![0u8; MSG_SIZE])
        });

        let mut ticks: Vec<usize> = (0..=MSG_COUNT).step_by(MSG_COUNT / 10).collect();
        ticks.remove(0);

        let mut count = 0;
        while count < MSG_COUNT {
            if count == ticks[0] {
                println!("\nScheduled {count}");
                ticks.remove(0);
            }
            let transports = c_router_manager.get_transports_unicast().await;
            if !transports.is_empty() {
                for s in transports.iter() {
                    if let Ok(ll) = s.get_links() {
                        if ll.is_empty() {
                            print!("#");
                        } else {
                            assert_eq!(ll.len(), 1);
                        }
                    }
                    let res = s.schedule(message.clone().as_mut());
                    if res.is_err() {
                        print!("X");
                        std::io::stdout().flush().unwrap();
                    }
                }
                count += 1;
            } else {
                print!("O");
                tokio::time::sleep(USLEEP).await;
            }
        }
    })));

    // Stop the tasks
    c2_handle.abort();
    c3_handle.abort();

    // Check that client01 received all the messages
    println!("Transport Intermittent [4b1]");
    ztimeout!(async {
        loop {
            let c = counter.load(Ordering::Acquire);
            if c == MSG_COUNT {
                break;
            }
            println!("Transport Intermittent [4b2]: Received {c}/{MSG_COUNT}");
            tokio::time::sleep(SLEEP).await;
        }
    });

    /* [5] */
    // Close the open transport on the client
    println!("Transport Intermittent [5a1]");
    for s in ztimeout!(client01_manager.get_transports_unicast()).iter() {
        ztimeout!(s.close()).unwrap();
    }
    println!("Transport Intermittent [5a2]");
    for s in ztimeout!(client02_manager.get_transports_unicast()).iter() {
        ztimeout!(s.close()).unwrap();
    }
    println!("Transport Intermittent [5a3]");
    for s in ztimeout!(client03_manager.get_transports_unicast()).iter() {
        ztimeout!(s.close()).unwrap();
    }

    /* [6] */
    // Verify that the transport has been closed also on the router
    println!("Transport Intermittent [6a1]");
    ztimeout!(async {
        loop {
            let transports = router_manager.get_transports_unicast().await;
            if transports.is_empty() {
                break;
            }
            tokio::time::sleep(SLEEP).await;
        }
    });

    /* [7] */
    // Perform clean up of the open locators
    println!("\nTransport Intermittent [7a1]");
    ztimeout!(router_manager.del_listener(endpoint)).unwrap();

    // Wait a little bit
    tokio::time::sleep(SLEEP).await;

    ztimeout!(router_manager.close());
    ztimeout!(client01_manager.close());
    ztimeout!(client02_manager.close());
    ztimeout!(client03_manager.close());

    // Wait a little bit
    tokio::time::sleep(SLEEP).await;
}

async fn universal_transport_intermittent(endpoint: &EndPoint) {
    transport_intermittent(endpoint, false).await
}

async fn lowlatency_transport_intermittent(endpoint: &EndPoint) {
    transport_intermittent(endpoint, true).await
}

#[cfg(feature = "transport_tcp")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn transport_tcp_intermittent() {
    zenoh_util::init_log_from_env_or("error");
    let endpoint: EndPoint = format!("tcp/127.0.0.1:{}", 12000).parse().unwrap();
    universal_transport_intermittent(&endpoint).await;
}

#[cfg(feature = "transport_tcp")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn transport_tcp_intermittent_for_lowlatency_transport() {
    zenoh_util::init_log_from_env_or("error");
    let endpoint: EndPoint = format!("tcp/127.0.0.1:{}", 12100).parse().unwrap();
    lowlatency_transport_intermittent(&endpoint).await;
}

#[cfg(feature = "transport_ws")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn transport_ws_intermittent() {
    zenoh_util::init_log_from_env_or("error");
    let endpoint: EndPoint = format!("ws/127.0.0.1:{}", 12010).parse().unwrap();
    universal_transport_intermittent(&endpoint).await;
}

#[cfg(feature = "transport_ws")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn transport_ws_intermittent_for_lowlatency_transport() {
    zenoh_util::init_log_from_env_or("error");
    let endpoint: EndPoint = format!("ws/127.0.0.1:{}", 12110).parse().unwrap();
    lowlatency_transport_intermittent(&endpoint).await;
}

#[cfg(feature = "transport_unixpipe")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn transport_unixpipe_intermittent() {
    zenoh_util::init_log_from_env_or("error");
    let endpoint: EndPoint = "unixpipe/transport_unixpipe_intermittent".parse().unwrap();
    universal_transport_intermittent(&endpoint).await;
}

#[cfg(feature = "transport_unixpipe")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn transport_unixpipe_intermittent_for_lowlatency_transport() {
    zenoh_util::init_log_from_env_or("error");
    let endpoint: EndPoint = "unixpipe/transport_unixpipe_intermittent_for_lowlatency_transport"
        .parse()
        .unwrap();
    lowlatency_transport_intermittent(&endpoint).await;
}

#[cfg(all(feature = "transport_vsock", target_os = "linux"))]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn transport_vsock_intermittent() {
    zenoh_util::init_log_from_env_or("error");
    let endpoint: EndPoint = "vsock/VMADDR_CID_LOCAL:17000".parse().unwrap();
    universal_transport_intermittent(&endpoint).await;
}
