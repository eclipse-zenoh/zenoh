use crate::ConfigurationInspector;
use alloc::vec::Vec;
use rustls::{
    client::{
        danger::{ServerCertVerified, ServerCertVerifier},
        verify_server_cert_signed_by_trust_anchor,
    },
    crypto::{verify_tls12_signature, verify_tls13_signature},
    pki_types::{CertificateDer, PrivateKeyDer, ServerName, TrustAnchor, UnixTime},
    server::{ParsedCertificate, WebPkiClientVerifier},
    version::TLS13,
    ClientConfig, RootCertStore, ServerConfig,
};
use secrecy::ExposeSecret;
use std::{
    fs::File,
    io::{BufRead, BufReader, Cursor},
    sync::Arc,
};
use webpki::{anchor_from_trusted_cert, ALL_VERIFICATION_ALGS};
use zenoh_config::Config;
use zenoh_core::zerror;
use zenoh_protocol::core::endpoint;
use zenoh_result::{bail, ZError, ZResult};

use config::{
    TLS_CLIENT_AUTH, TLS_CLIENT_CERTIFICATE_BASE64, TLS_CLIENT_CERTIFICATE_FILE,
    TLS_CLIENT_CERTIFICATE_RAW, TLS_CLIENT_PRIVATE_KEY_BASE64, TLS_CLIENT_PRIVATE_KEY_FILE,
    TLS_CLIENT_PRIVATE_KEY_RAW, TLS_ROOT_CA_CERTIFICATE_BASE64, TLS_ROOT_CA_CERTIFICATE_FILE,
    TLS_ROOT_CA_CERTIFICATE_RAW, TLS_SERVER_CERTIFICATE_BASE64, TLS_SERVER_CERTIFICATE_FILE,
    TLS_SERVER_CERTIFICATE_RAW, TLS_SERVER_NAME_VERIFICATION, TLS_SERVER_PRIVATE_KEY_BASE_64,
    TLS_SERVER_PRIVATE_KEY_FILE, TLS_SERVER_PRIVATE_KEY_RAW,
};

pub fn base64_decode(data: &str) -> ZResult<Vec<u8>> {
    use base64::engine::general_purpose;
    use base64::Engine;
    Ok(general_purpose::STANDARD
        .decode(data)
        .map_err(|e| zerror!("Unable to perform base64 decoding: {e:?}"))?)
}

impl ServerCertVerifier for WebPkiVerifierAnyServerName {
    /// Will verify the certificate is valid in the following ways:
    /// - Signed by a  trusted `RootCertStore` CA
    /// - Not Expired
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let cert = ParsedCertificate::try_from(end_entity)?;
        verify_server_cert_signed_by_trust_anchor(
            &cert,
            &self.roots,
            intermediates,
            now,
            ALL_VERIFICATION_ALGS,
        )?;
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// `ServerCertVerifier` that verifies that the server is signed by a trusted root, but allows any serverName
/// see the trait impl for more information.
#[derive(Debug)]
pub struct WebPkiVerifierAnyServerName {
    roots: RootCertStore,
}

#[allow(unreachable_pub)]
impl WebPkiVerifierAnyServerName {
    /// Constructs a new `WebPkiVerifierAnyServerName`.
    ///
    /// `roots` is the set of trust anchors to trust for issuing server certs.
    pub fn new(roots: RootCertStore) -> Self {
        Self { roots }
    }
}

pub mod config {
    pub const TLS_ROOT_CA_CERTIFICATE_FILE: &str = "root_ca_certificate_file";
    pub const TLS_ROOT_CA_CERTIFICATE_RAW: &str = "root_ca_certificate_raw";
    pub const TLS_ROOT_CA_CERTIFICATE_BASE64: &str = "root_ca_certificate_base64";

    pub const TLS_SERVER_PRIVATE_KEY_FILE: &str = "server_private_key_file";
    pub const TLS_SERVER_PRIVATE_KEY_RAW: &str = "server_private_key_raw";
    pub const TLS_SERVER_PRIVATE_KEY_BASE_64: &str = "server_private_key_base64";

    pub const TLS_SERVER_CERTIFICATE_FILE: &str = "server_certificate_file";
    pub const TLS_SERVER_CERTIFICATE_RAW: &str = "server_certificate_raw";
    pub const TLS_SERVER_CERTIFICATE_BASE64: &str = "server_certificate_base64";

    pub const TLS_CLIENT_PRIVATE_KEY_FILE: &str = "client_private_key_file";
    pub const TLS_CLIENT_PRIVATE_KEY_RAW: &str = "client_private_key_raw";
    pub const TLS_CLIENT_PRIVATE_KEY_BASE64: &str = "client_private_key_base64";

    pub const TLS_CLIENT_CERTIFICATE_FILE: &str = "client_certificate_file";
    pub const TLS_CLIENT_CERTIFICATE_RAW: &str = "client_certificate_raw";
    pub const TLS_CLIENT_CERTIFICATE_BASE64: &str = "client_certificate_base64";

    pub const TLS_CLIENT_AUTH: &str = "client_auth";

    pub const TLS_SERVER_NAME_VERIFICATION: &str = "server_name_verification";
    pub const TLS_SERVER_NAME_VERIFICATION_DEFAULT: &str = "true";
}

#[derive(Default, Clone, Copy, Debug)]
pub struct TlsConfigurator;

impl ConfigurationInspector<Config> for TlsConfigurator {
    fn inspect_config(&self, config: &Config) -> ZResult<String> {
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
                    TLS_SERVER_PRIVATE_KEY_BASE_64,
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

        let mut s = String::new();
        endpoint::Parameters::extend(ps.drain(..), &mut s);

        Ok(s)
    }
}

pub struct TlsServerConfig {
    pub server_config: ServerConfig,
}

impl TlsServerConfig {
    pub async fn new(config: &endpoint::Config<'_>) -> ZResult<TlsServerConfig> {
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

    async fn load_tls_private_key(config: &endpoint::Config<'_>) -> ZResult<Vec<u8>> {
        load_tls_key(
            config,
            TLS_SERVER_PRIVATE_KEY_RAW,
            TLS_SERVER_PRIVATE_KEY_FILE,
            TLS_SERVER_PRIVATE_KEY_BASE_64,
        )
        .await
    }

    async fn load_tls_certificate(config: &endpoint::Config<'_>) -> ZResult<Vec<u8>> {
        load_tls_certificate(
            config,
            TLS_SERVER_CERTIFICATE_RAW,
            TLS_SERVER_CERTIFICATE_FILE,
            TLS_SERVER_CERTIFICATE_BASE64,
        )
        .await
    }
}

pub struct TlsClientConfig {
    pub client_config: ClientConfig,
}

impl TlsClientConfig {
    pub async fn new(config: &endpoint::Config<'_>) -> ZResult<TlsClientConfig> {
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
                    log::warn!("Skipping name verification of servers");
                }
                s
            }
            None => false,
        };

        // Allows mixed user-generated CA and webPKI CA
        log::debug!("Loading default Web PKI certificates.");
        let mut root_cert_store = RootCertStore {
            roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
        };

        if let Some(custom_root_cert) = load_trust_anchors(config)? {
            log::debug!("Loading user-generated certificates.");
            root_cert_store.extend(custom_root_cert.roots);
        }

        let cc = if tls_client_server_auth {
            log::debug!("Loading client authentication key and certificate...");
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

    async fn load_tls_private_key(config: &endpoint::Config<'_>) -> ZResult<Vec<u8>> {
        load_tls_key(
            config,
            TLS_CLIENT_PRIVATE_KEY_RAW,
            TLS_CLIENT_PRIVATE_KEY_FILE,
            TLS_CLIENT_PRIVATE_KEY_BASE64,
        )
        .await
    }

    async fn load_tls_certificate(config: &endpoint::Config<'_>) -> ZResult<Vec<u8>> {
        load_tls_certificate(
            config,
            TLS_CLIENT_CERTIFICATE_RAW,
            TLS_CLIENT_CERTIFICATE_FILE,
            TLS_CLIENT_CERTIFICATE_BASE64,
        )
        .await
    }
}

async fn load_tls_key(
    config: &endpoint::Config<'_>,
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
    config: &endpoint::Config<'_>,
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

fn load_trust_anchors(config: &endpoint::Config<'_>) -> ZResult<Option<RootCertStore>> {
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

fn process_pem(pem: &mut dyn BufRead) -> ZResult<Vec<TrustAnchor<'static>>> {
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
