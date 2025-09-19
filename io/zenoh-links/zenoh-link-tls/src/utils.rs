//
// Copyright (c) 2024 ZettaScale Technology
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
    convert::TryFrom,
    fs::File,
    io::{self, BufReader, Cursor},
    net::SocketAddr,
    str::FromStr,
    sync::Arc,
    time::Duration,
};

use rustls::{
    pki_types::{CertificateDer, PrivateKeyDer, TrustAnchor},
    server::WebPkiClientVerifier,
    version::TLS13,
    ClientConfig, RootCertStore, ServerConfig,
};
use rustls_pki_types::ServerName;
use secrecy::ExposeSecret;
use webpki::anchor_from_trusted_cert;
use zenoh_config::Config as ZenohConfig;
use zenoh_link_commons::{
    parse_dscp,
    tcp::TcpSocketConfig,
    tls::{
        config::{self, *},
        WebPkiVerifierAnyServerName,
    },
    ConfigurationInspector, BIND_INTERFACE, BIND_SOCKET, TCP_SO_RCV_BUF, TCP_SO_SND_BUF,
};
use zenoh_protocol::core::{
    endpoint::{Address, Config},
    parameters,
};
use zenoh_result::{bail, zerror, ZError, ZResult};

#[derive(Default, Clone, Copy, Debug)]
pub struct TlsConfigurator;

impl ConfigurationInspector<ZenohConfig> for TlsConfigurator {
    fn inspect_config(&self, config: &ZenohConfig) -> ZResult<String> {
        let mut ps: Vec<(&str, &str)> = vec![];

        let c = config.transport().link().tls();

        match (c.root_ca_certificate(), c.root_ca_certificate_base64()) {
            (Some(_), Some(_)) => {
                bail!("Only one between 'root_ca_certificate' and 'root_ca_certificate_base64' can be present!")
            }
            (Some(ca_certificate), None) => {
                ps.push((TLS_ROOT_CA_CERTIFICATE_FILE, ca_certificate));
            }
            (None, Some(ca_certificate)) => {
                ps.push((
                    TLS_ROOT_CA_CERTIFICATE_BASE64,
                    ca_certificate.expose_secret(),
                ));
            }
            _ => {}
        }

        match (c.listen_private_key(), c.listen_private_key_base64()) {
            (Some(_), Some(_)) => {
                bail!("Only one between 'listen_private_key' and 'listen_private_key' can be present!")
            }
            (Some(server_private_key), None) => {
                ps.push((TLS_LISTEN_PRIVATE_KEY_FILE, server_private_key));
            }
            (None, Some(server_private_key)) => {
                ps.push((
                    TLS_LISTEN_PRIVATE_KEY_BASE64,
                    server_private_key.expose_secret(),
                ));
            }
            _ => {}
        }

        match (c.listen_certificate(), c.listen_certificate_base64()) {
            (Some(_), Some(_)) => {
                bail!("Only one between 'listen_certificate' and 'listen_certificate_base64' can be present!")
            }
            (Some(server_certificate), None) => {
                ps.push((TLS_LISTEN_CERTIFICATE_FILE, server_certificate));
            }
            (None, Some(server_certificate)) => {
                ps.push((
                    TLS_LISTEN_CERTIFICATE_BASE64,
                    server_certificate.expose_secret(),
                ));
            }
            _ => {}
        }

        match c.enable_mtls().unwrap_or(TLS_ENABLE_MTLS_DEFAULT) {
            true => ps.push((TLS_ENABLE_MTLS, "true")),
            false => ps.push((TLS_ENABLE_MTLS, "false")),
        }

        match (c.connect_private_key(), c.connect_private_key_base64()) {
            (Some(_), Some(_)) => {
                bail!("Only one between 'connect_private_key' and 'connect_private_key_base64' can be present!")
            }
            (Some(client_private_key), None) => {
                ps.push((TLS_CONNECT_PRIVATE_KEY_FILE, client_private_key));
            }
            (None, Some(client_private_key)) => {
                ps.push((
                    TLS_CONNECT_PRIVATE_KEY_BASE64,
                    client_private_key.expose_secret(),
                ));
            }
            _ => {}
        }

        match (c.connect_certificate(), c.connect_certificate_base64()) {
            (Some(_), Some(_)) => {
                bail!("Only one between 'connect_certificate' and 'connect_certificate_base64' can be present!")
            }
            (Some(client_certificate), None) => {
                ps.push((TLS_CONNECT_CERTIFICATE_FILE, client_certificate));
            }
            (None, Some(client_certificate)) => {
                ps.push((
                    TLS_CONNECT_CERTIFICATE_BASE64,
                    client_certificate.expose_secret(),
                ));
            }
            _ => {}
        }

        match c
            .verify_name_on_connect()
            .unwrap_or(TLS_VERIFY_NAME_ON_CONNECT_DEFAULT)
        {
            true => ps.push((TLS_VERIFY_NAME_ON_CONNECT, "true")),
            false => ps.push((TLS_VERIFY_NAME_ON_CONNECT, "false")),
        };

        match c
            .close_link_on_expiration()
            .unwrap_or(TLS_CLOSE_LINK_ON_EXPIRATION_DEFAULT)
        {
            true => ps.push((TLS_CLOSE_LINK_ON_EXPIRATION, "true")),
            false => ps.push((TLS_CLOSE_LINK_ON_EXPIRATION, "false")),
        }

        let rx_buffer_size;
        if let Some(size) = c.so_rcvbuf() {
            rx_buffer_size = size.to_string();
            ps.push((TCP_SO_RCV_BUF, &rx_buffer_size));
        }

        let tx_buffer_size;
        if let Some(size) = c.so_sndbuf() {
            tx_buffer_size = size.to_string();
            ps.push((TCP_SO_SND_BUF, &tx_buffer_size));
        }

        Ok(parameters::from_iter(ps.drain(..)))
    }
}

pub(crate) struct TlsServerConfig<'a> {
    pub(crate) server_config: ServerConfig,
    pub(crate) tls_handshake_timeout: Duration,
    pub(crate) tls_close_link_on_expiration: bool,
    pub(crate) tcp_socket_config: TcpSocketConfig<'a>,
}

impl<'a> TlsServerConfig<'a> {
    pub async fn new(config: &'a Config<'_>) -> ZResult<Self> {
        let tls_server_client_auth: bool = match config.get(TLS_ENABLE_MTLS) {
            Some(s) => s
                .parse()
                .map_err(|_| zerror!("Unknown enable mTLS argument: {}", s))?,
            None => TLS_ENABLE_MTLS_DEFAULT,
        };
        let tls_close_link_on_expiration: bool = match config.get(TLS_CLOSE_LINK_ON_EXPIRATION) {
            Some(s) => s
                .parse()
                .map_err(|_| zerror!("Unknown close on expiration argument: {}", s))?,
            None => TLS_CLOSE_LINK_ON_EXPIRATION_DEFAULT,
        };
        let tls_server_private_key = TlsServerConfig::load_tls_private_key(config).await?;
        let tls_server_certificate = TlsServerConfig::load_tls_certificate(config).await?;

        let certs: Vec<CertificateDer> =
            rustls_pemfile::certs(&mut Cursor::new(&tls_server_certificate))
                .collect::<Result<_, _>>()
                .map_err(|err| zerror!("Error processing server certificate: {err}."))?;

        let mut keys: Vec<PrivateKeyDer> =
            rustls_pemfile::rsa_private_keys(&mut Cursor::new(&tls_server_private_key))
                .map(|x| x.map(PrivateKeyDer::from))
                .collect::<Result<_, _>>()
                .map_err(|err| zerror!("Error processing server key: {err}."))?;

        if keys.is_empty() {
            keys = rustls_pemfile::pkcs8_private_keys(&mut Cursor::new(&tls_server_private_key))
                .map(|x| x.map(PrivateKeyDer::from))
                .collect::<Result<_, _>>()
                .map_err(|err| zerror!("Error processing server key: {err}."))?;
        }

        if keys.is_empty() {
            keys = rustls_pemfile::ec_private_keys(&mut Cursor::new(&tls_server_private_key))
                .map(|x| x.map(PrivateKeyDer::from))
                .collect::<Result<_, _>>()
                .map_err(|err| zerror!("Error processing server key: {err}."))?;
        }

        if keys.is_empty() {
            bail!("No private key found for TLS server.");
        }

        // Install ring based rustls CryptoProvider.
        rustls::crypto::ring::default_provider()
            // This can be called successfully at most once in any process execution.
            // Call this early in your process to configure which provider is used for the provider.
            // The configuration should happen before any use of ClientConfig::builder() or ServerConfig::builder().
            .install_default()
            // Ignore the error here, because `rustls::crypto::ring::default_provider().install_default()` will inevitably be executed multiple times
            // when there are multiple quic links, and all but the first execution will fail.
            .ok();

        let sc = if tls_server_client_auth {
            let root_cert_store = load_trust_anchors(config)?.map_or_else(
                || Err(zerror!("Missing root certificates while mTLS is enabled.")),
                Ok,
            )?;
            let client_auth = WebPkiClientVerifier::builder(root_cert_store.into()).build()?;
            ServerConfig::builder_with_protocol_versions(&[&TLS13])
                .with_client_cert_verifier(client_auth)
                .with_single_cert(certs, keys.remove(0))
                .map_err(|e| zerror!(e))?
        } else {
            ServerConfig::builder()
                .with_no_client_auth()
                .with_single_cert(certs, keys.remove(0))
                .map_err(|e| zerror!(e))?
        };

        let tls_handshake_timeout = Duration::from_millis(
            config
                .get(config::TLS_HANDSHAKE_TIMEOUT_MS)
                .map(u64::from_str)
                .transpose()?
                .unwrap_or(config::TLS_HANDSHAKE_TIMEOUT_MS_DEFAULT),
        );

        let mut tcp_rx_buffer_size = None;
        if let Some(size) = config.get(TCP_SO_RCV_BUF) {
            tcp_rx_buffer_size = Some(
                size.parse()
                    .map_err(|_| zerror!("Unknown TCP read buffer size argument: {}", size))?,
            );
        };
        let mut tcp_tx_buffer_size = None;
        if let Some(size) = config.get(TCP_SO_SND_BUF) {
            tcp_tx_buffer_size = Some(
                size.parse()
                    .map_err(|_| zerror!("Unknown TCP write buffer size argument: {}", size))?,
            );
        };
        let mut bind_socket = None;
        if let Some(bind_socket_str) = config.get(BIND_SOCKET) {
            bind_socket = Some(get_tls_addr(&Address::from(bind_socket_str)).await?);
        };

        Ok(TlsServerConfig {
            server_config: sc,
            tls_handshake_timeout,
            tls_close_link_on_expiration,
            tcp_socket_config: TcpSocketConfig::new(
                tcp_tx_buffer_size,
                tcp_rx_buffer_size,
                config.get(BIND_INTERFACE),
                bind_socket,
                parse_dscp(config)?,
            ),
        })
    }

    async fn load_tls_private_key(config: &Config<'_>) -> ZResult<Vec<u8>> {
        load_tls_key(
            config,
            TLS_LISTEN_PRIVATE_KEY_RAW,
            TLS_LISTEN_PRIVATE_KEY_FILE,
            TLS_LISTEN_PRIVATE_KEY_BASE64,
        )
        .await
    }

    async fn load_tls_certificate(config: &Config<'_>) -> ZResult<Vec<u8>> {
        load_tls_certificate(
            config,
            TLS_LISTEN_CERTIFICATE_RAW,
            TLS_LISTEN_CERTIFICATE_FILE,
            TLS_LISTEN_CERTIFICATE_BASE64,
        )
        .await
    }
}

pub(crate) struct TlsClientConfig<'a> {
    pub(crate) client_config: ClientConfig,
    pub(crate) tls_close_link_on_expiration: bool,
    pub(crate) tcp_socket_config: TcpSocketConfig<'a>,
}

impl<'a> TlsClientConfig<'a> {
    pub async fn new(config: &'a Config<'_>) -> ZResult<Self> {
        let tls_client_server_auth: bool = match config.get(TLS_ENABLE_MTLS) {
            Some(s) => s
                .parse()
                .map_err(|_| zerror!("Unknown enable mTLS auth argument: {}", s))?,
            None => TLS_ENABLE_MTLS_DEFAULT,
        };

        let tls_server_name_verification: bool = match config.get(TLS_VERIFY_NAME_ON_CONNECT) {
            Some(s) => s
                .parse()
                .map_err(|_| zerror!("Unknown server name verification argument: {}", s))?,
            None => TLS_VERIFY_NAME_ON_CONNECT_DEFAULT,
        };
        if !tls_server_name_verification {
            tracing::warn!("Skipping name verification of TLS server");
        }

        let tls_close_link_on_expiration: bool = match config.get(TLS_CLOSE_LINK_ON_EXPIRATION) {
            Some(s) => s
                .parse()
                .map_err(|_| zerror!("Unknown close on expiration argument: {}", s))?,
            None => TLS_CLOSE_LINK_ON_EXPIRATION_DEFAULT,
        };

        // Allows mixed user-generated CA and webPKI CA
        tracing::debug!("Loading default Web PKI certificates.");
        let mut root_cert_store = RootCertStore {
            roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
        };

        if let Some(custom_root_cert) = load_trust_anchors(config)? {
            tracing::debug!("Loading user-generated certificates.");
            root_cert_store.extend(custom_root_cert.roots);
        }

        // Install ring based rustls CryptoProvider.
        rustls::crypto::ring::default_provider()
            // This can be called successfully at most once in any process execution.
            // Call this early in your process to configure which provider is used for the provider.
            // The configuration should happen before any use of ClientConfig::builder() or ServerConfig::builder().
            .install_default()
            // Ignore the error here, because `rustls::crypto::ring::default_provider().install_default()` will inevitably be executed multiple times
            // when there are multiple quic links, and all but the first execution will fail.
            .ok();

        let cc = if tls_client_server_auth {
            tracing::debug!("Loading client authentication key and certificate...");
            let tls_client_private_key = TlsClientConfig::load_tls_private_key(config).await?;
            let tls_client_certificate = TlsClientConfig::load_tls_certificate(config).await?;

            let certs: Vec<CertificateDer> =
                rustls_pemfile::certs(&mut Cursor::new(&tls_client_certificate))
                    .collect::<Result<_, _>>()
                    .map_err(|err| zerror!("Error processing client certificate: {err}."))?;

            let mut keys: Vec<PrivateKeyDer> =
                rustls_pemfile::rsa_private_keys(&mut Cursor::new(&tls_client_private_key))
                    .map(|x| x.map(PrivateKeyDer::from))
                    .collect::<Result<_, _>>()
                    .map_err(|err| zerror!("Error processing client key: {err}."))?;

            if keys.is_empty() {
                keys =
                    rustls_pemfile::pkcs8_private_keys(&mut Cursor::new(&tls_client_private_key))
                        .map(|x| x.map(PrivateKeyDer::from))
                        .collect::<Result<_, _>>()
                        .map_err(|err| zerror!("Error processing client key: {err}."))?;
            }

            if keys.is_empty() {
                keys = rustls_pemfile::ec_private_keys(&mut Cursor::new(&tls_client_private_key))
                    .map(|x| x.map(PrivateKeyDer::from))
                    .collect::<Result<_, _>>()
                    .map_err(|err| zerror!("Error processing client key: {err}."))?;
            }

            if keys.is_empty() {
                bail!("No private key found for TLS client.");
            }

            let builder = ClientConfig::builder_with_protocol_versions(&[&TLS13]);

            if tls_server_name_verification {
                builder
                    .with_root_certificates(root_cert_store)
                    .with_client_auth_cert(certs, keys.remove(0))
            } else {
                builder
                    .dangerous()
                    .with_custom_certificate_verifier(Arc::new(WebPkiVerifierAnyServerName::new(
                        root_cert_store,
                    )))
                    .with_client_auth_cert(certs, keys.remove(0))
            }
            .map_err(|e| zerror!("Bad certificate/key: {}", e))?
        } else {
            let builder = ClientConfig::builder();
            if tls_server_name_verification {
                builder
                    .with_root_certificates(root_cert_store)
                    .with_no_client_auth()
            } else {
                builder
                    .dangerous()
                    .with_custom_certificate_verifier(Arc::new(WebPkiVerifierAnyServerName::new(
                        root_cert_store,
                    )))
                    .with_no_client_auth()
            }
        };

        let mut tcp_rx_buffer_size = None;
        if let Some(size) = config.get(TCP_SO_RCV_BUF) {
            tcp_rx_buffer_size = Some(
                size.parse()
                    .map_err(|_| zerror!("Unknown TCP read buffer size argument: {}", size))?,
            );
        };
        let mut tcp_tx_buffer_size = None;
        if let Some(size) = config.get(TCP_SO_SND_BUF) {
            tcp_tx_buffer_size = Some(
                size.parse()
                    .map_err(|_| zerror!("Unknown TCP write buffer size argument: {}", size))?,
            );
        };
        let mut bind_socket = None;
        if let Some(bind_socket_str) = config.get(BIND_SOCKET) {
            bind_socket = Some(get_tls_addr(&Address::from(bind_socket_str)).await?);
        };

        Ok(TlsClientConfig {
            client_config: cc,
            tls_close_link_on_expiration,
            tcp_socket_config: TcpSocketConfig::new(
                tcp_tx_buffer_size,
                tcp_rx_buffer_size,
                config.get(BIND_INTERFACE),
                bind_socket,
                parse_dscp(config)?,
            ),
        })
    }

    async fn load_tls_private_key(config: &Config<'_>) -> ZResult<Vec<u8>> {
        load_tls_key(
            config,
            TLS_CONNECT_PRIVATE_KEY_RAW,
            TLS_CONNECT_PRIVATE_KEY_FILE,
            TLS_CONNECT_PRIVATE_KEY_BASE64,
        )
        .await
    }

    async fn load_tls_certificate(config: &Config<'_>) -> ZResult<Vec<u8>> {
        load_tls_certificate(
            config,
            TLS_CONNECT_CERTIFICATE_RAW,
            TLS_CONNECT_CERTIFICATE_FILE,
            TLS_CONNECT_CERTIFICATE_BASE64,
        )
        .await
    }
}

fn process_pem(pem: &mut dyn io::BufRead) -> ZResult<Vec<TrustAnchor<'static>>> {
    let certs: Vec<CertificateDer> = rustls_pemfile::certs(pem)
        .map(|result| result.map_err(|err| zerror!("Error processing PEM certificates: {err}.")))
        .collect::<Result<Vec<CertificateDer>, ZError>>()?;

    let trust_anchors: Vec<TrustAnchor> = certs
        .into_iter()
        .map(|cert| {
            anchor_from_trusted_cert(&cert)
                .map_err(|err| zerror!("Error processing trust anchor: {err}."))
                .map(|trust_anchor| trust_anchor.to_owned())
        })
        .collect::<Result<Vec<TrustAnchor>, ZError>>()?;

    Ok(trust_anchors)
}

async fn load_tls_key(
    config: &Config<'_>,
    tls_private_key_raw_config_key: &str,
    tls_private_key_file_config_key: &str,
    tls_private_key_base64_config_key: &str,
) -> ZResult<Vec<u8>> {
    if let Some(value) = config.get(tls_private_key_raw_config_key) {
        return Ok(value.as_bytes().to_vec());
    }

    if let Some(b64_key) = config.get(tls_private_key_base64_config_key) {
        return base64_decode(b64_key);
    }

    if let Some(value) = config.get(tls_private_key_file_config_key) {
        return Ok(tokio::fs::read(value)
            .await
            .map_err(|e| zerror!("Invalid TLS private key file: {}", e))?)
        .and_then(|result| {
            if result.is_empty() {
                Err(zerror!("Empty TLS key.").into())
            } else {
                Ok(result)
            }
        });
    }
    Err(zerror!("Missing TLS private key.").into())
}

async fn load_tls_certificate(
    config: &Config<'_>,
    tls_certificate_raw_config_key: &str,
    tls_certificate_file_config_key: &str,
    tls_certificate_base64_config_key: &str,
) -> ZResult<Vec<u8>> {
    if let Some(value) = config.get(tls_certificate_raw_config_key) {
        return Ok(value.as_bytes().to_vec());
    }

    if let Some(b64_certificate) = config.get(tls_certificate_base64_config_key) {
        return base64_decode(b64_certificate);
    }

    if let Some(value) = config.get(tls_certificate_file_config_key) {
        return Ok(tokio::fs::read(value)
            .await
            .map_err(|e| zerror!("Invalid TLS certificate file: {}", e))?);
    }
    Err(zerror!("Missing tls certificates.").into())
}

fn load_trust_anchors(config: &Config<'_>) -> ZResult<Option<RootCertStore>> {
    let mut root_cert_store = RootCertStore::empty();
    if let Some(value) = config.get(TLS_ROOT_CA_CERTIFICATE_RAW) {
        let mut pem = BufReader::new(value.as_bytes());
        let trust_anchors = process_pem(&mut pem)?;
        root_cert_store.extend(trust_anchors);
        return Ok(Some(root_cert_store));
    }

    if let Some(b64_certificate) = config.get(TLS_ROOT_CA_CERTIFICATE_BASE64) {
        let certificate_pem = base64_decode(b64_certificate)?;
        let mut pem = BufReader::new(certificate_pem.as_slice());
        let trust_anchors = process_pem(&mut pem)?;
        root_cert_store.extend(trust_anchors);
        return Ok(Some(root_cert_store));
    }

    if let Some(filename) = config.get(TLS_ROOT_CA_CERTIFICATE_FILE) {
        let mut pem = BufReader::new(File::open(filename)?);
        let trust_anchors = process_pem(&mut pem)?;
        root_cert_store.extend(trust_anchors);
        return Ok(Some(root_cert_store));
    }
    Ok(None)
}

pub fn base64_decode(data: &str) -> ZResult<Vec<u8>> {
    use base64::{engine::general_purpose, Engine};
    Ok(general_purpose::STANDARD
        .decode(data)
        .map_err(|e| zerror!("Unable to perform base64 decoding: {e:?}"))?)
}

pub async fn get_tls_addr(address: &Address<'_>) -> ZResult<SocketAddr> {
    match tokio::net::lookup_host(address.as_str()).await?.next() {
        Some(addr) => Ok(addr),
        None => bail!("Couldn't resolve TLS locator address: {}", address),
    }
}

pub fn get_tls_host<'a>(address: &'a Address<'a>) -> ZResult<&'a str> {
    Ok(address
        .as_str()
        .rsplit_once(':')
        .ok_or_else(|| zerror!("Invalid TLS address"))?
        .0)
}

pub fn get_tls_server_name<'a>(address: &'a Address<'a>) -> ZResult<ServerName<'a>> {
    Ok(ServerName::try_from(get_tls_host(address)?).map_err(|e| zerror!(e))?)
}
