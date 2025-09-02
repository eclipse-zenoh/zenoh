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
use std::{any::Any, convert::TryFrom, iter::FromIterator, sync::Arc, time::Duration};

use zenoh_core::ztimeout;
use zenoh_link::Link;
use zenoh_protocol::{
    core::{EndPoint, ZenohIdProto},
    network::NetworkMessageMut,
};
use zenoh_result::ZResult;
use zenoh_transport::{
    multicast::TransportMulticast, unicast::TransportUnicast, TransportEventHandler,
    TransportManager, TransportMulticastEventHandler, TransportPeer, TransportPeerEventHandler,
};

const TIMEOUT: Duration = Duration::from_secs(60);
const SLEEP: Duration = Duration::from_secs(1);

// Transport Handler for the router
struct SHRouter;

impl TransportEventHandler for SHRouter {
    fn new_unicast(
        &self,
        _peer: TransportPeer,
        _transport: TransportUnicast,
    ) -> ZResult<Arc<dyn TransportPeerEventHandler>> {
        let arc = Arc::new(SCRouter);
        Ok(arc)
    }

    fn new_multicast(
        &self,
        _transport: TransportMulticast,
    ) -> ZResult<Arc<dyn TransportMulticastEventHandler>> {
        panic!();
    }
}

pub struct SCRouter;

impl TransportPeerEventHandler for SCRouter {
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

async fn run(endpoints: &[EndPoint]) {
    // Define client and router IDs
    let router_id1 = ZenohIdProto::try_from([1]).unwrap();
    let router_id2 = ZenohIdProto::try_from([2]).unwrap();

    // Create the router transport manager
    println!(">>> Transport Whitelist [1a1]");
    let router_manager1: TransportManager = TransportManager::builder()
        .zid(router_id1)
        .protocols(Some(vec![])) // No protocols allowed
        .build(Arc::new(SHRouter))
        .unwrap();
    let router_manager2: TransportManager = TransportManager::builder()
        .zid(router_id2)
        .protocols(Some(vec![])) // No protocols allowed
        .build(Arc::new(SHRouter))
        .unwrap();

    // Create the listener on the router
    for e in endpoints.iter() {
        println!("Listener endpoint: {e}");
        let res = ztimeout!(router_manager1.add_listener_unicast(e.clone()));
        assert!(res.is_err());

        println!("Open endpoint: {e}");
        let res = ztimeout!(router_manager2.open_transport_unicast(e.clone()));
        assert!(res.is_err());
    }

    // Create the router transport manager
    println!(">>> Transport Whitelist [2a1]");
    let router_manager1 = TransportManager::builder()
        .zid(router_id1)
        .unicast(TransportManager::config_unicast().max_links(usize::MAX))
        .protocols(Some(Vec::from_iter(
            endpoints.iter().map(|e| e.protocol().to_string()),
        )))
        .build(Arc::new(SHRouter))
        .unwrap();
    let router_manager2 = TransportManager::builder()
        .zid(router_id2)
        .unicast(TransportManager::config_unicast().max_links(usize::MAX))
        .protocols(Some(Vec::from_iter(
            endpoints.iter().map(|e| e.protocol().to_string()),
        )))
        .build(Arc::new(SHRouter))
        .unwrap();

    // Create the listener on the router
    for e in endpoints.iter() {
        println!("Listener endpoint: {e}");
        let _ = ztimeout!(router_manager1.add_listener_unicast(e.clone())).unwrap();

        tokio::time::sleep(SLEEP).await;

        println!("Open endpoint: {e}");
        let _ = ztimeout!(router_manager2.open_transport_unicast(e.clone())).unwrap();

        tokio::time::sleep(SLEEP).await;
    }
}

#[cfg(feature = "transport_tcp")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn transport_whitelist_tcp() {
    zenoh_util::init_log_from_env_or("error");

    // Define the locators
    let endpoints: Vec<EndPoint> = vec![
        format!("tcp/127.0.0.1:{}", 17000).parse().unwrap(),
        format!("tcp/[::1]:{}", 17001).parse().unwrap(),
    ];
    // Run
    run(&endpoints).await;
}

#[cfg(feature = "transport_unixpipe")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn transport_whitelist_unixpipe() {
    zenoh_util::init_log_from_env_or("error");

    // Define the locators
    let endpoints: Vec<EndPoint> = vec![
        "unixpipe/transport_whitelist_unixpipe".parse().unwrap(),
        "unixpipe/transport_whitelist_unixpipe2".parse().unwrap(),
    ];
    // Run
    run(&endpoints).await;
}

#[cfg(all(feature = "transport_vsock", target_os = "linux"))]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn transport_whitelist_vsock() {
    zenoh_util::init_log_from_env_or("error");

    // Define the locators
    let endpoints: Vec<EndPoint> = vec![
        "vsock/VMADDR_CID_LOCAL:17000".parse().unwrap(),
        "vsock/1:17001".parse().unwrap(),
    ];
    // Run
    run(&endpoints).await;
}
