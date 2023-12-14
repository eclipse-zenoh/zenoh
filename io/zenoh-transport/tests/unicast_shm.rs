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
#[cfg(feature = "shared-memory")]
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
    use zenoh_buffers::SplitBuffer;
    use zenoh_core::ztimeout;
    use zenoh_link::Link;
    use zenoh_protocol::{
        core::{CongestionControl, Encoding, EndPoint, Priority, WhatAmI, ZenohId},
        network::{
            push::ext::{NodeIdType, QoSType},
            NetworkBody, NetworkMessage, Push,
        },
        zenoh::{PushBody, Put},
    };
    use zenoh_result::{zerror, ZResult};
    use zenoh_shm::{SharedMemoryBuf, SharedMemoryManager};
    use zenoh_transport::{
        multicast::TransportMulticast, unicast::TransportUnicast, TransportEventHandler,
        TransportManager, TransportMulticastEventHandler, TransportPeer, TransportPeerEventHandler,
    };

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const USLEEP: Duration = Duration::from_micros(100);

    const MSG_COUNT: usize = 1_000;
    const MSG_SIZE: usize = 1_024;

    // Transport Handler for the router
    struct SHPeer {
        count: Arc<AtomicUsize>,
        is_shm: bool,
    }

    impl SHPeer {
        fn new(is_shm: bool) -> Self {
            Self {
                count: Arc::new(AtomicUsize::new(0)),
                is_shm,
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
            _transport: TransportUnicast,
        ) -> ZResult<Arc<dyn TransportPeerEventHandler>> {
            let arc = Arc::new(SCPeer::new(self.count.clone(), self.is_shm));
            Ok(arc)
        }

        fn new_multicast(
            &self,
            _transport: TransportMulticast,
        ) -> ZResult<Arc<dyn TransportMulticastEventHandler>> {
            panic!();
        }
    }

    // Transport Callback for the peer
    pub struct SCPeer {
        count: Arc<AtomicUsize>,
        is_shm: bool,
    }

    impl SCPeer {
        pub fn new(count: Arc<AtomicUsize>, is_shm: bool) -> Self {
            Self { count, is_shm }
        }
    }

    impl TransportPeerEventHandler for SCPeer {
        fn handle_message(&self, message: NetworkMessage) -> ZResult<()> {
            if self.is_shm {
                print!("s");
            } else {
                print!("n");
            }
            let payload = match message.body {
                NetworkBody::Push(m) => match m.payload {
                    PushBody::Put(Put { payload, .. }) => {
                        for zs in payload.zslices() {
                            if self.is_shm && zs.downcast_ref::<SharedMemoryBuf>().is_none() {
                                panic!("Expected SharedMemoryBuf: {:?}", zs);
                            } else if !self.is_shm && zs.downcast_ref::<SharedMemoryBuf>().is_some()
                            {
                                panic!("Not Expected SharedMemoryBuf: {:?}", zs);
                            }
                        }
                        payload.contiguous().into_owned()
                    }
                    _ => panic!("Unsolicited message"),
                },
                _ => panic!("Unsolicited message"),
            };
            assert_eq!(payload.len(), MSG_SIZE);

            let mut count_bytes = [0_u8; 8];
            count_bytes.copy_from_slice(&payload[0..8]);
            let msg_count = u64::from_le_bytes(count_bytes) as usize;
            let sex_count = self.count.fetch_add(1, Ordering::SeqCst);
            assert_eq!(msg_count, sex_count);
            print!("{msg_count} ");

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

    async fn run(endpoint: &EndPoint, lowlatency_transport: bool) {
        println!("Transport SHM [0a]: {endpoint:?}");

        // Define client and router IDs
        let peer_shm01 = ZenohId::try_from([1]).unwrap();
        let peer_shm02 = ZenohId::try_from([2]).unwrap();
        let peer_net01 = ZenohId::try_from([3]).unwrap();

        let mut tries = 100;
        let mut prng = PseudoRng::from_entropy();
        let mut shm01 = loop {
            // Create the SharedMemoryManager
            if let Ok(shm01) = SharedMemoryManager::make(
                format!("peer_shm01_{}_{}", endpoint.protocol(), prng.gen::<usize>()),
                2 * MSG_SIZE,
            ) {
                break Ok(shm01);
            }
            tries -= 1;
            if tries == 0 {
                break Err(zerror!("Unable to create SharedMemoryManager!"));
            }
        }
        .unwrap();

        // Create a peer manager with shared-memory authenticator enabled
        let peer_shm01_handler = Arc::new(SHPeer::new(true));
        let peer_shm01_manager = TransportManager::builder()
            .whatami(WhatAmI::Peer)
            .zid(peer_shm01)
            .unicast(
                TransportManager::config_unicast()
                    .shm(true)
                    .lowlatency(lowlatency_transport)
                    .qos(!lowlatency_transport),
            )
            .build(peer_shm01_handler.clone())
            .unwrap();

        // Create a peer manager with shared-memory authenticator enabled
        let peer_shm02_handler = Arc::new(SHPeer::new(true));
        let peer_shm02_manager = TransportManager::builder()
            .whatami(WhatAmI::Peer)
            .zid(peer_shm02)
            .unicast(
                TransportManager::config_unicast()
                    .shm(true)
                    .lowlatency(lowlatency_transport)
                    .qos(!lowlatency_transport),
            )
            .build(peer_shm02_handler.clone())
            .unwrap();

        // Create a peer manager with shared-memory authenticator disabled
        let peer_net01_handler = Arc::new(SHPeer::new(false));
        let peer_net01_manager = TransportManager::builder()
            .whatami(WhatAmI::Peer)
            .zid(peer_net01)
            .unicast(
                TransportManager::config_unicast()
                    .shm(false)
                    .lowlatency(lowlatency_transport)
                    .qos(!lowlatency_transport),
            )
            .build(peer_net01_handler.clone())
            .unwrap();

        // Create the listener on the peer
        println!("Transport SHM [1a]");
        let _ = ztimeout!(peer_shm01_manager
            .add_listener(endpoint.clone())
            )
        .unwrap();

        // Create a transport with the peer
        println!("Transport SHM [1b]");
        let peer_shm01_transport =
            ztimeout!(peer_shm02_manager.open_transport_unicast(endpoint.clone())).unwrap();
        assert!(peer_shm01_transport.is_shm().unwrap());

        // Create a transport with the peer
        println!("Transport SHM [1c]");
        let peer_net02_transport =
            ztimeout!(peer_net01_manager.open_transport_unicast(endpoint.clone())).unwrap();
        assert!(!peer_net02_transport.is_shm().unwrap());

        // Retrieve the transports
        println!("Transport SHM [2a]");
        let peer_shm02_transport = peer_shm01_manager
            .get_transport_unicast(&peer_shm02)
            .unwrap();
        assert!(peer_shm02_transport.is_shm().unwrap());

        println!("Transport SHM [2b]");
        let peer_net01_transport = peer_shm01_manager
            .get_transport_unicast(&peer_net01)
            .unwrap();
        assert!(!peer_net01_transport.is_shm().unwrap());

        // Send the message
        println!("Transport SHM [3a]");
        // The msg count
        for (msg_count, _) in (0..MSG_COUNT).enumerate() {
            // Create the message to send
            let mut sbuf = ztimeout!(async {
                loop {
                    match shm01.alloc(MSG_SIZE) {
                        Ok(sbuf) => break sbuf,
                        Err(_) => tokio::time::sleep(USLEEP).await,
                    }
                }
            });

            let bs = unsafe { sbuf.as_mut_slice() };
            bs[0..8].copy_from_slice(&msg_count.to_le_bytes());

            let message: NetworkMessage = Push {
                wire_expr: "test".into(),
                ext_qos: QoSType::new(Priority::default(), CongestionControl::Block, false),
                ext_tstamp: None,
                ext_nodeid: NodeIdType::default(),
                payload: Put {
                    payload: sbuf.into(),
                    timestamp: None,
                    encoding: Encoding::default(),
                    ext_sinfo: None,
                    ext_shm: None,
                    ext_attachment: None,
                    ext_unknown: vec![],
                }
                .into(),
            }
            .into();

            peer_shm02_transport.schedule(message).unwrap();
        }

        // Wait a little bit
        tokio::time::sleep(SLEEP).await;

        // Wait for the messages to arrive to the other side
        println!("Transport SHM [3b]");
        ztimeout!(async {
            while peer_shm02_handler.get_count() != MSG_COUNT {
                tokio::time::sleep(SLEEP).await;
            }
        });

        // Send the message
        println!("Transport SHM [4a]");
        // The msg count
        for (msg_count, _) in (0..MSG_COUNT).enumerate() {
            // Create the message to send
            let mut sbuf = ztimeout!(async {
                loop {
                    match shm01.alloc(MSG_SIZE) {
                        Ok(sbuf) => break sbuf,
                        Err(_) => tokio::time::sleep(USLEEP).await,
                    }
                }
            });
            let bs = unsafe { sbuf.as_mut_slice() };
            bs[0..8].copy_from_slice(&msg_count.to_le_bytes());

            let message: NetworkMessage = Push {
                wire_expr: "test".into(),
                ext_qos: QoSType::new(Priority::default(), CongestionControl::Block, false),
                ext_tstamp: None,
                ext_nodeid: NodeIdType::default(),
                payload: Put {
                    payload: sbuf.into(),
                    timestamp: None,
                    encoding: Encoding::default(),
                    ext_sinfo: None,
                    ext_shm: None,
                    ext_attachment: None,
                    ext_unknown: vec![],
                }
                .into(),
            }
            .into();

            peer_net01_transport.schedule(message).unwrap();
        }

        // Wait a little bit
        tokio::time::sleep(SLEEP).await;

        // Wait for the messages to arrive to the other side
        println!("Transport SHM [4b]");
        ztimeout!(async {
            while peer_net01_handler.get_count() != MSG_COUNT {
                tokio::time::sleep(SLEEP).await;
            }
        });

        // Wait a little bit
        tokio::time::sleep(SLEEP).await;

        // Close the transports
        println!("Transport SHM [5a]");
        ztimeout!(peer_shm02_transport.close()).unwrap();

        println!("Transport SHM [5b]");
        ztimeout!(peer_net01_transport.close()).unwrap();

        ztimeout!(async {
            while !peer_shm01_manager.get_transports_unicast().await.is_empty() {
                tokio::time::sleep(SLEEP).await;
            }
        });

        // Delete the listener
        println!("Transport SHM [6a]");
        ztimeout!(peer_shm01_manager.del_listener(endpoint)).unwrap();

        // Wait a little bit
        ztimeout!(async {
            while !peer_shm01_manager.get_listeners().is_empty() {
                tokio::time::sleep(SLEEP).await;
            }
        });
        tokio::time::sleep(SLEEP).await;

        ztimeout!(peer_net01_manager.close());
        ztimeout!(peer_shm01_manager.close());
        ztimeout!(peer_shm02_manager.close());

        // Wait a little bit
        tokio::time::sleep(SLEEP).await;
    }

    #[cfg(feature = "transport_tcp")]
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn transport_tcp_shm() {
        let _ = env_logger::try_init();
        let endpoint: EndPoint = format!("tcp/127.0.0.1:{}", 14000).parse().unwrap();
        run(&endpoint, false).await;
    }

    #[cfg(feature = "transport_tcp")]
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn transport_tcp_shm_with_lowlatency_transport() {
        let _ = env_logger::try_init();
        let endpoint: EndPoint = format!("tcp/127.0.0.1:{}", 14001).parse().unwrap();
        run(&endpoint, true).await;
    }

    #[cfg(feature = "transport_ws")]
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn transport_ws_shm() {
        let _ = env_logger::try_init();
        let endpoint: EndPoint = format!("ws/127.0.0.1:{}", 14010).parse().unwrap();
        run(&endpoint, false).await;
    }

    #[cfg(feature = "transport_ws")]
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn transport_ws_shm_with_lowlatency_transport() {
        let _ = env_logger::try_init();
        let endpoint: EndPoint = format!("ws/127.0.0.1:{}", 14011).parse().unwrap();
        run(&endpoint, true).await;
    }

    #[cfg(feature = "transport_unixpipe")]
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn transport_unixpipe_shm() {
        let _ = env_logger::try_init();
        let endpoint: EndPoint = "unixpipe/transport_unixpipe_shm".parse().unwrap();
        run(&endpoint, false).await;
    }

    #[cfg(feature = "transport_unixpipe")]
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn transport_unixpipe_shm_with_lowlatency_transport() {
        let _ = env_logger::try_init();
        let endpoint: EndPoint = "unixpipe/transport_unixpipe_shm_with_lowlatency_transport"
            .parse()
            .unwrap();
        run(&endpoint, true).await;
    }
}
