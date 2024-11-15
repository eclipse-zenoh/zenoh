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

pub mod expiration {
    use std::{net::SocketAddr, sync::Weak};

    use time::OffsetDateTime;
    use tokio::task::JoinHandle;
    use tokio_util::sync::CancellationToken;

    use crate::LinkUnicastTrait;

    #[derive(Debug)]
    pub struct LinkCertExpirationManager {
        token: CancellationToken,
        handle: Option<JoinHandle<()>>,
    }

    impl LinkCertExpirationManager {
        pub fn new(
            link: Weak<dyn LinkUnicastTrait>,
            src_addr: SocketAddr,
            dst_addr: SocketAddr,
            link_type: String,
            expiration_time: OffsetDateTime,
        ) -> Self {
            let token = CancellationToken::new();
            let handle = zenoh_runtime::ZRuntime::Acceptor.spawn(expiration_task(
                link,
                src_addr,
                dst_addr,
                link_type,
                expiration_time,
                token.clone(),
            ));
            Self {
                token,
                handle: Some(handle),
            }
        }
    }

    // Cleanup expiration task when manager is dropped
    impl Drop for LinkCertExpirationManager {
        fn drop(&mut self) {
            if let Some(handle) = self.handle.take() {
                self.token.cancel();
                zenoh_runtime::ZRuntime::Acceptor.block_in_place(async {
                    let _ = handle.await;
                })
            }
        }
    }

    async fn expiration_task(
        link: Weak<dyn LinkUnicastTrait>,
        src_addr: SocketAddr,
        dst_addr: SocketAddr,
        link_type: String,
        expiration_time: OffsetDateTime,
        token: CancellationToken,
    ) {
        tracing::trace!(
            "Expiration task started for {} link {:?} => {:?}",
            link_type.to_uppercase(),
            src_addr,
            dst_addr,
        );
        tokio::select! {
            _ = token.cancelled() => {},
            _ = sleep_until_date(expiration_time) => {
                // close link
                if let Some(link) = link.upgrade() {
                    tracing::warn!(
                        "Closing {} link {:?} => {:?} : remote certificate chain expired",
                        link_type.to_uppercase(),
                        src_addr,
                        dst_addr,
                    );
                    if let Err(e) = link.close().await {
                        tracing::error!(
                            "Error closing {} link {:?} => {:?} : {}",
                            link_type.to_uppercase(),
                            src_addr,
                            dst_addr,
                            e
                        )
                    }
                }
            },
        }
    }

    async fn sleep_until_date(wakeup_time: OffsetDateTime) {
        const MAX_SLEEP_DURATION: tokio::time::Duration = tokio::time::Duration::from_secs(600);
        loop {
            let now = OffsetDateTime::now_utc();
            if wakeup_time <= now {
                break;
            }
            // next sleep duration is the minimum between MAX_SLEEP_DURATION and the duration till wakeup
            // this mitigates the unsoundness of using `tokio::time::sleep` with long durations
            let wakeup_duration = std::time::Duration::try_from(wakeup_time - now)
                .expect("wakeup_time should be greater than now");
            let sleep_duration = tokio::time::Duration::min(MAX_SLEEP_DURATION, wakeup_duration);
            tokio::time::sleep(sleep_duration).await;
        }
    }
}
