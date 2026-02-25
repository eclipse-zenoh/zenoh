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
use std::{net::SocketAddr, sync::Arc, time::Duration};

use zenoh_core::zerror;
use zenoh_link_commons::{
    quic::unicast::{
        QuicAcceptorParams, QuicClient, QuicClientBuilder, QuicLinkMaterial, QuicServer,
        QuicServerBuilder, QuicStreams,
    },
    LinkUnicastTrait,
};
use zenoh_protocol::{
    core::{EndPoint, Locator, Priority},
    transport::BatchSize,
};
use zenoh_result::ZResult;

use crate::{
    LinkManagerUnicastUdp, LinkUnicastUdp, LinkUnicastUdpVariant, UDP_ACCEPT_THROTTLE_TIME,
};

pub(crate) struct LinkUnicastQuicUnsecure {
    connection: quinn::Connection,
    streams: QuicStreams,
}

impl LinkUnicastQuicUnsecure {
    pub(crate) async fn connect(
        endpoint: &EndPoint,
    ) -> ZResult<(LinkUnicastQuicUnsecure, SocketAddr, SocketAddr)> {
        let QuicClient {
            quic_conn,
            streams,
            src_addr,
            dst_addr,
            tls_close_link_on_expiration: _,
        } = QuicClientBuilder::new(endpoint).security(false).await?;
        let streams = streams.expect("QUIC streams should be initialized");
        Ok((
            Self {
                connection: quic_conn,
                streams,
            },
            src_addr,
            dst_addr,
        ))
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
        self.connection.close(quinn::VarInt::from_u32(0), &[0])
    }

    pub(crate) fn supports_priorities(&self) -> bool {
        self.streams.is_multistream
    }

    fn make_link(quic_link_material: QuicLinkMaterial) -> ZResult<Arc<dyn LinkUnicastTrait>> {
        let quic_link = Self {
            connection: quic_link_material.quic_conn,
            streams: quic_link_material
                .streams
                .expect("QUIC streams should be initialized"),
        };
        let link = LinkUnicastUdp::new(
            quic_link_material.src_addr,
            quic_link_material.dst_addr,
            LinkUnicastUdpVariant::Reliable(Box::new(quic_link)),
        );
        Ok(Arc::new(link))
    }
}

impl Drop for LinkUnicastQuicUnsecure {
    fn drop(&mut self) {
        self.close()
    }
}
