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
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use zenoh::net::link::{EndPoint, Link};
use zenoh::net::protocol::core::{
    Channel, CongestionControl, Priority, Reliability, WhatAmI, ZenohId,
};
use zenoh::net::protocol::io::ZBuf;
use zenoh::net::protocol::message::ZenohMessage;
use zenoh::net::transport::{
    TransportEventHandler, TransportManager, TransportMulticast, TransportMulticastEventHandler,
    TransportPeer, TransportPeerEventHandler, TransportUnicast,
};
use zenoh_util::core::Result as ZResult;
use zenoh_util::zasync_executor_init;

const TIMEOUT: Duration = Duration::from_secs(60);
const SLEEP: Duration = Duration::from_millis(500);

const MSG_COUNT: usize = 16;
const MSG_SIZE: usize = 1_024;

macro_rules! ztimeout {
    ($f:expr) => {
        $f.timeout(TIMEOUT).await.unwrap()
    };
}

// Transport Handler for the router
struct SHPeer {
    pid: ZenohId,
    count: Arc<AtomicUsize>,
}

impl SHPeer {
    fn new(pid: ZenohId) -> Self {
        Self {
            pid,
            count: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn get_count(&self) -> usize {
        self.count.load(Ordering::SeqCst)
    }
}

impl TransportEventHandler for SHPeer {
    fn new_unicast(
        &self,
        _peer: TransportPeer,
        transport: TransportUnicast,
    ) -> ZResult<Arc<dyn TransportPeerEventHandler>> {
        // Create the message to send
        let key = "/test".into();
        let payload = ZBuf::from(vec![0_u8; MSG_SIZE]);
        let channel = Channel {
            priority: Priority::Control,
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

        println!("[Simultaneous {}] Sending {}...", self.pid, MSG_COUNT);
        for _ in 0..MSG_COUNT {
            transport.handle_message(message.clone()).unwrap();
        }
        println!("[Simultaneous {}] ... sent {}", self.pid, MSG_COUNT);

        let mh = Arc::new(MHPeer::new(self.count.clone()));
        Ok(mh)
    }

    fn new_multicast(
        &self,
        _transport: TransportMulticast,
    ) -> ZResult<Arc<dyn TransportMulticastEventHandler>> {
        panic!();
    }
}

struct MHPeer {
    count: Arc<AtomicUsize>,
}

impl MHPeer {
    fn new(count: Arc<AtomicUsize>) -> Self {
        Self { count }
    }
}

impl TransportPeerEventHandler for MHPeer {
    fn handle_message(&self, _msg: ZenohMessage) -> ZResult<()> {
        self.count.fetch_add(1, Ordering::AcqRel);
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

async fn transport_simultaneous(endpoint01: Vec<EndPoint>, endpoint02: Vec<EndPoint>) {
    /* [Peers] */
    let peer_id01 = ZenohId::new(1, [1_u8; ZenohId::MAX_SIZE]);
    let peer_id02 = ZenohId::new(1, [2_u8; ZenohId::MAX_SIZE]);

    // Create the peer01 transport manager
    let peer_sh01 = Arc::new(SHPeer::new(peer_id01));
    let unicast = TransportManager::config_unicast().max_links(endpoint01.len());
    let peer01_manager = TransportManager::builder()
        .whatami(WhatAmI::Peer)
        .zid(peer_id01)
        .unicast(unicast)
        .build(peer_sh01.clone())
        .unwrap();

    // Create the peer02 transport manager
    let peer_sh02 = Arc::new(SHPeer::new(peer_id02));
    let unicast = TransportManager::config_unicast().max_links(endpoint02.len());
    let peer02_manager = TransportManager::builder()
        .whatami(WhatAmI::Peer)
        .zid(peer_id02)
        .unicast(unicast)
        .build(peer_sh02.clone())
        .unwrap();

    // Add the endpoints on the peer01
    for e in endpoint01.iter() {
        let res = ztimeout!(peer01_manager.add_listener(e.clone()));
        println!("[Simultaneous 01a] => Adding endpoint {:?}: {:?}", e, res);
        assert!(res.is_ok());
    }
    let locs = peer01_manager.get_listeners();
    println!(
        "[Simultaneous 01b] => Getting endpoints: {:?} {:?}",
        endpoint01, locs
    );
    assert_eq!(endpoint01.len(), locs.len());

    // Add the endpoints on peer02
    for e in endpoint02.iter() {
        let res = ztimeout!(peer02_manager.add_listener(e.clone()));
        println!("[Simultaneous 02a] => Adding endpoint {:?}: {:?}", e, res);
        assert!(res.is_ok());
    }
    let locs = peer02_manager.get_listeners();
    println!(
        "[Simultaneous 02b] => Getting endpoints: {:?} {:?}",
        endpoint02, locs
    );
    assert_eq!(endpoint02.len(), locs.len());

    // Endpoints
    let c_ep01 = endpoint01.clone();
    let c_ep02 = endpoint02.clone();

    // Peer01
    let c_p01m = peer01_manager.clone();
    let peer01_task = task::spawn(async move {
        // Open the transport with the second peer
        // These open should succeed
        for e in c_ep02.iter() {
            println!("[Simultaneous 01c] => Opening transport with {:?}...", e);
            let _ = ztimeout!(c_p01m.open_transport(e.clone())).unwrap();
        }

        // These open should fails
        for e in c_ep02.iter() {
            println!("[Simultaneous 01d] => Exceeding transport with {:?}...", e);
            let res = ztimeout!(c_p01m.open_transport(e.clone()));
            assert!(res.is_err());
        }

        task::sleep(SLEEP).await;

        let tp02 = ztimeout!(async {
            let mut tp02 = None;
            while tp02.is_none() {
                task::sleep(SLEEP).await;
                println!(
                    "[Simultaneous 01e] => Transports: {:?}",
                    peer01_manager.get_transports()
                );
                tp02 = peer01_manager.get_transport(&peer_id02);
            }

            tp02.unwrap()
        });

        // Wait for the links to be properly established
        ztimeout!(async {
            let expected = endpoint01.len() + c_ep02.len();
            let mut tl02 = vec![];
            while tl02.len() != expected {
                task::sleep(SLEEP).await;
                tl02 = tp02.get_links().unwrap();
                println!("[Simultaneous 01f] => Links {}/{}", tl02.len(), expected);
            }
        });

        // Wait for the messages to arrive to peer 01
        ztimeout!(async {
            let mut check = 0;
            while check != MSG_COUNT {
                task::sleep(SLEEP).await;
                check = peer_sh01.get_count();
                println!("[Simultaneous 01g] => Received {:?}/{:?}", check, MSG_COUNT);
            }
        });
    });

    // Peer02
    let c_p02m = peer02_manager.clone();
    let peer02_task = task::spawn(async move {
        // Open the transport with the first peer
        // These open should succeed
        for e in c_ep01.iter() {
            println!("[Simultaneous 02c] => Opening transport with {:?}...", e);
            let _ = ztimeout!(c_p02m.open_transport(e.clone())).unwrap();
        }

        // These open should fails
        for e in c_ep01.iter() {
            println!("[Simultaneous 02d] => Exceeding transport with {:?}...", e);
            let res = ztimeout!(c_p02m.open_transport(e.clone()));
            assert!(res.is_err());
        }

        // Wait a little bit
        task::sleep(SLEEP).await;

        let tp01 = ztimeout!(async {
            let mut tp01 = None;
            while tp01.is_none() {
                task::sleep(SLEEP).await;
                println!(
                    "[Simultaneous 02e] => Transports: {:?}",
                    peer02_manager.get_transports()
                );
                tp01 = peer02_manager.get_transport(&peer_id01);
            }
            tp01.unwrap()
        });

        // Wait for the links to be properly established
        ztimeout!(async {
            let expected = c_ep01.len() + endpoint02.len();
            let mut tl01 = vec![];
            while tl01.len() != expected {
                task::sleep(SLEEP).await;
                tl01 = tp01.get_links().unwrap();
                println!("[Simultaneous 02f] => Links {}/{}", tl01.len(), expected);
            }
        });

        // Wait for the messages to arrive to peer 02
        ztimeout!(async {
            let mut check = 0;
            while check != MSG_COUNT {
                task::sleep(SLEEP).await;
                check = peer_sh02.get_count();
                println!("[Simultaneous 02g] => Received {:?}/{:?}", check, MSG_COUNT);
            }
        });
    });

    println!("[Simultaneous] => Waiting for peer01 and peer02 tasks...");
    peer01_task.join(peer02_task).await;
    println!("[Simultaneous] => Waiting for peer01 and peer02 tasks... DONE\n");

    // Wait a little bit
    task::sleep(SLEEP).await;
}

#[cfg(feature = "transport_tcp")]
#[test]
fn transport_tcp_simultaneous() {
    task::block_on(async {
        zasync_executor_init!();
    });

    let endpoint01: Vec<EndPoint> = vec![
        "tcp/127.0.0.1:15447".parse().unwrap(),
        "tcp/127.0.0.1:15448".parse().unwrap(),
        "tcp/127.0.0.1:15449".parse().unwrap(),
        "tcp/127.0.0.1:15450".parse().unwrap(),
        "tcp/127.0.0.1:15451".parse().unwrap(),
        "tcp/127.0.0.1:15452".parse().unwrap(),
        "tcp/127.0.0.1:15453".parse().unwrap(),
    ];
    let endpoint02: Vec<EndPoint> = vec![
        "tcp/127.0.0.1:15454".parse().unwrap(),
        "tcp/127.0.0.1:15455".parse().unwrap(),
        "tcp/127.0.0.1:15456".parse().unwrap(),
    ];

    task::block_on(async {
        transport_simultaneous(endpoint01, endpoint02).await;
    });
}
