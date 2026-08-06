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

// Unit tests for the per-category SHM optimization gating (`transport_optimization.messages`).
//
// Two behaviors are covered:
// - Implicit optimization: a normal (non-SHM) ZBuf payload above the size threshold is
//   copied into SHM only when its category is enabled in the policy.
// - Explicit forwarding: a payload already allocated in SHM is always forwarded as a
//   descriptor, regardless of the policy.
//
// In both cases "promoted" is observed as ZSliceKind::ShmPtr together with the `ext_shm`
// extension being set on the message.
#[cfg(feature = "shared-memory")]
mod optimization_policy {
    use std::{num::NonZeroUsize, sync::Arc, time::Duration};

    use zenoh_buffers::{ZBuf, ZSliceKind};
    use zenoh_core::Wait;
    use zenoh_protocol::{
        core::{CongestionControl, Encoding, Priority},
        network::{push::ext::QoSType, NetworkBody, NetworkMessage, Push, Request, Response},
        zenoh::{
            query::{ext::QueryBodyType, Query},
            reply::Reply,
            PushBody, Put, RequestBody, ResponseBody,
        },
    };
    use zenoh_shm::{
        api::{
            common::types::ProtocolID,
            protocol_implementations::posix::posix_shm_provider_backend_binary_heap::PosixShmProviderBackendBinaryHeap,
            provider::shm_provider::{BlockOn, GarbageCollect, ShmProviderBuilder},
        },
        ShmBufInner,
    };
    use zenoh_transport::common::shm::{
        map_zmsg_to_partner, LazyShmProvider, PartnerShmConfig, ProviderInitState,
        ShmOptimizationPolicy,
    };

    // Implicit transport optimization only kicks in for payloads at or above this size.
    const MESSAGE_SIZE_THRESHOLD: usize = 3072;
    // A raw (non-SHM) payload larger than the threshold, hence eligible for the
    // implicit optimization (automatic copy into SHM).
    const LARGE_PAYLOAD_SIZE: usize = 4096;
    // SHM arena size: must comfortably exceed a handful of LARGE_PAYLOAD_SIZE allocations.
    const POOL_SIZE: usize = 1024 * 1024;

    // Partner that supports every SHM protocol. Only relevant to the explicit-buffer
    // branch; the implicit optimization does not consult it, but the argument is required.
    struct AllProtocols;
    impl PartnerShmConfig for AllProtocols {
        fn supports_protocol(&self, _protocol: ProtocolID) -> bool {
            true
        }
    }

    // Build a LazyShmProvider for the implicit optimization and drive its lazy
    // initialization to completion, so map_zmsg_to_partner can actually wrap payloads.
    async fn ready_provider() -> Option<Arc<LazyShmProvider>> {
        let provider = Arc::new(LazyShmProvider::new(
            NonZeroUsize::new(POOL_SIZE).unwrap(),
            MESSAGE_SIZE_THRESHOLD,
        ));
        loop {
            match provider.try_get_provider() {
                ProviderInitState::Ready(_) => break,
                ProviderInitState::Initializing(_) => {
                    tokio::time::sleep(Duration::from_millis(10)).await
                }
                ProviderInitState::Error => panic!("failed to initialize SHM provider"),
            }
        }
        Some(provider)
    }

    // A normal, non-SHM payload large enough to be eligible for implicit optimization.
    fn raw_payload() -> ZBuf {
        ZBuf::from(vec![0u8; LARGE_PAYLOAD_SIZE])
    }

    fn put_message(payload: ZBuf) -> NetworkMessage {
        NetworkMessage::from(Push {
            wire_expr: "test".into(),
            ext_qos: QoSType::new(Priority::DEFAULT, CongestionControl::Block, false),
            ..Push::from(Put {
                payload,
                ..Put::default()
            })
        })
    }

    fn query_message(payload: ZBuf) -> NetworkMessage {
        let mut query = Query::rand();
        query.ext_body = Some(QueryBodyType {
            ext_shm: None,
            encoding: Encoding::default(),
            payload,
        });
        let mut request = Request::rand();
        request.payload = RequestBody::Query(query);
        NetworkMessage::from(NetworkBody::Request(request))
    }

    fn reply_message(payload: ZBuf) -> NetworkMessage {
        let mut reply = Reply::rand();
        reply.payload = PushBody::Put(Put {
            payload,
            ..Put::default()
        });
        let mut response = Response::rand();
        response.payload = ResponseBody::Reply(reply);
        NetworkMessage::from(NetworkBody::Response(response))
    }

    // Run map_zmsg_to_partner and report whether the payload was promoted to SHM
    // (i.e. it will be sent as a descriptor). Asserts the two promotion signals
    // (ZSliceKind::ShmPtr and the `ext_shm` extension) are consistent with each other.
    fn map_and_check_promoted(
        mut msg: NetworkMessage,
        provider: &Option<Arc<LazyShmProvider>>,
        policy: ShmOptimizationPolicy,
    ) -> bool {
        {
            let mut m = msg.as_mut();
            map_zmsg_to_partner(&mut m, &AllProtocols, provider, policy);
        }

        let (payload, ext_shm_set) = match &msg.body {
            NetworkBody::Push(Push {
                payload:
                    PushBody::Put(Put {
                        payload, ext_shm, ..
                    }),
                ..
            }) => (payload, ext_shm.is_some()),
            NetworkBody::Request(Request {
                payload:
                    RequestBody::Query(Query {
                        ext_body: Some(body),
                        ..
                    }),
                ..
            }) => (&body.payload, body.ext_shm.is_some()),
            NetworkBody::Response(Response {
                payload:
                    ResponseBody::Reply(Reply {
                        payload:
                            PushBody::Put(Put {
                                payload, ext_shm, ..
                            }),
                        ..
                    }),
                ..
            }) => (payload, ext_shm.is_some()),
            other => panic!("unexpected message body: {other:?}"),
        };

        // "Promoted" = the payload will be sent over SHM as a descriptor, signaled by
        // ZSliceKind::ShmPtr and by the `ext_shm` extension; both must agree. We key on
        // these rather than on `downcast_ref::<ShmBufInner>()`, because an explicitly
        // allocated SHM buffer downcasts to ShmBufInner whether or not it is forwarded.
        let kind_is_shmptr = payload.zslices().next().unwrap().kind == ZSliceKind::ShmPtr;
        assert_eq!(
            kind_is_shmptr, ext_shm_set,
            "inconsistent SHM promotion signals (ZSliceKind::ShmPtr vs ext_shm)"
        );
        kind_is_shmptr
    }

    const ALL_ON: ShmOptimizationPolicy = ShmOptimizationPolicy {
        put: true,
        query: true,
        reply: true,
    };
    const NO_PUBS: ShmOptimizationPolicy = ShmOptimizationPolicy {
        put: false,
        query: true,
        reply: true,
    };
    const NO_QUERIES: ShmOptimizationPolicy = ShmOptimizationPolicy {
        put: true,
        query: false,
        reply: true,
    };
    const NO_REPLIES: ShmOptimizationPolicy = ShmOptimizationPolicy {
        put: true,
        query: true,
        reply: false,
    };

    #[tokio::test]
    async fn publications_flag_gates_put() {
        let provider = ready_provider().await;
        assert!(map_and_check_promoted(
            put_message(raw_payload()),
            &provider,
            ALL_ON
        ));
        assert!(map_and_check_promoted(
            put_message(raw_payload()),
            &provider,
            NO_QUERIES
        ));
        assert!(map_and_check_promoted(
            put_message(raw_payload()),
            &provider,
            NO_REPLIES
        ));
        assert!(!map_and_check_promoted(
            put_message(raw_payload()),
            &provider,
            NO_PUBS
        ));
    }

    #[tokio::test]
    async fn query_flag_gates_query() {
        let provider = ready_provider().await;
        // Query follows the `query` flag, independently of `put` and `reply`.
        assert!(map_and_check_promoted(
            query_message(raw_payload()),
            &provider,
            ALL_ON
        ));
        assert!(map_and_check_promoted(
            query_message(raw_payload()),
            &provider,
            NO_PUBS
        ));
        assert!(map_and_check_promoted(
            query_message(raw_payload()),
            &provider,
            NO_REPLIES
        ));
        assert!(!map_and_check_promoted(
            query_message(raw_payload()),
            &provider,
            NO_QUERIES
        ));
    }

    #[tokio::test]
    async fn reply_flag_gates_reply() {
        let provider = ready_provider().await;
        // Reply follows the `reply` flag, independently of `put` and `query`.
        assert!(map_and_check_promoted(
            reply_message(raw_payload()),
            &provider,
            ALL_ON
        ));
        assert!(map_and_check_promoted(
            reply_message(raw_payload()),
            &provider,
            NO_PUBS
        ));
        assert!(map_and_check_promoted(
            reply_message(raw_payload()),
            &provider,
            NO_QUERIES
        ));
        assert!(!map_and_check_promoted(
            reply_message(raw_payload()),
            &provider,
            NO_REPLIES
        ));
    }

    // A payload below the threshold is never promoted, even when the category is
    // enabled: this is the size gate of the transport optimization itself.
    #[tokio::test]
    async fn below_threshold_is_never_promoted() {
        let provider = ready_provider().await;
        let small = || ZBuf::from(vec![0u8; MESSAGE_SIZE_THRESHOLD - 1]);
        assert!(!map_and_check_promoted(
            put_message(small()),
            &provider,
            ALL_ON
        ));
        assert!(!map_and_check_promoted(
            query_message(small()),
            &provider,
            ALL_ON
        ));
        assert!(!map_and_check_promoted(
            reply_message(small()),
            &provider,
            ALL_ON
        ));
    }

    // Allocate a real SHM buffer and return it as a ZBuf payload. Its ZSlice downcasts to
    // ShmBufInner, exercising the explicit-forwarding branch of map_to_partner.
    async fn shm_payload() -> ZBuf {
        const SHM_ARENA: usize = 4 * LARGE_PAYLOAD_SIZE;
        let backend = PosixShmProviderBackendBinaryHeap::builder(SHM_ARENA)
            .wait()
            .unwrap();
        let provider = ShmProviderBuilder::backend(backend).wait();
        let layout = provider.alloc_layout(LARGE_PAYLOAD_SIZE).unwrap();
        let sbuf = layout
            .alloc()
            .with_policy::<BlockOn<GarbageCollect>>()
            .await
            .unwrap();
        let zbuf: ZBuf = sbuf.into();
        assert!(zbuf
            .zslices()
            .next()
            .unwrap()
            .downcast_ref::<ShmBufInner>()
            .is_some());
        zbuf
    }

    // Explicitly-allocated SHM buffers are always forwarded as descriptors, even when the
    // implicit optimization is disabled for that message category (and even with no
    // implicit provider at all).
    #[tokio::test]
    async fn explicit_shm_buffer_always_forwarded() {
        // No implicit provider: only the explicit branch can promote here.
        let no_provider: Option<Arc<LazyShmProvider>> = None;

        // Category disabled -> still forwarded.
        assert!(map_and_check_promoted(
            put_message(shm_payload().await),
            &no_provider,
            NO_PUBS
        ));
        assert!(map_and_check_promoted(
            query_message(shm_payload().await),
            &no_provider,
            NO_QUERIES
        ));
        assert!(map_and_check_promoted(
            reply_message(shm_payload().await),
            &no_provider,
            NO_REPLIES
        ));

        // Category enabled -> forwarded as well.
        assert!(map_and_check_promoted(
            query_message(shm_payload().await),
            &no_provider,
            ALL_ON
        ));
    }
}
