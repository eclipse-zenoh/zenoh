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

use async_trait::async_trait;
use zenoh_link_commons::{LinkManagerMulticastTrait, LinkMulticast};
use zenoh_protocol::core::EndPoint;
use zenoh_result::ZResult;

/// A CAN bus is a broadcast medium, so peers do not pair off: they all listen
/// and filter by identifier. There is no connect side and no accept.
#[derive(Debug, Default)]
pub struct LinkManagerMulticastCan;

#[async_trait]
impl LinkManagerMulticastTrait for LinkManagerMulticastCan {
    async fn new_link(&self, endpoint: &EndPoint) -> ZResult<LinkMulticast> {
        // Parse and validate on every platform, so a bad endpoint is reported
        // as a bad endpoint rather than as a missing platform.
        let ep = crate::CanEndpoint::parse(endpoint)?;
        new_link_inner(ep).await
    }
}

#[cfg(target_os = "linux")]
async fn new_link_inner(ep: crate::CanEndpoint) -> ZResult<LinkMulticast> {
    let link = imp::LinkMulticastCan::new(ep)?;
    Ok(LinkMulticast(std::sync::Arc::new(link)))
}

#[cfg(not(target_os = "linux"))]
async fn new_link_inner(ep: crate::CanEndpoint) -> ZResult<LinkMulticast> {
    zenoh_result::bail!(
        "CAN links need SocketCAN, which is a Linux kernel interface; \
         cannot open {:?} on this platform",
        ep.device
    )
}

#[cfg(target_os = "linux")]
mod imp {
    use std::{borrow::Cow, fmt};

    use async_trait::async_trait;
    use zenoh_link_commons::{LinkAuthId, LinkMulticastTrait};
    use zenoh_protocol::{
        core::{Locator, Priority},
        transport::BatchSize,
    };
    use zenoh_result::ZResult;

    use crate::{sys::CanSocket, CanEndpoint};

    pub(super) struct LinkMulticastCan {
        socket: CanSocket,
        /// This peer's own address on the bus.
        src_locator: Locator,
        /// The identifier band this link listens to, which is what the
        /// transport manager keys the multicast transport by.
        group_locator: Locator,
        endpoint: CanEndpoint,
    }

    impl LinkMulticastCan {
        pub(super) fn new(endpoint: CanEndpoint) -> ZResult<LinkMulticastCan> {
            let socket = CanSocket::open(&endpoint)?;
            let src_locator = endpoint.peer_locator(endpoint.id);
            let group_locator = endpoint.group_locator();

            tracing::debug!(
                "CAN link on {:?}: id {:#x}, band {:#x}/{:#x}, MTU {}",
                endpoint.device,
                endpoint.id,
                endpoint.filter_match,
                endpoint.filter_mask,
                socket.mtu()
            );

            Ok(LinkMulticastCan {
                socket,
                src_locator,
                group_locator,
                endpoint,
            })
        }
    }

    #[async_trait]
    impl LinkMulticastTrait for LinkMulticastCan {
        fn get_mtu(&self) -> BatchSize {
            self.socket.mtu()
        }

        fn get_src(&self) -> &Locator {
            &self.src_locator
        }

        fn get_dst(&self) -> &Locator {
            &self.group_locator
        }

        fn get_auth_id(&self) -> &LinkAuthId {
            &LinkAuthId::Can
        }

        /// CAN is reliable at frame level -- CRC, ACK slot, automatic
        /// retransmission -- but not end to end: controller buffers overrun and
        /// a bus-off condition drops everything.
        fn is_reliable(&self) -> bool {
            crate::IS_RELIABLE
        }

        async fn write(&self, buffer: &[u8]) -> ZResult<usize> {
            self.socket.send(buffer, Priority::DEFAULT as u8).await
        }

        /// A datagram link writes one frame or fails; there is no partial write
        /// to loop over. zenoh's transport never hands the link more than its
        /// MTU, because the transmission pipeline clamps to it and fragments
        /// above it.
        async fn write_all(&self, buffer: &[u8]) -> ZResult<()> {
            self.socket
                .send(buffer, Priority::DEFAULT as u8)
                .await
                .map(|_| ())
        }

        /// A CAN identifier **is** the bus priority, so this is the one link
        /// where the batch's priority belongs on the wire. With `prio_bits=0`,
        /// the default, it changes nothing and the frames stay exactly what
        /// zenoh-pico speaks.
        async fn write_all_with_priority(&self, buffer: &[u8], priority: Priority) -> ZResult<()> {
            self.socket.send(buffer, priority as u8).await.map(|_| ())
        }

        async fn read<'a>(&'a self, buffer: &mut [u8]) -> ZResult<(usize, Cow<'a, Locator>)> {
            let (n, sender) = self.socket.recv(buffer).await?;
            Ok((n, Cow::Owned(self.endpoint.peer_locator(sender))))
        }

        async fn close(&self) -> ZResult<()> {
            // The socket is closed when the link is dropped; a CAN bus has no
            // group to leave and no connection to shut down.
            tracing::trace!("Closing CAN link: {self}");
            Ok(())
        }
    }

    impl fmt::Display for LinkMulticastCan {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "{} => {}", self.src_locator, self.group_locator)
        }
    }

    impl fmt::Debug for LinkMulticastCan {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("Can")
                .field("device", &self.endpoint.device)
                .field("id", &self.endpoint.id)
                .field("match", &self.endpoint.filter_match)
                .field("mask", &self.endpoint.filter_mask)
                .field("mtu", &self.socket.mtu())
                .finish()
        }
    }
}
