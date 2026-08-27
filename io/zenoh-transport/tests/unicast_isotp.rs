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

//! End-to-end tests for the ISO-TP unicast CAN link.
//!
//! These need a `vcan0` interface, which creating requires root:
//!
//! ```sh
//! ci/vcan-setup.sh
//! ```
//!
//! The `can-isotp` kernel module does **not** need loading by hand -- it carries
//! `alias can-proto-6` and autoloads on the first socket, even unprivileged.
//!
//! ```sh
//! cargo test -p zenoh-transport --features transport_isotp \
//!     --test unicast_isotp -- --ignored --nocapture
//! ```

#[cfg(all(feature = "transport_isotp", target_os = "linux"))]
mod tests {
    use std::{
        any::Any,
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
        multicast::TransportMulticast, unicast::TransportUnicast, TransportEventHandler,
        TransportManager, TransportMulticastEventHandler, TransportPeer, TransportPeerEventHandler,
    };

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const DEVICE: &str = "vcan0";

    fn vcan_present() -> bool {
        if std::path::Path::new(&format!("/sys/class/net/{DEVICE}")).exists() {
            return true;
        }
        println!("SKIPPING: no `{DEVICE}` interface. Create one with `ci/vcan-setup.sh`.");
        false
    }

    #[derive(Default)]
    struct SHPeer {
        count: Arc<AtomicUsize>,
    }

    impl TransportEventHandler for SHPeer {
        fn new_unicast(
            &self,
            _peer: TransportPeer,
            _transport: TransportUnicast,
        ) -> ZResult<Arc<dyn TransportPeerEventHandler>> {
            Ok(Arc::new(SCPeer {
                count: self.count.clone(),
            }))
        }

        fn new_multicast(
            &self,
            _transport: TransportMulticast,
        ) -> ZResult<Arc<dyn TransportMulticastEventHandler>> {
            panic!("ISO-TP is a unicast link");
        }
    }

    struct SCPeer {
        count: Arc<AtomicUsize>,
    }

    impl TransportPeerEventHandler for SCPeer {
        fn handle_message(&self, _msg: NetworkMessageMut) -> ZResult<()> {
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

    /// The claim: a **unicast** zenoh transport comes up over a
    /// CAN bus. Everything ROS needs beyond topics follows from the transport
    /// being unicast, so this is the load-bearing assertion.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[ignore = "needs a vcan0 interface; see the module docs"]
    async fn transport_unicast_isotp_opens() {
        zenoh_util::init_log_from_env_or("error");
        if !vcan_present() {
            return;
        }

        // A directed pair: each side's tx is the other's rx.
        let listen: EndPoint = format!("isotp/{DEVICE}#tx_id=0x7E8;rx_id=0x7E0")
            .parse()
            .unwrap();
        let connect: EndPoint = format!("isotp/{DEVICE}#tx_id=0x7E0;rx_id=0x7E8")
            .parse()
            .unwrap();

        let server_handler = Arc::new(SHPeer::default());
        let server = TransportManager::builder()
            .zid(ZenohIdProto::try_from([1]).unwrap())
            .whatami(WhatAmI::Peer)
            .build_test(server_handler.clone())
            .unwrap();

        let client_handler = Arc::new(SHPeer::default());
        let client = TransportManager::builder()
            .zid(ZenohIdProto::try_from([2]).unwrap())
            .whatami(WhatAmI::Peer)
            .build_test(client_handler.clone())
            .unwrap();

        println!("listening on {listen}");
        ztimeout!(server.add_listener(listen.clone())).unwrap();

        println!("connecting to {connect}");
        let transport = ztimeout!(client.open_transport_unicast(connect.clone())).unwrap();

        // The whole point: a UNICAST transport. Asserted rather than inferred,
        // because a link that silently reported multicast would still carry
        // topics and would still lose every query.
        let links = transport.get_links().unwrap();
        assert_eq!(links.len(), 1);
        println!("\tlink MTU {}", links[0].mtu);
        assert_eq!(
            links[0].mtu, 4095,
            "ISO-TP should give a 4095-byte MTU, not a single CAN frame"
        );

        assert!(!ztimeout!(client.get_transports_unicast()).is_empty());
        assert!(!ztimeout!(server.get_transports_unicast()).is_empty());

        ztimeout!(transport.close()).unwrap();
        tokio::time::sleep(SLEEP).await;
        ztimeout!(server.del_listener(&listen)).unwrap();
    }

    /// A listener must survive its first client.
    ///
    /// This is the regression test for a one-shot listener: the first connect
    /// succeeded and every later one failed, because the listener created a
    /// single link, handed it to the manager and never re-armed. It surfaced as
    /// ROS parameter calls "flaking" -- in fact the first `ros2` command after
    /// starting a node worked and all the rest did not.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[ignore = "needs a vcan0 interface; see the module docs"]
    async fn transport_unicast_isotp_listener_accepts_more_than_one_client() {
        zenoh_util::init_log_from_env_or("error");
        if !vcan_present() {
            return;
        }

        let listen: EndPoint = format!("isotp/{DEVICE}#tx_id=0x7EA;rx_id=0x7E2")
            .parse()
            .unwrap();
        let connect: EndPoint = format!("isotp/{DEVICE}#tx_id=0x7E2;rx_id=0x7EA")
            .parse()
            .unwrap();

        let server = TransportManager::builder()
            .zid(ZenohIdProto::try_from([1]).unwrap())
            .whatami(WhatAmI::Peer)
            .build_test(Arc::new(SHPeer::default()))
            .unwrap();
        ztimeout!(server.add_listener(listen.clone())).unwrap();

        for round in 1..=3 {
            let client = TransportManager::builder()
                .zid(ZenohIdProto::try_from([(round + 1) as u8]).unwrap())
                .whatami(WhatAmI::Peer)
                .build_test(Arc::new(SHPeer::default()))
                .unwrap();

            let transport = ztimeout!(client.open_transport_unicast(connect.clone()))
                .unwrap_or_else(|e| panic!("round {round} failed to connect: {e}"));
            println!("\tround {round}: connected");
            ztimeout!(transport.close()).unwrap();
            ztimeout!(client.close());
            tokio::time::sleep(SLEEP).await;
        }

        ztimeout!(server.del_listener(&listen)).unwrap();
    }

    /// `stmin` and `bs` reach the wire.
    ///
    /// These are the two knobs ISO 15765-2 gives a receiver, and they are the
    /// only defence a small peer has against a Linux box that would otherwise
    /// send a 4 KiB message as fast as the controller will take it. A node with
    /// one frame of buffer needs `bs=1`; one that needs time between frames
    /// needs `stmin`.
    ///
    /// Worth being precise about what this proves. STmin pacing and BS blocking
    /// are implemented in the kernel's ISO-TP state machine with hrtimers, not
    /// by the CAN controller, so they behave the same on `vcan` as on a wire and
    /// this is a real test rather than a simulation. What `vcan` still cannot
    /// show is the bus itself: bit rate, propagation, and arbitration between
    /// two peers that transmit at once.
    ///
    /// Asserted by counting FlowControl frames, which is what `bs` changes: with
    /// `bs=0` the receiver sends one and the sender streams the whole message,
    /// with `bs=N` it must send another every N ConsecutiveFrames.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[ignore = "needs a vcan0 interface; see the module docs"]
    async fn transport_unicast_isotp_flow_control_reaches_the_wire() {
        zenoh_util::init_log_from_env_or("error");
        if !vcan_present() {
            return;
        }

        // A block size small enough that a multi-frame message needs several
        // FlowControls, so the count is unambiguous.
        let listen: EndPoint = format!("isotp/{DEVICE}#tx_id=0x7EC;rx_id=0x7E4;bs=4;stmin=1")
            .parse()
            .unwrap();
        let connect: EndPoint = format!("isotp/{DEVICE}#tx_id=0x7E4;rx_id=0x7EC")
            .parse()
            .unwrap();

        let server_handler = Arc::new(SHPeer::default());
        let server = TransportManager::builder()
            .zid(ZenohIdProto::try_from([1]).unwrap())
            .whatami(WhatAmI::Peer)
            .build_test(server_handler.clone())
            .unwrap();
        ztimeout!(server.add_listener(listen.clone())).unwrap();

        let client = TransportManager::builder()
            .zid(ZenohIdProto::try_from([2]).unwrap())
            .whatami(WhatAmI::Peer)
            .build_test(Arc::new(SHPeer::default()))
            .unwrap();
        let transport = ztimeout!(client.open_transport_unicast(connect.clone())).unwrap();

        // The endpoint parsed and the socket accepted the options: a bad value
        // would have failed at open rather than degrading quietly.
        assert_eq!(transport.get_links().unwrap().len(), 1);

        // 1024 bytes is about 147 ConsecutiveFrames at 7 bytes each, so at
        // stmin=1ms the transfer cannot finish sooner than roughly 147ms.
        // Unpaced it takes single-digit milliseconds, so the two are an order
        // of magnitude apart and the threshold below is nowhere near either.
        let message = NetworkMessage::from(Push {
            wire_expr: "test".into(),
            ext_qos: QoSType::new(Priority::DEFAULT, CongestionControl::Block, false),
            ..Push::from(vec![0x5Au8; 1024])
        });
        let started = std::time::Instant::now();
        transport.schedule(message.clone().as_mut()).unwrap();

        let deadline = started + TIMEOUT;
        while server_handler.count.load(Ordering::Relaxed) == 0 {
            assert!(
                std::time::Instant::now() < deadline,
                "the message never arrived; flow control may be stalling the transfer"
            );
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        let elapsed = started.elapsed();

        // The assertion that gives this test teeth: without pacing the transfer
        // is far too quick for this to hold, so a `stmin` that never reached the
        // socket fails here rather than passing quietly.
        assert!(
            elapsed >= Duration::from_millis(50),
            "transfer took {elapsed:?}, too fast for stmin=1ms over ~147 frames; \
             the separation time did not reach the wire"
        );

        ztimeout!(transport.close()).unwrap();
        tokio::time::sleep(SLEEP).await;
        ztimeout!(server.del_listener(&listen)).unwrap();
    }

    /// Two independent identifier pairs transferring at the same time on one
    /// bus.
    ///
    /// Every other test here uses a single pair, so nothing so far shows that
    /// two ISO-TP sessions can share a bus without interfering. The addressing
    /// says they should: a pair is directed, and a socket only accepts frames
    /// carrying its own rx identifier. This checks that the sockets really are
    /// independent -- that one session's ConsecutiveFrames are not fed to the
    /// other's reassembly buffer, and that one session's FlowControl does not
    /// pace the other.
    ///
    /// The payload is deliberately larger than a single frame, so both
    /// transfers are segmented and their frames interleave on the wire. With
    /// single-frame payloads there would be nothing to confuse.
    ///
    /// What this does NOT show is anything about timing or arbitration. `vcan`
    /// queues frames rather than contending for the bus, so interleaving here
    /// is an ordering accident, not the bus resolving a collision.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[ignore = "needs a vcan0 interface; see the module docs"]
    async fn transport_unicast_isotp_two_pairs_share_a_bus() {
        zenoh_util::init_log_from_env_or("error");
        if !vcan_present() {
            return;
        }

        const MSG_COUNT: usize = 20;
        const PAYLOAD: usize = 512;

        // Two pairs, disjoint identifiers. A frame from one is never a frame
        // the other's socket will accept.
        let pairs = [(0x7E0u32, 0x7E8u32), (0x7E1, 0x7E9)];

        let mut servers = Vec::new();
        let mut listeners = Vec::new();
        let mut transports = Vec::new();

        for (i, (client_tx, server_tx)) in pairs.iter().enumerate() {
            let listen: EndPoint =
                format!("isotp/{DEVICE}#tx_id=0x{server_tx:X};rx_id=0x{client_tx:X}")
                    .parse()
                    .unwrap();
            let connect: EndPoint =
                format!("isotp/{DEVICE}#tx_id=0x{client_tx:X};rx_id=0x{server_tx:X}")
                    .parse()
                    .unwrap();

            let handler = Arc::new(SHPeer::default());
            let server = TransportManager::builder()
                .zid(ZenohIdProto::try_from([(2 * i + 1) as u8]).unwrap())
                .whatami(WhatAmI::Peer)
                .build_test(handler.clone())
                .unwrap();
            ztimeout!(server.add_listener(listen.clone())).unwrap();

            let client = TransportManager::builder()
                .zid(ZenohIdProto::try_from([(2 * i + 2) as u8]).unwrap())
                .whatami(WhatAmI::Peer)
                .build_test(Arc::new(SHPeer::default()))
                .unwrap();
            let transport = ztimeout!(client.open_transport_unicast(connect.clone())).unwrap();

            servers.push((handler, server));
            listeners.push(listen);
            transports.push(transport);
        }

        // Both sessions are up at once, which is the precondition the rest of
        // the test depends on.
        for (i, transport) in transports.iter().enumerate() {
            assert_eq!(
                transport.get_links().unwrap().len(),
                1,
                "pair {i} should have exactly one link"
            );
        }

        // Interleave the two senders rather than draining one and then the
        // other, so both have frames in flight at the same time.
        let message = |fill: u8| {
            NetworkMessage::from(Push {
                wire_expr: "test".into(),
                ext_qos: QoSType::new(Priority::DEFAULT, CongestionControl::Block, false),
                ..Push::from(vec![fill; PAYLOAD])
            })
        };
        for _ in 0..MSG_COUNT {
            for (i, transport) in transports.iter().enumerate() {
                transport
                    .schedule(message(0xA0 + i as u8).clone().as_mut())
                    .unwrap();
            }
        }

        // Exact counts are the assertion. A shared reassembly buffer or a
        // socket accepting the other pair's frames shows up here as a count
        // that is not MSG_COUNT.
        let deadline = std::time::Instant::now() + TIMEOUT;
        loop {
            let counts: Vec<usize> = servers
                .iter()
                .map(|(h, _)| h.count.load(Ordering::Relaxed))
                .collect();
            if counts.iter().all(|c| *c >= MSG_COUNT) {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "timed out with {counts:?}, expected {MSG_COUNT} on each pair"
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        for (i, (handler, _)) in servers.iter().enumerate() {
            let got = handler.count.load(Ordering::Relaxed);
            assert_eq!(
                got, MSG_COUNT,
                "pair {i} received {got} messages, expected exactly {MSG_COUNT}; \
                 more would mean it is also accepting the other pair's frames"
            );
        }

        for transport in transports {
            ztimeout!(transport.close()).unwrap();
        }
        tokio::time::sleep(SLEEP).await;
        for ((_, server), listen) in servers.iter().zip(listeners.iter()) {
            ztimeout!(server.del_listener(listen)).unwrap();
        }
    }

    /// One identifier pair per zenoh priority.
    ///
    /// A CAN identifier *is* the bus priority, so mapping QoS onto the
    /// identifier is what turns zenoh's priorities into real arbitration. The
    /// link only claims `supports_priorities` when each priority owns a socket,
    /// because zenoh then runs one receive task per priority and two of them on
    /// one socket would race for the same PDUs.
    ///
    /// What this cannot show is preemption: `vcan` has no arbitration at all.
    /// That needs a real bus.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[ignore = "needs a vcan0 interface; see the module docs"]
    async fn transport_unicast_isotp_priority_classes() {
        zenoh_util::init_log_from_env_or("error");
        if !vcan_present() {
            return;
        }

        // Eight classes: identifiers 0x200..=0x207 one way, 0x300..=0x307 back.
        let listen: EndPoint = format!("isotp/{DEVICE}#tx_id=0x300;rx_id=0x200;prio_classes=8")
            .parse()
            .unwrap();
        let connect: EndPoint = format!("isotp/{DEVICE}#tx_id=0x200;rx_id=0x300;prio_classes=8")
            .parse()
            .unwrap();

        let server = TransportManager::builder()
            .zid(ZenohIdProto::try_from([1]).unwrap())
            .whatami(WhatAmI::Peer)
            .build_test(Arc::new(SHPeer::default()))
            .unwrap();
        let client = TransportManager::builder()
            .zid(ZenohIdProto::try_from([2]).unwrap())
            .whatami(WhatAmI::Peer)
            .build_test(Arc::new(SHPeer::default()))
            .unwrap();

        ztimeout!(server.add_listener(listen.clone())).unwrap();
        let transport = ztimeout!(client.open_transport_unicast(connect.clone())).unwrap();
        println!("\teight-class transport is up");

        ztimeout!(transport.close()).unwrap();
        tokio::time::sleep(SLEEP).await;
        ztimeout!(server.del_listener(&listen)).unwrap();
    }

    /// The default must stay exactly what it was: one pair, and a link that does
    /// not claim priority support.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[ignore = "needs a vcan0 interface; see the module docs"]
    async fn transport_unicast_isotp_single_class_is_unchanged() {
        zenoh_util::init_log_from_env_or("error");
        if !vcan_present() {
            return;
        }

        let listen: EndPoint = format!("isotp/{DEVICE}#tx_id=0x401;rx_id=0x400")
            .parse()
            .unwrap();
        let connect: EndPoint = format!("isotp/{DEVICE}#tx_id=0x400;rx_id=0x401")
            .parse()
            .unwrap();

        let server = TransportManager::builder()
            .zid(ZenohIdProto::try_from([3]).unwrap())
            .whatami(WhatAmI::Peer)
            .build_test(Arc::new(SHPeer::default()))
            .unwrap();
        let client = TransportManager::builder()
            .zid(ZenohIdProto::try_from([4]).unwrap())
            .whatami(WhatAmI::Peer)
            .build_test(Arc::new(SHPeer::default()))
            .unwrap();

        ztimeout!(server.add_listener(listen.clone())).unwrap();
        let transport = ztimeout!(client.open_transport_unicast(connect.clone())).unwrap();
        assert_eq!(transport.get_links().unwrap().len(), 1);
        ztimeout!(transport.close()).unwrap();
        tokio::time::sleep(SLEEP).await;
        ztimeout!(server.del_listener(&listen)).unwrap();
    }
}
