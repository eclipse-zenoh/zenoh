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

use std::{
    net::{Ipv4Addr, Ipv6Addr, SocketAddr},
    sync::Arc,
};

use quinn::{
    crypto::rustls::{QuicClientConfig, QuicServerConfig},
    EndpointConfig,
};
use zenoh_config::{EndPoint, Locator};
use zenoh_core::{bail, zerror};
use zenoh_protocol::core::Address;
use zenoh_result::ZResult;

use crate::{
    parse_dscp,
    quic::{
        get_quic_addr, get_quic_host, QuicMtuConfig, TlsClientConfig, TlsServerConfig,
        ALPN_QUIC_HTTP,
    },
    set_dscp, BIND_INTERFACE, BIND_SOCKET,
};

pub struct QuicLink {}

impl QuicLink {
    pub async fn server(
        endpoint: &EndPoint,
    ) -> ZResult<(quinn::Endpoint, Locator, SocketAddr, bool)> {
        let epaddr = endpoint.address();
        let epconf = endpoint.config();

        if epconf.is_empty() {
            bail!("No QUIC configuration provided");
        };

        let addr = get_quic_addr(&epaddr).await?;
        let host = get_quic_host(&epaddr)?;

        // Server config
        let mut server_crypto = TlsServerConfig::new(&epconf)
            .await
            .map_err(|e| zerror!("Cannot create a new QUIC listener on {addr}: {e}"))?;
        server_crypto.server_config.alpn_protocols =
            ALPN_QUIC_HTTP.iter().map(|&x| x.into()).collect();

        // Install ring based rustls CryptoProvider.
        rustls::crypto::ring::default_provider()
            // This can be called successfully at most once in any process execution.
            // Call this early in your process to configure which provider is used for the provider.
            // The configuration should happen before any use of ClientConfig::builder() or ServerConfig::builder().
            .install_default()
            // Ignore the error here, because `rustls::crypto::ring::default_provider().install_default()` will inevitably be executed multiple times
            // when there are multiple quic links, and all but the first execution will fail.
            .ok();

        let quic_config: QuicServerConfig = server_crypto
            .server_config
            .try_into()
            .map_err(|e| zerror!("Can not create a new QUIC listener on {addr}: {e}"))?;
        let mut server_config = quinn::ServerConfig::with_crypto(Arc::new(quic_config));

        // We do not accept unidireactional streams.
        Arc::get_mut(&mut server_config.transport)
            .unwrap()
            .max_concurrent_uni_streams(0_u8.into());
        // For the time being we only allow one bidirectional stream
        Arc::get_mut(&mut server_config.transport)
            .unwrap()
            .max_concurrent_bidi_streams(1_u8.into());

        let mtu_config = QuicMtuConfig::try_from(&epconf)?;
        mtu_config.apply_to_transport(Arc::get_mut(&mut server_config.transport).unwrap());

        // Initialize the Endpoint
        let quic_endpoint = if let Some(iface) = server_crypto.bind_iface {
            async {
                // Bind the UDP socket
                let socket = tokio::net::UdpSocket::bind(addr).await?;
                zenoh_util::net::set_bind_to_device_udp_socket(&socket, iface)?;

                // create the Endpoint with this socket
                let runtime = quinn::default_runtime()
                    .ok_or_else(|| std::io::Error::other("no async runtime found"))?;
                ZResult::Ok(quinn::Endpoint::new_with_abstract_socket(
                    EndpointConfig::default(),
                    Some(server_config),
                    runtime.wrap_udp_socket(socket.into_std()?)?,
                    runtime,
                )?)
            }
            .await
        } else {
            quinn::Endpoint::server(server_config, addr).map_err(Into::into)
        }
        .map_err(|e| zerror!("Can not create a new QUIC listener on {}: {}", addr, e))?;

        let local_addr = quic_endpoint
            .local_addr()
            .map_err(|e| zerror!("Can not create a new QUIC listener on {}: {}", addr, e))?;
        let local_port = local_addr.port();

        let locator = Locator::new(
            endpoint.protocol(),
            format!("{host}:{local_port}"),
            endpoint.metadata(),
        )?;

        Ok((
            quic_endpoint,
            locator,
            local_addr,
            server_crypto.tls_close_link_on_expiration,
        ))
    }

    pub async fn connect(
        endpoint: &EndPoint,
    ) -> ZResult<(quinn::Connection, SocketAddr, SocketAddr, &str, bool)> {
        let epaddr = endpoint.address();
        let host = get_quic_host(&epaddr)?;
        let epconf = endpoint.config();
        let dst_addr = get_quic_addr(&epaddr).await?;

        // if both `iface`, and `bind` are present, return error
        if let (Some(_), Some(_)) = (epconf.get(BIND_INTERFACE), epconf.get(BIND_SOCKET)) {
            bail!(
                "Using Config options `iface` and `bind` in conjunction is unsupported at this time {} {:?}",
                BIND_INTERFACE,
                BIND_SOCKET
            )
        }

        // Initialize the QUIC connection
        let mut client_crypto = TlsClientConfig::new(&epconf)
            .await
            .map_err(|e| zerror!("Cannot create a new QUIC client on {dst_addr}: {e}"))?;

        client_crypto.client_config.alpn_protocols =
            ALPN_QUIC_HTTP.iter().map(|&x| x.into()).collect();

        let src_addr = if let Some(bind_socket_str) = epconf.get(BIND_SOCKET) {
            get_quic_addr(&Address::from(bind_socket_str)).await?
        } else if dst_addr.is_ipv4() {
            SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), 0)
        } else {
            SocketAddr::new(Ipv6Addr::UNSPECIFIED.into(), 0)
        };

        let socket = tokio::net::UdpSocket::bind(src_addr).await?;
        if let Some(dscp) = parse_dscp(&epconf)? {
            set_dscp(&socket, src_addr, dscp)?;
        }

        // Initialize the Endpoint
        if let Some(iface) = client_crypto.bind_iface {
            zenoh_util::net::set_bind_to_device_udp_socket(&socket, iface)?;
        };

        let mut quic_endpoint = {
            // create the Endpoint with this socket
            let runtime = quinn::default_runtime()
                .ok_or_else(|| std::io::Error::other("no async runtime found"))?;
            ZResult::Ok(quinn::Endpoint::new_with_abstract_socket(
                EndpointConfig::default(),
                None,
                runtime.wrap_udp_socket(socket.into_std()?)?,
                runtime,
            )?)
        }
        .map_err(|e| zerror!("Can not create a new QUIC link bound to {host}: {e}"))?;

        let quic_config: QuicClientConfig = client_crypto
            .client_config
            .try_into()
            .map_err(|e| zerror!("Can not get QUIC config {host}: {e}"))?;
        quic_endpoint.set_default_client_config(quinn::ClientConfig::new(Arc::new(quic_config)));

        let src_addr = quic_endpoint
            .local_addr()
            .map_err(|e| zerror!("Can not get QUIC local_addr bound to {}: {}", host, e))?;

        let connection = quic_endpoint
            .connect(dst_addr, host)
            .map_err(|e| {
                zerror!(
                    "Can not get connect quick endpoint : {} : {} : {}",
                    dst_addr,
                    host,
                    e
                )
            })?
            .await
            .map_err(|e| zerror!("Can not create a new QUIC link bound to {}: {}", host, e))?;

        Ok((
            connection,
            src_addr,
            dst_addr,
            host,
            client_crypto.tls_close_link_on_expiration,
        ))
    }
}
