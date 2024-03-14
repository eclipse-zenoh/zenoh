use rustls::client::verify_server_cert_signed_by_trust_anchor;
use rustls::server::ParsedCertificate;
use std::time::SystemTime;
use tokio_rustls::rustls::{
    client::{ServerCertVerified, ServerCertVerifier},
    Certificate, RootCertStore, ServerName,
};

impl ServerCertVerifier for WebPkiVerifierAnyServerName {
    /// Will verify the certificate is valid in the following ways:
    /// - Signed by a  trusted `RootCertStore` CA
    /// - Not Expired
    fn verify_server_cert(
        &self,
        end_entity: &Certificate,
        intermediates: &[Certificate],
        _server_name: &ServerName,
        _scts: &mut dyn Iterator<Item = &[u8]>,
        _ocsp_response: &[u8],
        now: SystemTime,
    ) -> Result<ServerCertVerified, tokio_rustls::rustls::Error> {
        let cert = ParsedCertificate::try_from(end_entity)?;
        verify_server_cert_signed_by_trust_anchor(&cert, &self.roots, intermediates, now)?;
        Ok(ServerCertVerified::assertion())
    }
}

/// `ServerCertVerifier` that verifies that the server is signed by a trusted root, but allows any serverName
/// see the trait impl for more information.
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
