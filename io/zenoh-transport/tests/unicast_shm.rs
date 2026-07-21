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
            atomic::{AtomicBool, AtomicUsize, Ordering},
            Arc, Condvar, Mutex,
        },
        time::Duration,
    };

    use zenoh_buffers::buffer::SplitBuffer;
    use zenoh_core::{ztimeout, Wait};
    use zenoh_link::Link;
    use zenoh_protocol::{
        core::{CongestionControl, EndPoint, Priority, WhatAmI, ZenohIdProto},
        network::{push::ext::QoSType, NetworkBodyMut, NetworkMessage, NetworkMessageMut, Push},
        zenoh::{PushBody, Put},
    };
    use zenoh_result::ZResult;
    use zenoh_shm::{
        api::{
            protocol_implementations::posix::posix_shm_provider_backend_binary_heap::PosixShmProviderBackendBinaryHeap,
            provider::shm_provider::{BlockOn, GarbageCollect, ShmProviderBuilder},
        },
        ShmBufInner,
    };
    use zenoh_test::get_free_tcp_port;
    use zenoh_transport::{
        multicast::TransportMulticast, unicast::TransportUnicast, TransportEventHandler,
        TransportManager, TransportMulticastEventHandler, TransportPeer, TransportPeerEventHandler,
    };

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);

    const MSG_COUNT: usize = 1_000;
    const MSG_SIZE: usize = 1_024;

    const STALLED_QUEUE_BARRIER_MARKER: u8 = 0x01;
    const STALLED_QUEUE_SHM_MARKER: u8 = 0xa5;
    const STALLED_QUEUE_SHM_COUNT: usize = 32;
    const STALLED_QUEUE_SHM_SIZE: usize = 4 * 1024;
    const STALLED_QUEUE_DURATION: Duration = Duration::from_secs(1);

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
        fn handle_message(&self, message: NetworkMessageMut) -> ZResult<()> {
            if self.is_shm {
                print!("s");
            } else {
                print!("n");
            }
            let payload = match message.body {
                NetworkBodyMut::Push(m) => match &mut m.payload {
                    PushBody::Put(Put { payload, .. }) => {
                        for zs in payload.zslices() {
                            if self.is_shm && zs.downcast_ref::<ShmBufInner>().is_none() {
                                panic!("Expected ShmBufInner: {zs:?}");
                            } else if !self.is_shm && zs.downcast_ref::<ShmBufInner>().is_some() {
                                panic!("Not Expected ShmBufInner: {zs:?}");
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
        fn closed(&self) {}

        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    struct StallGate {
        entered: AtomicBool,
        released: Mutex<bool>,
        released_cv: Condvar,
    }

    impl StallGate {
        fn new() -> Self {
            Self {
                entered: AtomicBool::new(false),
                released: Mutex::new(false),
                released_cv: Condvar::new(),
            }
        }

        fn block(&self) {
            self.entered.store(true, Ordering::SeqCst);

            let mut released = self.released.lock().unwrap();
            while !*released {
                released = self.released_cv.wait(released).unwrap();
            }
        }

        fn is_entered(&self) -> bool {
            self.entered.load(Ordering::SeqCst)
        }

        fn release(&self) {
            *self.released.lock().unwrap() = true;
            self.released_cv.notify_all();
        }
    }

    struct StallReleaseGuard(Arc<StallGate>);

    impl Drop for StallReleaseGuard {
        fn drop(&mut self) {
            self.0.release();
        }
    }

    struct StalledQueuePeer {
        gate: Arc<StallGate>,
        shm_received: Arc<AtomicUsize>,
    }

    impl StalledQueuePeer {
        fn new() -> Self {
            Self {
                gate: Arc::new(StallGate::new()),
                shm_received: Arc::new(AtomicUsize::new(0)),
            }
        }

        fn is_stalled(&self) -> bool {
            self.gate.is_entered()
        }

        fn shm_received(&self) -> usize {
            self.shm_received.load(Ordering::SeqCst)
        }
    }

    impl TransportEventHandler for StalledQueuePeer {
        fn new_unicast(
            &self,
            _peer: TransportPeer,
            _transport: TransportUnicast,
        ) -> ZResult<Arc<dyn TransportPeerEventHandler>> {
            Ok(Arc::new(StalledQueueSCPeer {
                gate: self.gate.clone(),
                shm_received: self.shm_received.clone(),
            }))
        }

        fn new_multicast(
            &self,
            _transport: TransportMulticast,
        ) -> ZResult<Arc<dyn TransportMulticastEventHandler>> {
            panic!();
        }
    }

    struct StalledQueueSCPeer {
        gate: Arc<StallGate>,
        shm_received: Arc<AtomicUsize>,
    }

    impl TransportPeerEventHandler for StalledQueueSCPeer {
        fn handle_message(&self, message: NetworkMessageMut) -> ZResult<()> {
            let mut any_shm = false;
            let mut all_shm = true;
            let payload = match message.body {
                NetworkBodyMut::Push(m) => match &mut m.payload {
                    PushBody::Put(Put { payload, .. }) => {
                        for zs in payload.zslices() {
                            let is_shm = zs.downcast_ref::<ShmBufInner>().is_some();
                            any_shm |= is_shm;
                            all_shm &= is_shm;
                        }
                        payload.contiguous().into_owned()
                    }
                    _ => panic!("Unsolicited message"),
                },
                _ => panic!("Unsolicited message"),
            };

            match payload.first().copied() {
                Some(STALLED_QUEUE_BARRIER_MARKER) => {
                    assert!(!any_shm);
                    assert_eq!(payload.len(), 1);
                    self.gate.block();
                }
                Some(STALLED_QUEUE_SHM_MARKER) => {
                    assert!(
                        any_shm && all_shm,
                        "expected every queued payload slice to remain SHM-backed"
                    );
                    assert_eq!(payload.len(), STALLED_QUEUE_SHM_SIZE);

                    let mut sequence_bytes = [0_u8; 8];
                    sequence_bytes.copy_from_slice(&payload[1..9]);
                    let sequence = u64::from_le_bytes(sequence_bytes) as usize;
                    let expected_sequence = self.shm_received.fetch_add(1, Ordering::SeqCst);
                    assert_eq!(sequence, expected_sequence);
                    assert!(payload[9..]
                        .iter()
                        .all(|byte| *byte == STALLED_QUEUE_SHM_MARKER));
                }
                marker => panic!("unexpected payload marker: {marker:?}"),
            }

            Ok(())
        }

        fn new_link(&self, _link: Link) {}
        fn del_link(&self, _link: Link) {}
        fn closed(&self) {}

        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    #[cfg(feature = "transport_tcp")]
    async fn run_stalled_queue_test(endpoint: &EndPoint) {
        let receiver_zid = ZenohIdProto::try_from([4]).unwrap();
        let sender_zid = ZenohIdProto::try_from([5]).unwrap();

        let backend = PosixShmProviderBackendBinaryHeap::builder(
            2 * STALLED_QUEUE_SHM_COUNT * STALLED_QUEUE_SHM_SIZE,
        )
        .wait()
        .unwrap();
        let shm_provider = ShmProviderBuilder::backend(backend).wait();

        let receiver_handler = Arc::new(StalledQueuePeer::new());
        let receiver_manager = TransportManager::builder()
            .whatami(WhatAmI::Peer)
            .zid(receiver_zid)
            .unicast(TransportManager::config_unicast().qos(true))
            .build_test(receiver_handler.clone())
            .unwrap();

        let sender_handler = Arc::new(SHPeer::new(true));
        let sender_manager = TransportManager::builder()
            .whatami(WhatAmI::Peer)
            .zid(sender_zid)
            .unicast(TransportManager::config_unicast().qos(true))
            .build_test(sender_handler)
            .unwrap();

        ztimeout!(receiver_manager.add_listener(endpoint.clone())).unwrap();
        let transport = ztimeout!(sender_manager.open_transport_unicast(endpoint.clone())).unwrap();
        assert!(transport.is_shm().unwrap());

        // Block the receiver callback before sending the payloads that must remain queued.
        let mut barrier = NetworkMessage::from(Push {
            wire_expr: "test/stalled-queue/barrier".into(),
            ext_qos: QoSType::new(Priority::DEFAULT, CongestionControl::Block, false),
            ..Push::from(Put {
                payload: vec![STALLED_QUEUE_BARRIER_MARKER].into(),
                ..Put::default()
            })
        });
        transport.schedule(barrier.as_mut()).unwrap();

        ztimeout!(async {
            while !receiver_handler.is_stalled() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        });
        let stall_release_guard = StallReleaseGuard(receiver_handler.gate.clone());

        let layout = shm_provider.alloc_layout(STALLED_QUEUE_SHM_SIZE).unwrap();
        for sequence in 0..STALLED_QUEUE_SHM_COUNT {
            let mut shm_buf =
                ztimeout!(layout.alloc().with_policy::<BlockOn<GarbageCollect>>()).unwrap();
            shm_buf.fill(STALLED_QUEUE_SHM_MARKER);
            shm_buf[1..9].copy_from_slice(&(sequence as u64).to_le_bytes());

            let mut queued_shm = NetworkMessage::from(Push {
                wire_expr: "test/stalled-queue/shm".into(),
                ext_qos: QoSType::new(Priority::DEFAULT, CongestionControl::Block, false),
                ..Push::from(Put {
                    payload: shm_buf.into(),
                    ..Put::default()
                })
            });
            transport.schedule(queued_shm.as_mut()).unwrap();
        }

        // This is ten times the SHM validator interval, ensuring the buffer crosses
        // multiple expiration checks before the receiver is allowed to map it.
        tokio::time::sleep(STALLED_QUEUE_DURATION).await;
        assert_eq!(receiver_handler.shm_received(), 0);

        drop(stall_release_guard);
        ztimeout!(async {
            while receiver_handler.shm_received() != STALLED_QUEUE_SHM_COUNT {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        });

        ztimeout!(transport.close()).unwrap();
        ztimeout!(receiver_manager.del_listener(endpoint)).unwrap();
        ztimeout!(sender_manager.close());
        ztimeout!(receiver_manager.close());
    }

    async fn run(endpoint: &EndPoint, lowlatency_transport: bool) {
        println!("Transport SHM [0a]: {endpoint:?}");

        // Define client and router IDs
        let peer_shm01 = ZenohIdProto::try_from([1]).unwrap();
        let peer_shm02 = ZenohIdProto::try_from([2]).unwrap();
        let peer_net01 = ZenohIdProto::try_from([3]).unwrap();

        // create SHM provider
        let backend = PosixShmProviderBackendBinaryHeap::builder(2 * MSG_SIZE)
            .wait()
            .unwrap();
        let shm01 = ShmProviderBuilder::backend(backend).wait();

        // Create a peer manager with shared-memory authenticator enabled
        let peer_shm01_handler = Arc::new(SHPeer::new(true));
        let peer_shm01_manager = TransportManager::builder()
            .whatami(WhatAmI::Peer)
            .zid(peer_shm01)
            .unicast(
                TransportManager::config_unicast()
                    .lowlatency(lowlatency_transport)
                    .qos(!lowlatency_transport),
            )
            .build_test(peer_shm01_handler.clone())
            .unwrap();

        // Create a peer manager with shared-memory authenticator enabled
        let peer_shm02_handler = Arc::new(SHPeer::new(true));
        let peer_shm02_manager = TransportManager::builder()
            .whatami(WhatAmI::Peer)
            .zid(peer_shm02)
            .unicast(
                TransportManager::config_unicast()
                    .lowlatency(lowlatency_transport)
                    .qos(!lowlatency_transport),
            )
            .build_test(peer_shm02_handler.clone())
            .unwrap();

        let mut shm = zenoh_config::ShmConf::default();
        let _ = shm.set_enabled(false);

        // Create a peer manager with shared-memory authenticator disabled
        let peer_net01_handler = Arc::new(SHPeer::new(false));
        let peer_net01_manager = TransportManager::builder()
            .whatami(WhatAmI::Peer)
            .zid(peer_net01)
            .shm(shm)
            .unicast(
                TransportManager::config_unicast()
                    .lowlatency(lowlatency_transport)
                    .qos(!lowlatency_transport),
            )
            .build_test(peer_net01_handler.clone())
            .unwrap();

        // Create the listener on the peer
        println!("Transport SHM [1a]");
        let _ = ztimeout!(peer_shm01_manager.add_listener(endpoint.clone())).unwrap();

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
        let peer_shm02_transport =
            ztimeout!(peer_shm01_manager.get_transport_unicast(&peer_shm02)).unwrap();
        assert!(peer_shm02_transport.is_shm().unwrap());

        println!("Transport SHM [2b]");
        let peer_net01_transport =
            ztimeout!(peer_shm01_manager.get_transport_unicast(&peer_net01)).unwrap();
        assert!(!peer_net01_transport.is_shm().unwrap());

        let layout = shm01.alloc_layout(MSG_SIZE).unwrap();

        // Send the message
        println!("Transport SHM [3a]");
        // The msg count
        for (msg_count, _) in (0..MSG_COUNT).enumerate() {
            // Create the message to send
            let mut sbuf =
                ztimeout!(layout.alloc().with_policy::<BlockOn<GarbageCollect>>()).unwrap();
            sbuf[0..8].copy_from_slice(&msg_count.to_le_bytes());

            let mut message = NetworkMessage::from(Push {
                wire_expr: "test".into(),
                ext_qos: QoSType::new(Priority::DEFAULT, CongestionControl::Block, false),
                ..Push::from(Put {
                    payload: sbuf.into(),
                    ..Put::default()
                })
            });

            peer_shm02_transport.schedule(message.as_mut()).unwrap();
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
            let mut sbuf =
                ztimeout!(layout.alloc().with_policy::<BlockOn<GarbageCollect>>()).unwrap();
            sbuf[0..8].copy_from_slice(&msg_count.to_le_bytes());

            let mut message = NetworkMessage::from(Push {
                wire_expr: "test".into(),
                ext_qos: QoSType::new(Priority::DEFAULT, CongestionControl::Block, false),
                ..Push::from(Put {
                    payload: sbuf.into(),
                    ..Put::default()
                })
            });

            peer_net01_transport.schedule(message.as_mut()).unwrap();
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
            while !peer_shm01_manager.get_listeners().await.is_empty() {
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
        zenoh_util::init_log_from_env_or("error");
        let endpoint: EndPoint = format!("tcp/127.0.0.1:{}", get_free_tcp_port())
            .parse()
            .unwrap();
        run(&endpoint, false).await;
    }

    #[cfg(feature = "transport_tcp")]
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn transport_tcp_shm_with_lowlatency_transport() {
        zenoh_util::init_log_from_env_or("error");
        let endpoint: EndPoint = format!("tcp/127.0.0.1:{}", get_free_tcp_port())
            .parse()
            .unwrap();
        run(&endpoint, true).await;
    }

    #[cfg(feature = "transport_tcp")]
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn transport_tcp_shm_survives_stalled_queue() {
        zenoh_util::init_log_from_env_or("error");
        let endpoint: EndPoint = format!("tcp/127.0.0.1:{}", get_free_tcp_port())
            .parse()
            .unwrap();

        run_stalled_queue_test(&endpoint).await;
    }

    #[cfg(feature = "transport_ws")]
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn transport_ws_shm() {
        zenoh_util::init_log_from_env_or("error");
        let endpoint: EndPoint = format!("ws/127.0.0.1:{}", get_free_tcp_port())
            .parse()
            .unwrap();
        run(&endpoint, false).await;
    }

    #[cfg(feature = "transport_ws")]
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn transport_ws_shm_with_lowlatency_transport() {
        zenoh_util::init_log_from_env_or("error");
        let endpoint: EndPoint = format!("ws/127.0.0.1:{}", get_free_tcp_port())
            .parse()
            .unwrap();
        run(&endpoint, true).await;
    }

    #[cfg(feature = "transport_unixpipe")]
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn transport_unixpipe_shm() {
        zenoh_util::init_log_from_env_or("error");
        let endpoint: EndPoint = "unixpipe/transport_unixpipe_shm".parse().unwrap();
        run(&endpoint, false).await;
    }

    #[cfg(feature = "transport_unixpipe")]
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn transport_unixpipe_shm_with_lowlatency_transport() {
        zenoh_util::init_log_from_env_or("error");
        let endpoint: EndPoint = "unixpipe/transport_unixpipe_shm_with_lowlatency_transport"
            .parse()
            .unwrap();
        run(&endpoint, true).await;
    }
}
