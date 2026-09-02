//
// Copyright (c) 2026 ZettaScale Technology
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

//! End-to-end tests for the CAN link, over a virtual bus.
//!
//! These need a `vcan0` interface, which creating requires root:
//!
//! ```sh
//! ci/vcan-setup.sh
//! ```
//!
//! which is the equivalent of `modprobe vcan`, `ip link add dev vcan0 type
//! vcan` and `ip link set up vcan0`.
//!
//! They are `#[ignore]`d so no CI job runs them by accident, and they also skip
//! at runtime with an explanation when the interface is absent, so running them
//! deliberately on a machine without one reports why rather than failing.
//!
//! Run them with:
//!
//! ```sh
//! cargo test -p zenoh-transport --features transport_can --test multicast_can -- --ignored --nocapture
//! ```
//!
//! `candump -td vcan0` in another terminal shows every frame.

#[cfg(all(feature = "transport_can", target_os = "linux"))]
mod tests {
    use std::{
        any::Any,
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        },
        time::Duration,
    };

    use zenoh_buffers::buffer::SplitBuffer;
    use zenoh_core::ztimeout;
    use zenoh_link::Link;
    use zenoh_protocol::{
        core::{
            Channel, CongestionControl, EndPoint, Priority, Reliability, WhatAmI, ZenohIdProto,
        },
        network::{
            push::{ext::QoSType, Push},
            NetworkBodyMut, NetworkMessage, NetworkMessageMut,
        },
        zenoh::PushBody,
    };
    use zenoh_result::ZResult;
    use zenoh_transport::{
        multicast::{TransportManagerBuilderMulticast, TransportMulticast},
        unicast::TransportUnicast,
        TransportEventHandler, TransportManager, TransportMulticastEventHandler, TransportPeer,
        TransportPeerEventHandler,
    };

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const SLEEP_COUNT: Duration = Duration::from_millis(10);

    /// Far fewer than the UDP test's 1 000: every message here is fragmented
    /// across a 63-byte MTU, so this is already thousands of frames.
    const MSG_COUNT: usize = 100;

    /// Well above the link MTU, so zenoh's own fragmentation drives the link.
    /// 189 bytes is the figure measured with two zenoh-pico peers, so
    /// the two phases are directly comparable.
    const MSG_SIZE_FRAGMENTED: [usize; 1] = [189];

    /// Past a single batch, so fragmentation is exercised well beyond one frame
    /// worth of bookkeeping.
    const MSG_SIZE_LARGE: [usize; 1] = [4_096];

    const DEVICE: &str = "vcan0";

    /// Whether the virtual bus exists. Creating one needs root, so a developer
    /// without it gets an explanation rather than a failure.
    fn vcan_present() -> bool {
        if std::path::Path::new(&format!("/sys/class/net/{DEVICE}")).exists() {
            return true;
        }
        println!("SKIPPING: no `{DEVICE}` interface. Create one with `ci/vcan-setup.sh`.");
        false
    }

    /// Counts what arrives and checks it byte for byte.
    ///
    /// Counting alone would not catch a reassembly bug: a message split across
    /// 71 CAN frames and put back together wrongly still arrives.
    struct SHPeer {
        count: Arc<AtomicUsize>,
        corrupt: Arc<AtomicUsize>,
        expected: Arc<Vec<u8>>,
    }

    impl SHPeer {
        fn new(expected: Vec<u8>) -> Self {
            Self {
                count: Arc::new(AtomicUsize::new(0)),
                corrupt: Arc::new(AtomicUsize::new(0)),
                expected: Arc::new(expected),
            }
        }

        fn get_count(&self) -> usize {
            self.count.load(Ordering::Relaxed)
        }

        fn get_corrupt(&self) -> usize {
            self.corrupt.load(Ordering::Relaxed)
        }
    }

    impl TransportEventHandler for SHPeer {
        fn new_unicast(
            &self,
            _peer: TransportPeer,
            _transport: TransportUnicast,
        ) -> ZResult<Arc<dyn TransportPeerEventHandler>> {
            panic!("a CAN link is multicast only");
        }

        fn new_multicast(
            &self,
            _transport: TransportMulticast,
        ) -> ZResult<Arc<dyn TransportMulticastEventHandler>> {
            Ok(Arc::new(SCPeer::new(
                self.count.clone(),
                self.corrupt.clone(),
                self.expected.clone(),
            )))
        }
    }

    pub struct SCPeer {
        count: Arc<AtomicUsize>,
        corrupt: Arc<AtomicUsize>,
        expected: Arc<Vec<u8>>,
    }

    impl SCPeer {
        pub fn new(
            count: Arc<AtomicUsize>,
            corrupt: Arc<AtomicUsize>,
            expected: Arc<Vec<u8>>,
        ) -> Self {
            Self {
                count,
                corrupt,
                expected,
            }
        }
    }

    impl TransportMulticastEventHandler for SCPeer {
        fn new_peer(&self, peer: TransportPeer) -> ZResult<Arc<dyn TransportPeerEventHandler>> {
            println!("\tNew peer: {}", peer.zid);
            Ok(Arc::new(SCPeer {
                count: self.count.clone(),
                corrupt: self.corrupt.clone(),
                expected: self.expected.clone(),
            }))
        }
        fn closed(&self) {}

        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    impl TransportPeerEventHandler for SCPeer {
        fn handle_message(&self, msg: NetworkMessageMut) -> ZResult<()> {
            if let NetworkBodyMut::Push(push) = msg.body {
                if let PushBody::Put(put) = &push.payload {
                    if put.payload.contiguous().as_ref() != self.expected.as_slice() {
                        self.corrupt.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
            self.count.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }

        fn new_link(&self, _link: Link) {}
        fn del_link(&self, _link: Link) {}
        fn closed(&self) {}

        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    struct TransportMulticastPeer {
        manager: TransportManager,
        handler: Arc<SHPeer>,
        transport: TransportMulticast,
    }

    /// Unlike the UDP multicast test, the two peers open *different* endpoints:
    /// on a CAN bus a peer's identifier is its address, and a peer drops frames
    /// carrying its own identifier. Two peers sharing one `id` would each
    /// discard everything the other sent.
    ///
    /// `qos` decides whether the transport keeps one transmission queue or one
    /// per priority. It is off by default, and with it off every batch reports
    /// priority index 0, so a link that maps priority onto the wire sees
    /// nothing but `Control`.
    async fn open_transport_qos(
        endpoint01: &EndPoint,
        endpoint02: &EndPoint,
        payload: &[u8],
        qos: bool,
    ) -> (TransportMulticastPeer, TransportMulticastPeer) {
        let peer01_id = ZenohIdProto::try_from([1]).unwrap();
        let peer02_id = ZenohIdProto::try_from([2]).unwrap();

        let peer01_handler = Arc::new(SHPeer::new(payload.to_vec()));
        let peer01_manager = TransportManager::builder()
            .zid(peer01_id)
            .whatami(WhatAmI::Peer)
            .multicast(TransportManagerBuilderMulticast::default().qos(qos))
            .build_test(peer01_handler.clone())
            .unwrap();

        let peer02_handler = Arc::new(SHPeer::new(payload.to_vec()));
        let peer02_manager = TransportManager::builder()
            .whatami(WhatAmI::Peer)
            .zid(peer02_id)
            .multicast(TransportManagerBuilderMulticast::default().qos(qos))
            .build_test(peer02_handler.clone())
            .unwrap();

        println!("Opening transport with {endpoint01}");
        let t01 = ztimeout!(peer01_manager.open_transport_multicast(endpoint01.clone())).unwrap();
        assert!(!ztimeout!(peer01_manager.get_transports_multicast()).is_empty());
        println!("\tPeer01 link MTU: {}", t01.get_link().unwrap().mtu);

        println!("Opening transport with {endpoint02}");
        let t02 = ztimeout!(peer02_manager.open_transport_multicast(endpoint02.clone())).unwrap();
        assert!(!ztimeout!(peer02_manager.get_transports_multicast()).is_empty());
        println!("\tPeer02 link MTU: {}", t02.get_link().unwrap().mtu);

        // The two peers find each other through the Join messages the multicast
        // transport emits; there is no handshake on a bus.
        ztimeout!(async {
            while peer01_manager
                .get_transport_multicast(&peer02_id)
                .await
                .is_none()
            {
                tokio::time::sleep(SLEEP_COUNT).await;
            }
        });
        let peer01_transport =
            ztimeout!(peer01_manager.get_transport_multicast(&peer02_id)).unwrap();
        println!(
            "\tPeer01 peers: {:?}",
            peer01_transport.get_peers().unwrap()
        );

        ztimeout!(async {
            while peer02_manager
                .get_transport_multicast(&peer01_id)
                .await
                .is_none()
            {
                tokio::time::sleep(SLEEP_COUNT).await;
            }
        });
        let peer02_transport =
            ztimeout!(peer02_manager.get_transport_multicast(&peer01_id)).unwrap();
        println!(
            "\tPeer02 peers: {:?}",
            peer02_transport.get_peers().unwrap()
        );

        (
            TransportMulticastPeer {
                manager: peer01_manager,
                handler: peer01_handler,
                transport: peer01_transport,
            },
            TransportMulticastPeer {
                manager: peer02_manager,
                handler: peer02_handler,
                transport: peer02_transport,
            },
        )
    }

    async fn close_transport(peer01: TransportMulticastPeer, peer02: TransportMulticastPeer) {
        println!("Closing peer01 transport");
        ztimeout!(peer01.transport.close()).unwrap();
        assert!(ztimeout!(peer01.manager.get_transports_multicast()).is_empty());
        ztimeout!(async {
            while !peer02.transport.get_peers().unwrap().is_empty() {
                tokio::time::sleep(SLEEP_COUNT).await;
            }
        });

        println!("Closing peer02 transport");
        ztimeout!(peer02.transport.close()).unwrap();
        assert!(ztimeout!(peer02.manager.get_transports_multicast()).is_empty());

        tokio::time::sleep(SLEEP).await;
    }

    /// A payload with structure, so a reassembly that puts the right number of
    /// bytes back in the wrong order is caught. All-zeros would not be.
    fn pattern(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i % 251) as u8).collect()
    }

    async fn test_transport(
        peer01: &TransportMulticastPeer,
        peer02: &TransportMulticastPeer,
        channel: Channel,
        payload: &[u8],
    ) {
        let message = NetworkMessage::from(Push {
            wire_expr: "test".into(),
            ext_qos: QoSType::new(channel.priority, CongestionControl::Block, false),
            ..Push::from(payload.to_vec())
        });

        println!(
            "Sending {MSG_COUNT} messages of {} bytes... {channel:?}",
            payload.len()
        );
        for _ in 0..MSG_COUNT {
            peer01.transport.schedule(message.clone().as_mut()).unwrap();
        }

        // Wait for delivery to settle rather than stopping at the first
        // message. Sampling until the count holds still gives a figure that
        // means something; stopping at `count != 0` reports how much happened
        // to have arrived by then, which is not a measurement of anything.
        ztimeout!(async {
            let mut last = usize::MAX;
            let mut stable = 0;
            loop {
                let now = peer02.handler.get_count();
                if now == MSG_COUNT {
                    break;
                }
                if now == last {
                    stable += 1;
                    // A full second with no new message.
                    if stable >= 100 {
                        break;
                    }
                } else {
                    stable = 0;
                    last = now;
                }
                tokio::time::sleep(SLEEP_COUNT).await;
            }
        });

        let received = peer02.handler.get_count();
        let corrupt = peer02.handler.get_corrupt();
        println!(
            "\tPeer02 received {received}/{MSG_COUNT} messages of {} bytes, {corrupt} corrupt",
            payload.len()
        );

        // The link is best-effort by nature -- CAN is reliable per frame but not
        // end to end -- so the contract is that traffic arrives, not that all of
        // it does. This mirrors the UDP multicast test.
        assert!(received > 0, "nothing arrived at all");
        // Whatever does arrive must be intact. A message reassembled from CAN
        // frames is either right or it is a bug; there is no partial credit.
        assert_eq!(corrupt, 0, "{corrupt} of {received} messages were corrupt");

        tokio::time::sleep(SLEEP).await;
    }

    async fn run(endpoints: (&EndPoint, &EndPoint), channel: &[Channel], msg_size: &[usize]) {
        run_qos(endpoints, channel, msg_size, false).await
    }

    async fn run_qos(
        endpoints: (&EndPoint, &EndPoint),
        channel: &[Channel],
        msg_size: &[usize],
        qos: bool,
    ) {
        for ch in channel.iter() {
            for ms in msg_size.iter() {
                let payload = pattern(*ms);
                let (peer01, peer02) =
                    open_transport_qos(endpoints.0, endpoints.1, &payload, qos).await;
                test_transport(&peer01, &peer02, *ch, &payload).await;
                close_transport(peer01, peer02).await;
            }
        }
    }

    fn channels() -> [Channel; 2] {
        [
            Channel {
                priority: Priority::DEFAULT,
                reliability: Reliability::BestEffort,
            },
            Channel {
                priority: Priority::RealTime,
                reliability: Reliability::BestEffort,
            },
        ]
    }

    /// Two zenoh-rs peers exchange a payload that does not fit one
    /// frame, so the transport's own fragmentation is driving the link.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[ignore = "needs a vcan0 interface; see the module docs"]
    async fn transport_multicast_can_fragmented() {
        zenoh_util::init_log_from_env_or("error");
        if !vcan_present() {
            return;
        }

        let e01: EndPoint = format!("can/{DEVICE}#id=0x100").parse().unwrap();
        let e02: EndPoint = format!("can/{DEVICE}#id=0x101").parse().unwrap();
        run((&e01, &e02), &channels(), &MSG_SIZE_FRAGMENTED).await;
    }

    /// The same, well past a single batch.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[ignore = "needs a vcan0 interface; see the module docs"]
    async fn transport_multicast_can_large() {
        zenoh_util::init_log_from_env_or("error");
        if !vcan_present() {
            return;
        }

        // 100 messages of 4 KiB is 7 100 frames, and a virtual bus delivers them
        // as fast as memory allows -- far faster than any real bus, where
        // 2 Mbit/s of CAN FD is under 2 800 frames per second. Without a bigger
        // receive buffer the kernel drops the overflow before the link sees it,
        // and 31% of messages are lost with no error anywhere. This is the
        // knob for that, and this test is its demonstration.
        let e01: EndPoint = format!("can/{DEVICE}#id=0x110;so_rcvbuf=8388608")
            .parse()
            .unwrap();
        let e02: EndPoint = format!("can/{DEVICE}#id=0x111;so_rcvbuf=8388608")
            .parse()
            .unwrap();
        run((&e01, &e02), &channels()[..1], &MSG_SIZE_LARGE).await;
    }

    /// With `prio_bits` set, one peer transmits under several
    /// identifiers -- one per traffic class -- and every one of them must still
    /// resolve to that single peer. If it did not, the transport would see
    /// eight peers where there is one, and a peer would hear its own frames.
    ///
    /// What this test cannot show is the point of the feature. `vcan` has no
    /// arbitration: frames are queued, not contended for, so an urgent message
    /// overtaking a bulk burst is not observable here at all. That needs a real
    /// bus. What is checked here is that the addressing
    /// survives, which is the part that can be got wrong in software.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[ignore = "needs a vcan0 interface; see the module docs"]
    async fn transport_multicast_can_priority_major_ids() {
        zenoh_util::init_log_from_env_or("error");
        if !vcan_present() {
            return;
        }

        // Three class bits leave eight for the peer, so the identifiers are
        // 0x?0A and 0x?0B with the class in the top nibble.
        let e01: EndPoint = format!("can/{DEVICE}#id=0x0A;prio_bits=3;so_rcvbuf=8388608")
            .parse()
            .unwrap();
        let e02: EndPoint = format!("can/{DEVICE}#id=0x0B;prio_bits=3;so_rcvbuf=8388608")
            .parse()
            .unwrap();

        // Both a high and a low priority, so more than one class is actually
        // put on the wire and both have to arrive.
        let channel = [
            Channel {
                priority: Priority::RealTime,
                reliability: Reliability::BestEffort,
            },
            Channel {
                priority: Priority::Background,
                reliability: Reliability::BestEffort,
            },
        ];
        // NOTE: QoS stays OFF here, so the identifiers do not in fact leave
        // class 0 -- see `join_with_qos_does_not_fit_one_can_frame` below for
        // why turning it on cannot work on CAN FD. What this test does prove is
        // that the priority-major layout has not broken addressing: peers still
        // find each other and every message arrives intact when the identifier
        // is split into a class field and a peer field.
        run_qos((&e01, &e02), &channel, &MSG_SIZE_FRAGMENTED, false).await;
    }

    /// The blocker: **a `Join` carrying per-priority sequence
    /// numbers does not fit one CAN FD frame**, so the traffic-class field can
    /// never be exercised on this medium as the protocol stands.
    ///
    /// Mapping zenoh priority onto the identifier only does anything when the
    /// multicast transport keeps one queue per priority, because otherwise
    /// every batch reports priority index 0. Turning that on makes `Join` carry
    /// eight `PrioritySn` instead of one -- and `Join` is written as a single
    /// datagram with no fragmentation, so on a 63-byte link the transmit task
    /// dies before the session starts.
    ///
    /// This test needs no bus. It measures both encodings so the margin is a
    /// number, and it will fail loudly if the `Join` ever shrinks enough to fit
    /// -- which is the day this feature becomes usable.
    #[test]
    fn join_with_qos_does_not_fit_one_can_frame() {
        use zenoh_buffers::writer::HasWriter;
        use zenoh_codec::{WCodec, Zenoh080};
        use zenoh_protocol::{
            core::{Resolution, WhatAmI, ZenohIdProto},
            transport::{join::ext::PatchType, Join, PrioritySn, TransportMessage},
        };

        const CAN_FD_MTU: usize = 63;

        fn encoded_len(ext_qos: Option<Box<[PrioritySn; Priority::NUM]>>) -> usize {
            // Sequence numbers near the top of the resolution, which is what a
            // randomly seeded session actually starts with, so the varints are
            // their realistic width rather than one byte.
            let sn = PrioritySn {
                reliable: 0x0FFF_FFFF,
                best_effort: 0x0FFF_FFFF,
            };
            let msg: TransportMessage = Join {
                version: 0x09,
                whatami: WhatAmI::Peer,
                zid: ZenohIdProto::rand(),
                resolution: Resolution::default(),
                batch_size: CAN_FD_MTU as u16,
                lease: Duration::from_secs(60),
                next_sn: sn,
                ext_qos,
                ext_shm: None,
                ext_patch: PatchType::CURRENT,
            }
            .into();

            let mut buf = Vec::new();
            let codec = Zenoh080::new();
            codec.write(&mut buf.writer(), &msg).unwrap();
            buf.len()
        }

        let without = encoded_len(None);
        let with = encoded_len(Some(Box::new(
            [PrioritySn {
                reliable: 0x0FFF_FFFF,
                best_effort: 0x0FFF_FFFF,
            }; Priority::NUM],
        )));

        println!(
            "Join without QoS: {without} bytes; with QoS: {with} bytes; CAN FD MTU {CAN_FD_MTU}"
        );

        assert!(
            without <= CAN_FD_MTU,
            "a plain Join must fit, or the link could never work at all: {without} > {CAN_FD_MTU}"
        );
        assert!(
            with > CAN_FD_MTU,
            "Join with per-priority SNs now fits ({with} <= {CAN_FD_MTU}); \
             W7 may be unblocked -- re-test with multicast QoS enabled"
        );
    }

    /// Four peers on one bus, which is the first test that is not a pair.
    ///
    /// It exercises the thing a pair cannot: each peer has to track three
    /// distinct remotes, told apart only by the identifier in the frames they
    /// send. A bug in the peer-address derivation is invisible with two peers
    /// and obvious with four.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[ignore = "needs a vcan0 interface; see the module docs"]
    async fn transport_multicast_can_four_peers() {
        zenoh_util::init_log_from_env_or("error");
        if !vcan_present() {
            return;
        }

        const PEERS: usize = 4;
        let payload = pattern(189);

        let mut managers = Vec::new();
        let mut handlers = Vec::new();
        for i in 0..PEERS {
            let handler = Arc::new(SHPeer::new(payload.clone()));
            let manager = TransportManager::builder()
                .zid(ZenohIdProto::try_from([(i + 1) as u8]).unwrap())
                .whatami(WhatAmI::Peer)
                .build_test(handler.clone())
                .unwrap();
            let ep: EndPoint = format!("can/{DEVICE}#id=0x{:x};so_rcvbuf=8388608", 0x140 + i)
                .parse()
                .unwrap();
            ztimeout!(manager.open_transport_multicast(ep)).unwrap();
            managers.push(manager);
            handlers.push(handler);
        }

        // Every peer must see the other three, by identifier alone.
        // Convergence takes one `join_interval` (2.5 s by default), not the
        // instant a peer opens: a peer only learns about those that were
        // already there when the next periodic Join comes round. Immediately
        // after opening, the counts are a staircase by open order.
        ztimeout!(async {
            loop {
                let mut counts = Vec::new();
                for manager in managers.iter() {
                    let ts = manager.get_transports_multicast().await;
                    counts.push(
                        ts.first()
                            .map(|t| t.get_peers().unwrap().len())
                            .unwrap_or(0),
                    );
                }
                if counts.iter().all(|c| *c == PEERS - 1) {
                    println!("\tevery peer sees the other {}", PEERS - 1);
                    break;
                }
                tokio::time::sleep(SLEEP_COUNT).await;
            }
        });

        // Exactly the others, never itself and never a duplicate. A stray peer
        // left on the bus by an earlier run shows up here as an extra, which is
        // worth failing on rather than tolerating.
        for (i, manager) in managers.iter().enumerate() {
            let ts = ztimeout!(manager.get_transports_multicast());
            let peers = ts.first().unwrap().get_peers().unwrap();
            assert_eq!(
                peers.len(),
                PEERS - 1,
                "peer {i} sees {} remotes, expected {}; is something else on the bus?",
                peers.len(),
                PEERS - 1
            );
        }

        // One publisher, three subscribers.
        let message = NetworkMessage::from(Push {
            wire_expr: "test".into(),
            ext_qos: QoSType::new(Priority::DEFAULT, CongestionControl::Block, false),
            ..Push::from(payload.clone())
        });
        let publisher = ztimeout!(managers[0].get_transports_multicast())
            .first()
            .unwrap()
            .clone();
        for _ in 0..MSG_COUNT {
            publisher.schedule(message.clone().as_mut()).unwrap();
        }

        for (i, handler) in handlers.iter().enumerate().skip(1) {
            ztimeout!(async {
                while handler.get_count() < MSG_COUNT {
                    tokio::time::sleep(SLEEP_COUNT).await;
                }
            });
            println!(
                "\tPeer {i} received {}/{MSG_COUNT}, {} corrupt",
                handler.get_count(),
                handler.get_corrupt()
            );
            assert_eq!(handler.get_corrupt(), 0);
        }

        // The publisher must not have heard itself.
        assert_eq!(handlers[0].get_count(), 0, "a peer heard its own frames");

        for manager in managers {
            for t in ztimeout!(manager.get_transports_multicast()) {
                ztimeout!(t.close()).unwrap();
            }
        }
        tokio::time::sleep(SLEEP).await;
    }

    /// The link reports the MTU of the mode it actually obtained,
    /// and a band that excludes a peer's own identifier is refused at open.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[ignore = "needs a vcan0 interface; see the module docs"]
    async fn can_link_open_reports_its_mode() {
        zenoh_util::init_log_from_env_or("error");
        if !vcan_present() {
            return;
        }

        let handler = Arc::new(SHPeer::new(Vec::new()));
        let manager = TransportManager::builder()
            .zid(ZenohIdProto::try_from([3]).unwrap())
            .whatami(WhatAmI::Peer)
            .build_test(handler)
            .unwrap();

        let ep: EndPoint = format!("can/{DEVICE}#id=0x120").parse().unwrap();
        let transport = ztimeout!(manager.open_transport_multicast(ep)).unwrap();
        let mtu = transport.get_link().unwrap().mtu;
        // 63 with CAN FD, 7 if the interface refused it. A virtual bus supports
        // FD, so anything else means the negotiation regressed.
        assert_eq!(mtu, 63, "vcan0 should negotiate CAN FD");
        ztimeout!(transport.close()).unwrap();

        // An identifier outside its own band could never be addressed, so the
        // link refuses it rather than degrading quietly.
        let bad: EndPoint = format!("can/{DEVICE}#id=0x201;match=0x100;mask=0x700")
            .parse()
            .unwrap();
        let err = ztimeout!(manager.open_transport_multicast(bad)).unwrap_err();
        assert!(
            err.to_string().contains("outside its own"),
            "unexpected error: {err}"
        );

        // An over-long interface name is refused before open, with the limit
        // named, rather than truncating into some other interface.
        let long: EndPoint = "can/vcan-nonexistent".parse().unwrap();
        let err = ztimeout!(manager.open_transport_multicast(long)).unwrap_err();
        assert!(
            err.to_string().contains("at most 15"),
            "unexpected error: {err}"
        );

        // No interface of that name, and the error should say so. The name is
        // kept under IFNAMSIZ so this exercises the missing-interface path
        // rather than the name-length guard.
        let missing: EndPoint = "can/vcan9nope".parse().unwrap();
        let err = ztimeout!(manager.open_transport_multicast(missing)).unwrap_err();
        assert!(
            err.to_string().contains("no such interface"),
            "unexpected error: {err}"
        );
    }
}
