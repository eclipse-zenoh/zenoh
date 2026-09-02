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
use zenoh_link_commons::{LinkManagerUnicastTrait, LinkUnicast, NewLinkChannelSender};
use zenoh_protocol::core::{EndPoint, Locator};
use zenoh_result::{bail, ZResult};

use crate::IsotpEndpoint;

/// An ISO-TP channel is a directed identifier pair, so peers do not discover
/// one another: each side is configured with the other's identifier, and this
/// side's `tx_id` is the other side's `rx_id`.
///
/// There is therefore nothing to `accept()`. One side listens and the other
/// connects only so that zenoh's handshake has a client and a server; the
/// socket setup is identical on both. `zenoh-link-serial` takes the same shape
/// for the same reason.
pub struct LinkManagerUnicastIsotp {
    manager: NewLinkChannelSender,
    #[cfg(target_os = "linux")]
    listeners: std::sync::Arc<
        tokio::sync::RwLock<std::collections::HashMap<EndPoint, imp::ListenerHandle>>,
    >,
}

impl LinkManagerUnicastIsotp {
    pub fn new(manager: NewLinkChannelSender) -> Self {
        Self {
            manager,
            #[cfg(target_os = "linux")]
            listeners: std::sync::Arc::new(tokio::sync::RwLock::new(
                std::collections::HashMap::new(),
            )),
        }
    }
}

#[async_trait]
impl LinkManagerUnicastTrait for LinkManagerUnicastIsotp {
    async fn new_link(&self, endpoint: EndPoint) -> ZResult<LinkUnicast> {
        // Parse and validate on every platform, so a malformed endpoint is
        // reported as malformed rather than as a missing platform.
        let ep = IsotpEndpoint::parse(&endpoint)?;
        #[cfg(target_os = "linux")]
        {
            imp::new_link(ep).await
        }
        #[cfg(not(target_os = "linux"))]
        {
            unsupported(&ep)
        }
    }

    async fn new_listener(&self, endpoint: EndPoint) -> ZResult<Locator> {
        let ep = IsotpEndpoint::parse(&endpoint)?;
        #[cfg(target_os = "linux")]
        {
            imp::new_listener(self, endpoint, ep).await
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = &self.manager;
            unsupported(&ep)
        }
    }

    async fn del_listener(&self, endpoint: &EndPoint) -> ZResult<()> {
        #[cfg(target_os = "linux")]
        {
            match self.listeners.write().await.remove(endpoint) {
                Some(handle) => {
                    handle.stop();
                    Ok(())
                }
                None => bail!("ISO-TP: no listener on {endpoint}"),
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            bail!("ISO-TP: no listener on {endpoint}")
        }
    }

    async fn get_listeners(&self) -> Vec<EndPoint> {
        #[cfg(target_os = "linux")]
        {
            self.listeners.read().await.keys().cloned().collect()
        }
        #[cfg(not(target_os = "linux"))]
        {
            Vec::new()
        }
    }

    async fn get_locators(&self) -> Vec<Locator> {
        #[cfg(target_os = "linux")]
        {
            self.listeners
                .read()
                .await
                .values()
                .map(|h| h.locator.clone())
                .collect()
        }
        #[cfg(not(target_os = "linux"))]
        {
            Vec::new()
        }
    }

    /// A CAN interface is never loopback, so this is the full set.
    async fn get_locators_noloopback(&self) -> Vec<Locator> {
        self.get_locators().await
    }
}

#[cfg(not(target_os = "linux"))]
fn unsupported<T>(ep: &IsotpEndpoint) -> ZResult<T> {
    bail!(
        "ISO-TP is a Linux kernel protocol (PF_CAN/CAN_ISOTP); cannot use {:?} on this platform",
        ep.device
    )
}

#[cfg(target_os = "linux")]
mod imp {
    use std::{
        fmt,
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        },
        time::Duration,
    };

    use async_trait::async_trait;
    use tokio_util::sync::CancellationToken;
    use zenoh_link_commons::{LinkAuthId, LinkUnicast, LinkUnicastTrait};
    use zenoh_protocol::{
        core::{EndPoint, Locator, Priority},
        transport::BatchSize,
    };
    use zenoh_result::{bail, ZResult};

    use super::LinkManagerUnicastIsotp;
    use crate::{sys::IsotpSocket, IsotpEndpoint, ISOTP_MAX_MTU};

    pub(super) struct ListenerHandle {
        pub(super) locator: Locator,
        token: CancellationToken,
    }

    impl ListenerHandle {
        pub(super) fn stop(&self) {
            self.token.cancel();
        }
    }

    pub(super) struct LinkUnicastIsotp {
        /// One socket per traffic class. With the default single class this is
        /// one element and the link behaves exactly as it did before.
        sockets: Vec<IsotpSocket>,
        ep: IsotpEndpoint,
        src: Locator,
        dst: Locator,
        device: String,
        /// Cleared when the link is dropped, which is how the listener learns
        /// that its identifier pair is free again and it may re-arm.
        connected: Arc<AtomicBool>,
    }

    impl Drop for LinkUnicastIsotp {
        fn drop(&mut self) {
            self.connected.store(false, Ordering::Release);
        }
    }

    impl LinkUnicastIsotp {
        fn new(ep: &IsotpEndpoint) -> ZResult<LinkUnicastIsotp> {
            // One socket per class. Opening them all up front means a partly
            // usable link is never handed to the transport: either every class
            // has its identifier pair or the link fails to open.
            let mut sockets = Vec::with_capacity(ep.prio_classes as usize);
            for class in 0..ep.prio_classes {
                sockets.push(IsotpSocket::open(ep, class)?);
            }
            tracing::debug!(
                "ISO-TP link on {:?}: tx {:#x}, rx {:#x}, {} class(es), MTU {}",
                ep.device,
                ep.tx_id,
                ep.rx_id,
                ep.prio_classes,
                ISOTP_MAX_MTU
            );
            Ok(LinkUnicastIsotp {
                sockets,
                ep: ep.clone(),
                connected: Arc::new(AtomicBool::new(true)),
                // The pair is directional, so the two ends of the link are the
                // two identifiers rather than two addresses.
                src: ep.locator(),
                dst: ep.peer_locator(),
                device: ep.device.clone(),
            })
        }

        /// A CAN identifier is the bus priority, so this is where zenoh's QoS
        /// becomes arbitration: the batch's priority picks the socket, and the
        /// socket's identifier decides who wins the wire.
        fn socket_for(&self, priority: Option<Priority>) -> &IsotpSocket {
            let p = priority.unwrap_or(Priority::DEFAULT) as u8;
            &self.sockets[self.ep.class_of(p) as usize]
        }
    }

    #[async_trait]
    impl LinkUnicastTrait for LinkUnicastIsotp {
        fn get_mtu(&self) -> BatchSize {
            ISOTP_MAX_MTU
        }

        fn get_src(&self) -> &Locator {
            &self.src
        }

        fn get_dst(&self) -> &Locator {
            &self.dst
        }

        /// ISO-TP paces with flow control, but a lost consecutive frame aborts
        /// the whole PDU and nothing below zenoh recovers it.
        fn is_reliable(&self) -> bool {
            crate::IS_RELIABLE
        }

        /// ISO-TP preserves message boundaries: one write is one PDU.
        fn is_streamed(&self) -> bool {
            false
        }

        fn get_interface_names(&self) -> Vec<String> {
            vec![self.device.clone()]
        }

        fn get_auth_id(&self) -> &LinkAuthId {
            &LinkAuthId::Isotp
        }

        /// True only when each priority has its own identifier pair. Reporting
        /// it otherwise would make zenoh fan out receive tasks onto a single
        /// socket, where they would race for the same PDUs.
        fn supports_priorities(&self) -> bool {
            self.ep.has_priority_classes()
        }

        async fn write(&self, buffer: &[u8], priority: Option<Priority>) -> ZResult<usize> {
            self.socket_for(priority).send(buffer).await
        }

        /// One PDU per call; the kernel segments it. zenoh never hands the link
        /// more than its MTU, because the batch is clamped to it.
        async fn write_all(&self, buffer: &[u8], priority: Option<Priority>) -> ZResult<()> {
            let n = self.socket_for(priority).send(buffer).await?;
            if n != buffer.len() {
                bail!(
                    "ISO-TP: short write of {n} bytes, expected {}",
                    buffer.len()
                );
            }
            Ok(())
        }

        /// zenoh runs one receive task per priority when the link reports
        /// priority support, so each call already belongs to a single class and
        /// there is nothing to select across here.
        async fn read(&self, buffer: &mut [u8], priority: Option<Priority>) -> ZResult<usize> {
            self.socket_for(priority).recv(buffer).await
        }

        /// "Exact" and "best effort" collapse on a message-preserving link: one
        /// call returns one whole PDU or it fails.
        async fn read_exact(&self, buffer: &mut [u8], priority: Option<Priority>) -> ZResult<()> {
            let n = self.read(buffer, priority).await?;
            if n != buffer.len() {
                bail!("ISO-TP: read {n} bytes, expected {}", buffer.len());
            }
            Ok(())
        }

        async fn close(&self) -> ZResult<()> {
            // The socket closes when the link is dropped; ISO-TP has no
            // teardown handshake of its own.
            tracing::trace!("Closing ISO-TP link: {self}");
            Ok(())
        }

        #[cfg(all(feature = "uring", target_os = "linux"))]
        fn get_fd(&self) -> ZResult<std::os::fd::RawFd> {
            bail!("ISO-TP: io_uring is not supported on this link")
        }
    }

    impl fmt::Display for LinkUnicastIsotp {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "{} => {}", self.src, self.dst)
        }
    }

    impl fmt::Debug for LinkUnicastIsotp {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("Isotp")
                .field("src", &self.src)
                .field("dst", &self.dst)
                .field("mtu", &ISOTP_MAX_MTU)
                .finish()
        }
    }

    pub(super) async fn new_link(ep: IsotpEndpoint) -> ZResult<LinkUnicast> {
        let link = LinkUnicastIsotp::new(&ep)?;
        Ok(LinkUnicast::from(
            Arc::new(link) as Arc<dyn LinkUnicastTrait>
        ))
    }

    /// Binding is all a listener can do.
    ///
    /// ISO-TP is symmetric and connectionless: both ends bind the same way with
    /// the identifiers swapped, and there is exactly one peer a given pair can
    /// ever reach. So there is nothing to wait for and nothing to demultiplex --
    /// the link is handed straight to the transport, which runs its own
    /// handshake over it.
    pub(super) async fn new_listener(
        mgr: &LinkManagerUnicastIsotp,
        endpoint: EndPoint,
        ep: IsotpEndpoint,
    ) -> ZResult<Locator> {
        if mgr.listeners.read().await.contains_key(&endpoint) {
            bail!("ISO-TP: already listening on {endpoint}");
        }

        // Bind once up front so a bad endpoint fails the caller rather than a
        // background task nobody is watching.
        let first = LinkUnicastIsotp::new(&ep)?;
        let locator = first.src.clone();
        let token = CancellationToken::new();

        let manager = mgr.manager.clone();
        let task_token = token.clone();
        tokio::task::spawn(async move { accept_loop(ep, first, manager, task_token).await });

        mgr.listeners.write().await.insert(
            endpoint,
            ListenerHandle {
                locator: locator.clone(),
                token,
            },
        );
        Ok(locator)
    }

    /// Re-arm after every client.
    ///
    /// A listener that hands over one link and stops is not a listener: the
    /// first peer works and every later one fails, which surfaces far away from
    /// here as intermittent ROS behaviour. `zenoh-link-serial` loops for the
    /// same reason.
    ///
    /// The identifier pair is a single kernel socket, so the next bind can only
    /// happen once the previous link is dropped. `connected` is cleared in
    /// `Drop`, which is exactly that moment.
    async fn accept_loop(
        ep: IsotpEndpoint,
        first: LinkUnicastIsotp,
        manager: zenoh_link_commons::NewLinkChannelSender,
        token: CancellationToken,
    ) {
        let mut next = Some(first);
        loop {
            let link = match next.take() {
                Some(l) => l,
                None => match rebind(&ep, &token).await {
                    Some(l) => l,
                    None => return,
                },
            };
            let connected = link.connected.clone();

            if manager
                .send_async(LinkUnicast::from(
                    Arc::new(link) as Arc<dyn LinkUnicastTrait>
                ))
                .await
                .is_err()
            {
                // The transport manager is gone; so is the reason to listen.
                return;
            }

            // Wait for this client to finish before rebinding the pair.
            while connected.load(Ordering::Acquire) {
                tokio::select! {
                    _ = token.cancelled() => return,
                    _ = tokio::time::sleep(Duration::from_millis(100)) => {}
                }
            }
        }
    }

    /// The socket is released when the previous link drops, but the drop and
    /// this rebind race by a few microseconds, so a first failure is expected
    /// rather than fatal.
    async fn rebind(ep: &IsotpEndpoint, token: &CancellationToken) -> Option<LinkUnicastIsotp> {
        loop {
            match LinkUnicastIsotp::new(ep) {
                Ok(link) => return Some(link),
                Err(e) => {
                    tracing::debug!("ISO-TP: rebinding {:?} shortly: {e}", ep.device);
                    tokio::select! {
                        _ = token.cancelled() => return None,
                        _ = tokio::time::sleep(Duration::from_millis(100)) => {}
                    }
                }
            }
        }
    }
}
