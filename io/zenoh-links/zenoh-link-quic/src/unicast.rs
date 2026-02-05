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

use std::{fmt, net::SocketAddr, sync::Arc, time::Duration};

use async_trait::async_trait;
use zenoh_link_commons::{
    get_ip_interface_names,
    quic::{
        get_cert_chain_expiration, get_cert_common_name, get_quic_addr,
        unicast::{QuicLink, QuicLinkMaterial, QuicStreams},
    },
    tls::expiration::{LinkCertExpirationManager, LinkWithCertExpiration},
    LinkAuthId, LinkManagerUnicastTrait, LinkUnicast, LinkUnicastTrait, ListenersUnicastIP,
    NewLinkChannelSender,
};
use zenoh_protocol::{
    core::{EndPoint, Locator, Priority},
    transport::BatchSize,
};
use zenoh_result::{zerror, ZResult};

use super::{QUIC_ACCEPT_THROTTLE_TIME, QUIC_DEFAULT_MTU, QUIC_LOCATOR_PREFIX};

pub struct LinkUnicastQuic {
    connection: quinn::Connection,
    src_addr: SocketAddr,
    src_locator: Locator,
    dst_locator: Locator,
    streams: QuicStreams,
    auth_identifier: LinkAuthId,
    expiration_manager: Option<LinkCertExpirationManager>,
}

unsafe impl Sync for LinkUnicastQuic {}

impl LinkUnicastQuic {
    #[allow(clippy::too_many_arguments)]
    fn new(
        connection: quinn::Connection,
        src_addr: SocketAddr,
        dst_locator: Locator,
        streams: QuicStreams,
        auth_identifier: LinkAuthId,
        expiration_manager: Option<LinkCertExpirationManager>,
    ) -> LinkUnicastQuic {
        LinkUnicastQuic {
            connection,
            src_addr,
            src_locator: Locator::new(QUIC_LOCATOR_PREFIX, src_addr.to_string(), "").unwrap(),
            dst_locator,
            streams,
            auth_identifier,
            expiration_manager,
        }
    }

    async fn close(&self) -> ZResult<()> {
        tracing::trace!("Closing QUIC link: {}", self);
        self.connection.close(quinn::VarInt::from_u32(0), &[0]);
        Ok(())
    }
}

#[async_trait]
impl LinkUnicastTrait for LinkUnicastQuic {
    async fn close(&self) -> ZResult<()> {
        if let Some(expiration_manager) = &self.expiration_manager {
            if !expiration_manager.set_closing() {
                // expiration_task is closing link, return its returned ZResult to Transport
                return expiration_manager.wait_for_expiration_task().await;
            }
            // cancel the expiration task and close link
            expiration_manager.cancel_expiration_task();
            let res = self.close().await;
            let _ = expiration_manager.wait_for_expiration_task().await;
            return res;
        }
        self.close().await
    }

    async fn write(&self, buffer: &[u8], priority: Option<Priority>) -> ZResult<usize> {
        unsafe { self.streams.write(buffer, priority) }
            .await
            .map_err(|e| {
                let e = zerror!("Write error on QUIC link {}: {}", self, e);
                tracing::trace!("{}", &e);
                e.into()
            })
    }

    async fn write_all(&self, buffer: &[u8], priority: Option<Priority>) -> ZResult<()> {
        unsafe { self.streams.write_all(buffer, priority) }
            .await
            .map_err(|e| {
                let e = zerror!("Write error on QUIC link {}: {}", self, e);
                tracing::trace!("{}", &e);
                e.into()
            })
    }

    async fn read(&self, buffer: &mut [u8], priority: Option<Priority>) -> ZResult<usize> {
        unsafe { self.streams.read(buffer, priority) }
            .await
            .map_err(|e| {
                let e = zerror!("Read error on QUIC link {}: {}", self, e);
                tracing::trace!("{}", &e);
                e.into()
            })
    }

    async fn read_exact(&self, buffer: &mut [u8], priority: Option<Priority>) -> ZResult<()> {
        unsafe { self.streams.read_exact(buffer, priority) }
            .await
            .map_err(|e| {
                let e = zerror!("Read error on QUIC link {}: {}", self, e);
                tracing::trace!("{}", &e);
                e.into()
            })
    }

    #[inline(always)]
    fn get_src(&self) -> &Locator {
        &self.src_locator
    }

    #[inline(always)]
    fn get_dst(&self) -> &Locator {
        &self.dst_locator
    }

    #[inline(always)]
    fn get_mtu(&self) -> BatchSize {
        *QUIC_DEFAULT_MTU
    }

    #[inline(always)]
    fn get_interface_names(&self) -> Vec<String> {
        get_ip_interface_names(&self.src_addr)
    }

    #[inline(always)]
    fn is_reliable(&self) -> bool {
        super::IS_RELIABLE
    }

    #[inline(always)]
    fn is_streamed(&self) -> bool {
        true
    }

    #[inline(always)]
    fn get_auth_id(&self) -> &LinkAuthId {
        &self.auth_identifier
    }

    #[inline(always)]
    fn supports_priorities(&self) -> bool {
        self.streams.is_multistream
    }
}

#[async_trait]
impl LinkWithCertExpiration for LinkUnicastQuic {
    async fn expire(&self) -> ZResult<()> {
        let expiration_manager = self
            .expiration_manager
            .as_ref()
            .expect("expiration_manager should be set");
        if expiration_manager.set_closing() {
            return self.close().await;
        }
        // Transport is already closing the link
        Ok(())
    }
}

impl Drop for LinkUnicastQuic {
    fn drop(&mut self) {
        self.connection.close(quinn::VarInt::from_u32(0), &[0]);
    }
}

impl fmt::Display for LinkUnicastQuic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} => {}",
            self.src_addr,
            self.connection.remote_address()
        )?;
        Ok(())
    }
}

impl fmt::Debug for LinkUnicastQuic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Quic")
            .field("src", &self.src_addr)
            .field("dst", &self.connection.remote_address())
            .finish()
    }
}

pub struct LinkManagerUnicastQuic {
    manager: NewLinkChannelSender,
    listeners: ListenersUnicastIP,
}

impl LinkManagerUnicastQuic {
    pub fn new(manager: NewLinkChannelSender) -> Self {
        Self {
            manager,
            listeners: ListenersUnicastIP::new(),
        }
    }
}

#[async_trait]
impl LinkManagerUnicastTrait for LinkManagerUnicastQuic {
    async fn new_link(&self, endpoint: EndPoint) -> ZResult<LinkUnicast> {
        let (quic_conn, streams, src_addr, dst_addr, tls_close_link_on_expiration) =
            QuicLink::connect(&endpoint, true).await?;

        let auth_id = get_cert_common_name(&quic_conn)?;
        let certchain_expiration_time =
            get_cert_chain_expiration(&quic_conn)?.expect("server should have certificate chain");

        let link = Arc::<LinkUnicastQuic>::new_cyclic(|weak_link| {
            let mut expiration_manager = None;
            if tls_close_link_on_expiration {
                // setup expiration manager
                expiration_manager = Some(LinkCertExpirationManager::new(
                    weak_link.clone(),
                    src_addr,
                    dst_addr,
                    QUIC_LOCATOR_PREFIX,
                    certchain_expiration_time,
                ))
            }
            LinkUnicastQuic::new(
                quic_conn,
                src_addr,
                endpoint.into(),
                streams.expect("reliable QUIC streams should have been opened"),
                auth_id.into(),
                expiration_manager,
            )
        });

        Ok(LinkUnicast(link))
    }

    async fn new_listener(&self, endpoint: EndPoint) -> ZResult<Locator> {
        let is_streamed = true;
        let (quic_endpoint, locator, local_addr, tls_close_link_on_expiration) =
            QuicLink::server(&endpoint, is_streamed).await?;

        // Update the endpoint locator address
        let endpoint = EndPoint::new(
            locator.protocol(),
            locator.address(),
            locator.metadata(),
            endpoint.config(),
        )?;

        // Spawn the accept loop for the listener
        let token = self.listeners.token.child_token();

        let task = {
            let token = token.clone();
            let manager = self.manager.clone();

            async move {
                QuicLink::accept_task(
                    quic_endpoint,
                    token,
                    manager,
                    is_streamed,
                    Duration::from_micros(*QUIC_ACCEPT_THROTTLE_TIME),
                    |link_material| acceptor_callback(link_material, tls_close_link_on_expiration),
                )
                .await
            }
        };

        // Initialize the QuicAcceptor
        self.listeners
            .add_listener(endpoint, local_addr, task, token)
            .await?;

        Ok(locator)
    }

    async fn del_listener(&self, endpoint: &EndPoint) -> ZResult<()> {
        let epaddr = endpoint.address();
        let addr = get_quic_addr(&epaddr).await?;
        self.listeners.del_listener(addr).await
    }

    async fn get_listeners(&self) -> Vec<EndPoint> {
        self.listeners.get_endpoints()
    }

    async fn get_locators(&self) -> Vec<Locator> {
        self.listeners.get_locators()
    }
}

fn acceptor_callback(
    link_material: QuicLinkMaterial,
    tls_close_link_on_expiration: bool,
) -> ZResult<Arc<dyn LinkUnicastTrait>> {
    let QuicLinkMaterial {
        quic_conn,
        src_addr,
        dst_addr,
        streams,
    } = link_material;
    let streams = streams.expect("Streams should be initialized");

    let dst_locator = Locator::new(QUIC_LOCATOR_PREFIX, dst_addr.to_string(), "")?;
    // Get Quic auth identifier
    let auth_id = get_cert_common_name(&quic_conn)?;

    // Get certificate chain expiration
    let mut maybe_expiration_time = None;
    if tls_close_link_on_expiration {
        match get_cert_chain_expiration(&quic_conn)? {
            exp @ Some(_) => maybe_expiration_time = exp,
            None => tracing::warn!(
                "Cannot monitor expiration for QUIC link {:?} => {:?} : client does not have certificates",
                src_addr,
                dst_addr,
            ),
        }
    }

    tracing::debug!(
        "Accepted QUIC connection on {:?}: {:?}. {:?}.",
        src_addr,
        dst_addr,
        auth_id
    );
    // Create the new link object
    let link = Arc::<LinkUnicastQuic>::new_cyclic(|weak_link| {
        let mut expiration_manager = None;
        if let Some(certchain_expiration_time) = maybe_expiration_time {
            // setup expiration manager
            expiration_manager = Some(LinkCertExpirationManager::new(
                weak_link.clone(),
                src_addr,
                dst_addr,
                QUIC_LOCATOR_PREFIX,
                certchain_expiration_time,
            ));
        }
        LinkUnicastQuic::new(
            quic_conn,
            src_addr,
            dst_locator,
            streams,
            auth_id.into(),
            expiration_manager,
        )
    });
    Ok(link)
}
