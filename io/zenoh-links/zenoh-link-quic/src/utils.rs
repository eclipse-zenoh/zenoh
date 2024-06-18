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
use webpki::anchor_from_trusted_cert;
use zenoh_config::Config as ZenohConfig;
use zenoh_link_commons::{tls::WebPkiVerifierAnyServerName, ConfigurationInspector};
use zenoh_protocol::core::{
    endpoint::{Address, Config},
    parameters,
};
use zenoh_result::{bail, zerror, ZError, ZResult};

use crate::config::*;

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

        match (c.server_private_key(), c.server_private_key_base64()) {
            (Some(_), Some(_)) => {
                bail!("Only one between 'server_private_key' and 'server_private_key_base64' can be present!")
            }
            (Some(server_private_key), None) => {
                ps.push((TLS_SERVER_PRIVATE_KEY_FILE, server_private_key));
            }
            (None, Some(server_private_key)) => {
                ps.push((
                    TLS_SERVER_PRIVATE_KEY_BASE64,
                    server_private_key.expose_secret(),
                ));
            }
            _ => {}
        }

        match (c.server_certificate(), c.server_certificate_base64()) {
            (Some(_), Some(_)) => {
                bail!("Only one between 'server_certificate' and 'server_certificate_base64' can be present!")
            }
            (Some(server_certificate), None) => {
                ps.push((TLS_SERVER_CERTIFICATE_FILE, server_certificate));
            }
            (None, Some(server_certificate)) => {
                ps.push((
                    TLS_SERVER_CERTIFICATE_BASE64,
                    server_certificate.expose_secret(),
                ));
            }
            _ => {}
        }

        if let Some(client_auth) = c.client_auth() {
            match client_auth {
                true => ps.push((TLS_CLIENT_AUTH, "true")),
                false => ps.push((TLS_CLIENT_AUTH, "false")),
            };
        }

        match (c.client_private_key(), c.client_private_key_base64()) {
            (Some(_), Some(_)) => {
                bail!("Only one between 'client_private_key' and 'client_private_key_base64' can be present!")
            }
            (Some(client_private_key), None) => {
                ps.push((TLS_CLIENT_PRIVATE_KEY_FILE, client_private_key));
            }
            (None, Some(client_private_key)) => {
                ps.push((
                    TLS_CLIENT_PRIVATE_KEY_BASE64,
                    client_private_key.expose_secret(),
                ));
            }
            _ => {}
        }

        match (c.client_certificate(), c.client_certificate_base64()) {
            (Some(_), Some(_)) => {
                bail!("Only one between 'client_certificate' and 'client_certificate_base64' can be present!")
            }
            (Some(client_certificate), None) => {
                ps.push((TLS_CLIENT_CERTIFICATE_FILE, client_certificate));
            }
            (None, Some(client_certificate)) => {
                ps.push((
                    TLS_CLIENT_CERTIFICATE_BASE64,
                    client_certificate.expose_secret(),
                ));
            }
            _ => {}
        }

        if let Some(server_name_verification) = c.server_name_verification() {
            match server_name_verification {
                true => ps.push((TLS_SERVER_NAME_VERIFICATION, "true")),
                false => ps.push((TLS_SERVER_NAME_VERIFICATION, "false")),
            };
        }

        Ok(parameters::from_iter(ps.drain(..)))
    }
}

pub(crate) struct TlsServerConfig {
    pub(crate) server_config: ServerConfig,
}

impl TlsServerConfig {
    pub async fn new(config: &Config<'_>) -> ZResult<TlsServerConfig> {
        let tls_server_client_auth: bool = match config.get(TLS_CLIENT_AUTH) {
            Some(s) => s
                .parse()
                .map_err(|_| zerror!("Unknown client auth argument: {}", s))?,
            None => false,
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
                || {
                    Err(zerror!(
                        "Missing root certificates while client authentication is enabled."
                    ))
                },
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
        Ok(TlsServerConfig { server_config: sc })
    }

    async fn load_tls_private_key(config: &Config<'_>) -> ZResult<Vec<u8>> {
        load_tls_key(
            config,
            TLS_SERVER_PRIVATE_KEY_RAW,
            TLS_SERVER_PRIVATE_KEY_FILE,
            TLS_SERVER_PRIVATE_KEY_BASE64,
        )
        .await
    }

    async fn load_tls_certificate(config: &Config<'_>) -> ZResult<Vec<u8>> {
        load_tls_certificate(
            config,
            TLS_SERVER_CERTIFICATE_RAW,
            TLS_SERVER_CERTIFICATE_FILE,
            TLS_SERVER_CERTIFICATE_BASE64,
        )
        .await
    }
}

pub(crate) struct TlsClientConfig {
    pub(crate) client_config: ClientConfig,
}

impl TlsClientConfig {
    pub async fn new(config: &Config<'_>) -> ZResult<TlsClientConfig> {
        let tls_client_server_auth: bool = match config.get(TLS_CLIENT_AUTH) {
            Some(s) => s
                .parse()
                .map_err(|_| zerror!("Unknown client auth argument: {}", s))?,
            None => false,
        };

        let tls_server_name_verification: bool = match config.get(TLS_SERVER_NAME_VERIFICATION) {
            Some(s) => {
                let s: bool = s
                    .parse()
                    .map_err(|_| zerror!("Unknown server name verification argument: {}", s))?;
                if s {
                    tracing::warn!("Skipping name verification of servers");
                }
                s
            }
            None => false,
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
        Ok(TlsClientConfig { client_config: cc })
    }

    async fn load_tls_private_key(config: &Config<'_>) -> ZResult<Vec<u8>> {
        load_tls_key(
            config,
            TLS_CLIENT_PRIVATE_KEY_RAW,
            TLS_CLIENT_PRIVATE_KEY_FILE,
            TLS_CLIENT_PRIVATE_KEY_BASE64,
        )
        .await
    }

    async fn load_tls_certificate(config: &Config<'_>) -> ZResult<Vec<u8>> {
        load_tls_certificate(
            config,
            TLS_CLIENT_CERTIFICATE_RAW,
            TLS_CLIENT_CERTIFICATE_FILE,
            TLS_CLIENT_CERTIFICATE_BASE64,
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

pub fn base64_decode(data: &str) -> ZResult<Vec<u8>> {
    use base64::{engine::general_purpose, Engine};
    Ok(general_purpose::STANDARD
        .decode(data)
        .map_err(|e| zerror!("Unable to perform base64 decoding: {e:?}"))?)
}
