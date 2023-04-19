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
use async_std::{prelude::FutureExt, task};
use std::{any::Any, convert::TryFrom, iter::FromIterator, sync::Arc, time::Duration};
use zenoh_core::zasync_executor_init;
use zenoh_link::Link;
use zenoh_protocol::{
    core::{EndPoint, ZenohId},
    zenoh::ZenohMessage,
};
use zenoh_result::ZResult;
use zenoh_transport::{
    TransportEventHandler, TransportManager, TransportMulticast, TransportMulticastEventHandler,
    TransportPeer, TransportPeerEventHandler, TransportUnicast,
};

const TIMEOUT: Duration = Duration::from_secs(60);
const SLEEP: Duration = Duration::from_secs(1);

macro_rules! ztimeout {
    ($f:expr) => {
        $f.timeout(TIMEOUT).await.unwrap()
    };
}

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
    fn handle_message(&self, _message: ZenohMessage) -> ZResult<()> {
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

async fn run(endpoints: &[EndPoint]) {
    // Define client and router IDs
    let router_id = ZenohId::try_from([1]).unwrap();

    // Create the router transport manager
    println!(">>> Transport Whitelist [1a1]");
    let router_manager = TransportManager::builder()
        .zid(router_id)
        .protocols(Some(vec![])) // No protocols allowed
        .build(Arc::new(SHRouter))
        .unwrap();

    // Create the listener on the router
    for e in endpoints.iter() {
        println!("Listener endpoint: {e}");
        let res = ztimeout!(router_manager.add_listener(e.clone()));
        assert!(res.is_err());

        println!("Open endpoint: {e}");
        let res = ztimeout!(router_manager.open_transport(e.clone()));
        assert!(res.is_err());
    }

    // Create the router transport manager
    println!(">>> Transport Whitelist [2a1]");
    let unicast = TransportManager::config_unicast().max_links(usize::MAX);
    let router_manager = TransportManager::builder()
        .zid(router_id)
        .unicast(unicast)
        .protocols(Some(Vec::from_iter(
            endpoints.iter().map(|e| e.protocol().to_string()),
        )))
        .build(Arc::new(SHRouter))
        .unwrap();

    // Create the listener on the router
    for e in endpoints.iter() {
        println!("Listener endpoint: {e}");
        let _ = ztimeout!(router_manager.add_listener(e.clone())).unwrap();

        task::sleep(SLEEP).await;

        println!("Open endpoint: {e}");
        let _ = ztimeout!(router_manager.open_transport(e.clone())).unwrap();

        task::sleep(SLEEP).await;
    }
}

#[cfg(feature = "transport_tcp")]
#[test]
fn transport_whitelist_tcp() {
    let _ = env_logger::try_init();
    task::block_on(async {
        zasync_executor_init!();
    });

    // Define the locators
    let endpoints: Vec<EndPoint> = vec![
        format!("tcp/127.0.0.1:{}", 17000).parse().unwrap(),
        format!("tcp/[::1]:{}", 17001).parse().unwrap(),
    ];
    // Run
    task::block_on(run(&endpoints));
}
