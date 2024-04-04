use alloc::vec::Vec;
use rustls::{
    client::{
        danger::{ServerCertVerified, ServerCertVerifier},
        verify_server_cert_signed_by_trust_anchor,
    },
    crypto::{verify_tls12_signature, verify_tls13_signature},
    pki_types::{CertificateDer, ServerName, UnixTime},
    server::ParsedCertificate,
    RootCertStore,
};
use webpki::ALL_VERIFICATION_ALGS;

use crate::ConfigurationInspector;
use secrecy::ExposeSecret;
use zenoh_config::Config;
use zenoh_protocol::core::endpoint;
use zenoh_result::{bail, ZResult};

use config::{
    TLS_CLIENT_AUTH, TLS_CLIENT_CERTIFICATE_BASE64, TLS_CLIENT_CERTIFICATE_FILE,
    TLS_CLIENT_PRIVATE_KEY_BASE64, TLS_CLIENT_PRIVATE_KEY_FILE, TLS_ROOT_CA_CERTIFICATE_BASE64,
    TLS_ROOT_CA_CERTIFICATE_FILE, TLS_SERVER_CERTIFICATE_BASE64, TLS_SERVER_CERTIFICATE_FILE,
    TLS_SERVER_NAME_VERIFICATION, TLS_SERVER_PRIVATE_KEY_BASE_64, TLS_SERVER_PRIVATE_KEY_FILE,
};

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
