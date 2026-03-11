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
use std::{sync::Arc, time::Duration};

use zenoh_core::zerror;
use zenoh_link_commons::{
    quic::unicast::{
        QuicAcceptorParams, QuicClient, QuicClientBuilder, QuicConnection, QuicLinkMaterial,
        QuicServer, QuicServerBuilder, QuicStreams,
    },
    LinkAuthId, LinkUnicast, LinkUnicastTrait,
};
use zenoh_link_quic_datagram::LinkUnicastQuicDatagram;
use zenoh_protocol::{
    core::{EndPoint, Locator, Priority},
    transport::BatchSize,
};
use zenoh_result::ZResult;

use crate::{
    LinkManagerUnicastUdp, LinkUnicastUdp, LinkUnicastUdpVariant, UDP_ACCEPT_THROTTLE_TIME,
    UDP_LOCATOR_PREFIX,
};

pub(crate) struct LinkUnicastQuicUnsecure {
    connection: QuicConnection,
    streams: QuicStreams,
}

impl LinkUnicastQuicUnsecure {
    pub(crate) async fn connect(endpoint: &EndPoint) -> ZResult<LinkUnicast> {
        let QuicClient {
            quic_conn,
            streams,
            src_addr,
            dst_addr,
            is_mixed_rel,
            tls_close_link_on_expiration: _,
        } = QuicClientBuilder::new(endpoint).security(false).await?;
        let streams = streams.expect("QUIC streams should be initialized");
        let quic_link = Self {
            connection: quic_conn.clone(),
            streams,
        };
        let link: Arc<dyn LinkUnicastTrait> = Arc::new(LinkUnicastUdp::new(
            src_addr,
            dst_addr,
            LinkUnicastUdpVariant::Reliable(Box::new(quic_link)),
        ));
        if is_mixed_rel {
            let best_effort = Arc::new(LinkUnicastQuicDatagram::new(
                quic_conn,
                src_addr,
                endpoint.clone().into(),
                LinkAuthId::Udp,
                UDP_LOCATOR_PREFIX,
                None,
            ));
            Ok(LinkUnicast(zenoh_link_commons::NewLink::MixedReliability {
                reliable: link,
                best_effort,
            }))
        } else {
            Ok(LinkUnicast::from(link))
        }
    }

    pub(crate) async fn listen(
        endpoint: &EndPoint,
        manager: &LinkManagerUnicastUdp,
    ) -> ZResult<Locator> {
        let token = manager.listeners.token.child_token();
        let acceptor_params = QuicAcceptorParams {
            token: token.clone(),
            manager: manager.manager.clone(),
            throttle_time: Duration::from_micros(*UDP_ACCEPT_THROTTLE_TIME),
            make_link: Self::make_link,
        };

        let QuicServer {
            quic_acceptor,
            locator,
            local_addr,
        } = QuicServerBuilder::new(endpoint, acceptor_params)
            .security(false)
            .await?;

        // Update the endpoint locator address
        let endpoint = EndPoint::new(
            locator.protocol(),
            locator.address(),
            locator.metadata(),
            endpoint.config(),
        )?;

        let task = async move { quic_acceptor.await };
        manager
            .listeners
            .add_listener(endpoint, local_addr, task, token)
            .await?;

        Ok(locator)
    }

    pub(crate) async fn write(&self, buffer: &[u8], priority: Option<Priority>) -> ZResult<usize> {
        unsafe { self.streams.write(buffer, priority) }
            .await
            .map_err(|e| {
                let e = zerror!("Write error on QUIC link: {}", e);
                tracing::trace!("{}", &e);
                e.into()
            })
    }

    pub(crate) async fn write_all(&self, buffer: &[u8], priority: Option<Priority>) -> ZResult<()> {
        unsafe { self.streams.write_all(buffer, priority) }
            .await
            .map_err(|e| {
                let e = zerror!("Write error on QUIC link: {}", e);
                tracing::trace!("{}", &e);
                e.into()
            })
    }

    pub(crate) async fn read(
        &self,
        buffer: &mut [u8],
        priority: Option<Priority>,
    ) -> ZResult<usize> {
        unsafe { self.streams.read(buffer, priority) }
            .await
            .map_err(|e| {
                let e = zerror!("Read error on QUIC link: {}", e);
                tracing::trace!("{}", &e);
                e.into()
            })
    }

    pub(crate) async fn read_exact(
        &self,
        buffer: &mut [u8],
        priority: Option<Priority>,
    ) -> ZResult<()> {
        unsafe { self.streams.read_exact(buffer, priority) }
            .await
            .map_err(|e| {
                let e = zerror!("Read error on QUIC link: {}", e);
                tracing::trace!("{}", &e);
                e.into()
            })
    }

    pub(crate) fn get_mtu(&self) -> BatchSize {
        BatchSize::MAX
    }

    pub(crate) fn close(&self) {
        self.connection.close();
    }

    pub(crate) fn supports_priorities(&self) -> bool {
        self.streams.is_multistream
    }

    fn make_link(quic_link_material: QuicLinkMaterial) -> ZResult<LinkUnicast> {
        let QuicLinkMaterial {
            quic_conn,
            src_addr,
            dst_addr,
            streams,
            is_mixed_rel,
            tls_close_link_on_expiration: _,
        } = quic_link_material;
        let quic_link = Self {
            connection: quic_conn.clone(),
            streams: streams.expect("QUIC streams should be initialized"),
        };
        let link: Arc<dyn LinkUnicastTrait> = Arc::new(LinkUnicastUdp::new(
            src_addr,
            dst_addr,
            LinkUnicastUdpVariant::Reliable(Box::new(quic_link)),
        ));
        if is_mixed_rel {
            let dst_locator = Locator::new(UDP_LOCATOR_PREFIX, dst_addr.to_string(), "")?;
            let best_effort = Arc::new(LinkUnicastQuicDatagram::new(
                quic_conn,
                src_addr,
                dst_locator,
                LinkAuthId::Udp,
                UDP_LOCATOR_PREFIX,
                None,
            ));
            Ok(LinkUnicast(zenoh_link_commons::NewLink::MixedReliability {
                reliable: link,
                best_effort,
            }))
        } else {
            Ok(LinkUnicast::from(link))
        }
    }
}

impl Drop for LinkUnicastQuicUnsecure {
    fn drop(&mut self) {
        self.close()
    }
}
