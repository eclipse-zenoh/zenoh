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
#[cfg(target_family = "unix")]
mod tests {
    use std::{
        any::Any,
        convert::TryFrom,
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
        network::{push::ext::QoSType, NetworkMessage, NetworkMessageMut, Push},
    };
    use zenoh_result::ZResult;
    use zenoh_transport::{
        multicast::TransportMulticast, unicast::TransportUnicast, TransportEventHandler,
        TransportManager, TransportMulticastEventHandler, TransportPeer, TransportPeerEventHandler,
    };

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_millis(500);

    const MSG_COUNT: usize = 16;
    const MSG_SIZE: usize = 1_024;

    // Transport Handler for the router
    struct SHPeer {
        zid: ZenohIdProto,
        count: Arc<AtomicUsize>,
    }

    impl SHPeer {
        fn new(zid: ZenohIdProto) -> Self {
            Self {
                zid,
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
            let message = NetworkMessage::from(Push {
                wire_expr: "test".into(),
                ext_qos: QoSType::new(Priority::Control, CongestionControl::Block, false),
                ..Push::from(vec![0u8; MSG_SIZE])
            });

            println!("[Simultaneous {}] Sending {}...", self.zid, MSG_COUNT);
            for _ in 0..MSG_COUNT {
                transport.schedule(message.clone().as_mut()).unwrap();
            }
            println!("[Simultaneous {}] ... sent {}", self.zid, MSG_COUNT);

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
        fn handle_message(&self, _msg: NetworkMessageMut) -> ZResult<()> {
            self.count.fetch_add(1, Ordering::AcqRel);
            Ok(())
        }

        fn new_link(&self, _link: Link) {}
        fn del_link(&self, _link: Link) {}
        fn closed(&self) {}

        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    async fn transport_simultaneous(endpoint01: Vec<EndPoint>, endpoint02: Vec<EndPoint>) {
        /* [Peers] */
        let peer_id01 = ZenohIdProto::try_from([2]).unwrap();
        let peer_id02 = ZenohIdProto::try_from([3]).unwrap();

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
            println!("[Simultaneous 01a] => Adding endpoint {e:?}: {res:?}");
            assert!(res.is_ok());
        }
        let locs = ztimeout!(peer01_manager.get_listeners());
        println!("[Simultaneous 01b] => Getting endpoints: {endpoint01:?} {locs:?}");
        assert_eq!(endpoint01.len(), locs.len());

        // Add the endpoints on peer02
        for e in endpoint02.iter() {
            let res = ztimeout!(peer02_manager.add_listener(e.clone()));
            println!("[Simultaneous 02a] => Adding endpoint {e:?}: {res:?}");
            assert!(res.is_ok());
        }
        let locs = ztimeout!(peer02_manager.get_listeners());
        println!("[Simultaneous 02b] => Getting endpoints: {endpoint02:?} {locs:?}");
        assert_eq!(endpoint02.len(), locs.len());

        // Endpoints
        let c_ep01 = endpoint01.clone();
        let c_ep02 = endpoint02.clone();

        // Peer01
        let c_p01m = peer01_manager.clone();
        let peer01_task = tokio::task::spawn(async move {
            // Open the transport with the second peer
            // These open should succeed
            for e in c_ep02.iter() {
                println!("[Simultaneous 01c] => Opening transport with {e:?}...");
                let _ = ztimeout!(c_p01m.open_transport_unicast(e.clone())).unwrap();
            }

            // These open should fails
            for e in c_ep02.iter() {
                println!("[Simultaneous 01d] => Exceeding transport with {e:?}...");
                let res = ztimeout!(c_p01m.open_transport_unicast(e.clone()));
                assert!(res.is_err());
            }

            tokio::time::sleep(SLEEP).await;

            let tp02 = ztimeout!(async {
                let mut tp02 = None;
                while tp02.is_none() {
                    tokio::time::sleep(SLEEP).await;
                    println!(
                        "[Simultaneous 01e] => Transports: {:?}",
                        peer01_manager.get_transports_unicast().await
                    );
                    tp02 = peer01_manager.get_transport_unicast(&peer_id02).await;
                }

                tp02.unwrap()
            });

            // Wait for the links to be properly established
            ztimeout!(async {
                let expected = endpoint01.len() + c_ep02.len();
                let mut tl02 = vec![];
                while tl02.len() != expected {
                    tokio::time::sleep(SLEEP).await;
                    tl02 = tp02.get_links().unwrap();
                    println!("[Simultaneous 01f] => Links {}/{}", tl02.len(), expected);
                }
            });

            // Wait for the messages to arrive to peer 01
            ztimeout!(async {
                let mut check = 0;
                while check != MSG_COUNT {
                    tokio::time::sleep(SLEEP).await;
                    check = peer_sh01.get_count();
                    println!("[Simultaneous 01g] => Received {check:?}/{MSG_COUNT:?}");
                }
            });
        });

        // Peer02
        let c_p02m = peer02_manager.clone();
        let peer02_task = tokio::task::spawn(async move {
            // Open the transport with the first peer
            // These open should succeed
            for e in c_ep01.iter() {
                println!("[Simultaneous 02c] => Opening transport with {e:?}...");
                let _ = ztimeout!(c_p02m.open_transport_unicast(e.clone())).unwrap();
            }

            // These open should fails
            for e in c_ep01.iter() {
                println!("[Simultaneous 02d] => Exceeding transport with {e:?}...");
                let res = ztimeout!(c_p02m.open_transport_unicast(e.clone()));
                assert!(res.is_err());
            }

            // Wait a little bit
            tokio::time::sleep(SLEEP).await;

            let tp01 = ztimeout!(async {
                let mut tp01 = None;
                while tp01.is_none() {
                    tokio::time::sleep(SLEEP).await;
                    println!(
                        "[Simultaneous 02e] => Transports: {:?}",
                        peer02_manager.get_transports_unicast().await
                    );
                    tp01 = peer02_manager.get_transport_unicast(&peer_id01).await;
                }
                tp01.unwrap()
            });

            // Wait for the links to be properly established
            ztimeout!(async {
                let expected = c_ep01.len() + endpoint02.len();
                let mut tl01 = vec![];
                while tl01.len() != expected {
                    tokio::time::sleep(SLEEP).await;
                    tl01 = tp01.get_links().unwrap();
                    println!("[Simultaneous 02f] => Links {}/{}", tl01.len(), expected);
                }
            });

            // Wait for the messages to arrive to peer 02
            ztimeout!(async {
                let mut check = 0;
                while check != MSG_COUNT {
                    tokio::time::sleep(SLEEP).await;
                    check = peer_sh02.get_count();
                    println!("[Simultaneous 02g] => Received {check:?}/{MSG_COUNT:?}");
                }
            });
        });

        println!("[Simultaneous] => Waiting for peer01 and peer02 tasks...");
        let _ = tokio::join!(peer01_task, peer02_task);
        println!("[Simultaneous] => Waiting for peer01 and peer02 tasks... DONE\n");

        // Wait a little bit
        tokio::time::sleep(SLEEP).await;
    }

    #[cfg(feature = "transport_tcp")]
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn transport_tcp_simultaneous() {
        zenoh_util::init_log_from_env_or("error");
        let endpoint01: Vec<EndPoint> = vec![
            format!("tcp/127.0.0.1:{}", 15000).parse().unwrap(),
            format!("tcp/127.0.0.1:{}", 15001).parse().unwrap(),
            format!("tcp/127.0.0.1:{}", 15002).parse().unwrap(),
            format!("tcp/127.0.0.1:{}", 15003).parse().unwrap(),
        ];
        let endpoint02: Vec<EndPoint> = vec![
            format!("tcp/127.0.0.1:{}", 15010).parse().unwrap(),
            format!("tcp/127.0.0.1:{}", 15011).parse().unwrap(),
            format!("tcp/127.0.0.1:{}", 15012).parse().unwrap(),
            format!("tcp/127.0.0.1:{}", 15013).parse().unwrap(),
        ];

        transport_simultaneous(endpoint01, endpoint02).await;
    }

    #[cfg(feature = "transport_unixpipe")]
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[ignore]
    async fn transport_unixpipe_simultaneous() {
        zenoh_util::init_log_from_env_or("error");
        let endpoint01: Vec<EndPoint> = vec![
            "unixpipe/transport_unixpipe_simultaneous".parse().unwrap(),
            "unixpipe/transport_unixpipe_simultaneous2".parse().unwrap(),
            "unixpipe/transport_unixpipe_simultaneous3".parse().unwrap(),
            "unixpipe/transport_unixpipe_simultaneous4".parse().unwrap(),
        ];
        let endpoint02: Vec<EndPoint> = vec![
            "unixpipe/transport_unixpipe_simultaneous5".parse().unwrap(),
            "unixpipe/transport_unixpipe_simultaneous6".parse().unwrap(),
            "unixpipe/transport_unixpipe_simultaneous7".parse().unwrap(),
            "unixpipe/transport_unixpipe_simultaneous8".parse().unwrap(),
        ];

        transport_simultaneous(endpoint01, endpoint02).await;
    }

    #[cfg(feature = "transport_ws")]
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[ignore]
    async fn transport_ws_simultaneous() {
        zenoh_util::init_log_from_env_or("error");

        let endpoint01: Vec<EndPoint> = vec![
            format!("ws/127.0.0.1:{}", 15020).parse().unwrap(),
            format!("ws/127.0.0.1:{}", 15021).parse().unwrap(),
            format!("ws/127.0.0.1:{}", 15022).parse().unwrap(),
            format!("ws/127.0.0.1:{}", 15023).parse().unwrap(),
        ];
        let endpoint02: Vec<EndPoint> = vec![
            format!("ws/127.0.0.1:{}", 15030).parse().unwrap(),
            format!("ws/127.0.0.1:{}", 15031).parse().unwrap(),
            format!("ws/127.0.0.1:{}", 15032).parse().unwrap(),
            format!("ws/127.0.0.1:{}", 15033).parse().unwrap(),
        ];

        transport_simultaneous(endpoint01, endpoint02).await;
    }
}
