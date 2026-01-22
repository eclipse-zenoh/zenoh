//
// Copyright (c) 2025 ZettaScale Technology
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
    fmt::{self, Debug},
    fs::File,
    io,
    io::{BufReader, Cursor},
    net::SocketAddr,
    sync::Arc,
};

use rustls::{
    pki_types::{CertificateDer, PrivateKeyDer, TrustAnchor},
    server::WebPkiClientVerifier,
    version::TLS13,
    ClientConfig, RootCertStore, ServerConfig,
};
use secrecy::ExposeSecret;
use time::OffsetDateTime;
use webpki::anchor_from_trusted_cert;
use x509_parser::prelude::{FromDer, X509Certificate};
use zenoh_config::Config as ZenohConfig;
use zenoh_protocol::core::{
    endpoint::{Address, Config},
    parameters,
};
use zenoh_result::{bail, zerror, ZError, ZResult};

use crate::{
    tls::{config::*, WebPkiVerifierAnyServerName},
    ConfigurationInspector, LinkAuthId, BIND_INTERFACE,
};

// Default ALPN protocol
pub const ALPN_QUIC_HTTP: &[&[u8]] = &[b"hq-29"];

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
                bail!("Only one between 'listen_private_key' and 'listen_private_key_base64' can be present!")
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

        Ok(parameters::from_iter(ps.drain(..)))
    }
}

pub struct TlsServerConfig<'a> {
    pub server_config: ServerConfig,
    pub tls_close_link_on_expiration: bool,
    pub bind_iface: Option<&'a str>,
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
        Ok(TlsServerConfig {
            server_config: sc,
            tls_close_link_on_expiration,
            bind_iface: config.get(BIND_INTERFACE),
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

pub struct TlsClientConfig<'a> {
    pub client_config: ClientConfig,
    pub tls_close_link_on_expiration: bool,
    pub bind_iface: Option<&'a str>,
}

impl<'a> TlsClientConfig<'a> {
    pub async fn new(config: &'a Config<'_>) -> ZResult<Self> {
        let tls_client_server_auth: bool = match config.get(TLS_ENABLE_MTLS) {
            Some(s) => s
                .parse()
                .map_err(|_| zerror!("Unknown enable mTLS argument: {}", s))?,
            None => TLS_ENABLE_MTLS_DEFAULT,
        };

        let tls_server_name_verification: bool = match config.get(TLS_VERIFY_NAME_ON_CONNECT) {
            Some(s) => s
                .parse()
                .map_err(|_| zerror!("Unknown server name verification argument: {}", s))?,
            None => TLS_VERIFY_NAME_ON_CONNECT_DEFAULT,
        };
        if !tls_server_name_verification {
            tracing::warn!("Skipping name verification of QUIC server");
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
        Ok(TlsClientConfig {
            client_config: cc,
            tls_close_link_on_expiration,
            bind_iface: config.get(BIND_INTERFACE),
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

pub async fn get_quic_addr(address: &Address<'_>) -> ZResult<SocketAddr> {
    match tokio::net::lookup_host(address.as_str()).await?.next() {
        Some(addr) => Ok(addr),
        None => bail!("Couldn't resolve QUIC locator address: {}", address),
    }
}

pub fn get_quic_host<'a>(address: &'a Address<'a>) -> ZResult<&'a str> {
    Ok(address
        .as_str()
        .rsplit_once(':')
        .ok_or_else(|| zerror!("Invalid QUIC address"))?
        .0)
}

pub fn base64_decode(data: &str) -> ZResult<Vec<u8>> {
    use base64::{engine::general_purpose, Engine};
    Ok(general_purpose::STANDARD
        .decode(data)
        .map_err(|e| zerror!("Unable to perform base64 decoding: {e:?}"))?)
}

pub fn get_cert_common_name(conn: &quinn::Connection) -> ZResult<QuicAuthId> {
    let mut auth_id = QuicAuthId { auth_value: None };
    if let Some(pi) = conn.peer_identity() {
        let serv_certs = pi
            .downcast::<Vec<rustls_pki_types::CertificateDer>>()
            .unwrap();
        if let Some(item) = serv_certs.iter().next() {
            let (_, cert) = X509Certificate::from_der(item.as_ref()).unwrap();
            let subject_name = cert
                .subject
                .iter_common_name()
                .next()
                .and_then(|cn| cn.as_str().ok())
                .unwrap();
            auth_id = QuicAuthId {
                auth_value: Some(subject_name.to_string()),
            };
        }
    }
    Ok(auth_id)
}

/// Returns the minimum value of the `not_after` field in the remote certificate chain.
/// Returns `None` if the remote certificate chain is empty
pub fn get_cert_chain_expiration(conn: &quinn::Connection) -> ZResult<Option<OffsetDateTime>> {
    let mut link_expiration: Option<OffsetDateTime> = None;
    if let Some(pi) = conn.peer_identity() {
        if let Ok(remote_certs) = pi.downcast::<Vec<rustls_pki_types::CertificateDer>>() {
            for cert in *remote_certs {
                let (_, cert) = X509Certificate::from_der(cert.as_ref())?;
                let cert_expiration = cert.validity().not_after.to_datetime();
                link_expiration = link_expiration
                    .map(|current_min| current_min.min(cert_expiration))
                    .or(Some(cert_expiration));
            }
        }
    }
    Ok(link_expiration)
}

#[derive(Clone)]
pub struct QuicAuthId {
    auth_value: Option<String>,
}

impl Debug for QuicAuthId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Common Name: {}",
            self.auth_value.as_deref().unwrap_or("None")
        )
    }
}

impl From<QuicAuthId> for LinkAuthId {
    fn from(value: QuicAuthId) -> Self {
        LinkAuthId::Quic(value.auth_value.clone())
    }
}
