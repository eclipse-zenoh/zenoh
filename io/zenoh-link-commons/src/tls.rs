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
    use std::sync::Weak;

    use time::OffsetDateTime;
    use tokio::task::JoinHandle;
    use tokio_util::sync::CancellationToken;
    use zenoh_protocol::core::Locator;

    use crate::LinkUnicastTrait;

    #[derive(Debug)]
    pub struct LinkCertExpirationManager {
        pub token: CancellationToken,
        handle: Option<JoinHandle<()>>,
    }

    impl LinkCertExpirationManager {
        pub fn new(
            expiration_info: LinkCertExpirationInfo,
            token: Option<CancellationToken>,
        ) -> Self {
            let token = token.unwrap_or_default();
            let handle = zenoh_runtime::ZRuntime::Acceptor
                .spawn(expiration_task(expiration_info, token.clone()));
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
                zenoh_runtime::ZRuntime::Acceptor.block_in_place(async {
                    self.token.cancel();
                    let _ = handle.await;
                })
            }
        }
    }

    async fn expiration_task(expiration_info: LinkCertExpirationInfo, token: CancellationToken) {
        // TODO: Expose or tune sleep duration
        const MAX_EXPIRATION_SLEEP_DURATION: tokio::time::Duration =
            tokio::time::Duration::from_secs(600);

        tracing::trace!(
            "Expiration task started for link {} => {}",
            expiration_info.src_locator,
            expiration_info.dst_locator,
        );

        loop {
            let now = OffsetDateTime::now_utc();
            if expiration_info.expiration_time <= now {
                // close link
                if let Some(link) = expiration_info.link.upgrade() {
                    tracing::warn!(
                        "Closing link {} => {} : remote certificate chain expired",
                        expiration_info.src_locator,
                        expiration_info.dst_locator,
                    );
                    if let Err(e) = link.close().await {
                        tracing::error!(
                            "Error closing link {} => {} : {}",
                            expiration_info.src_locator,
                            expiration_info.dst_locator,
                            e
                        )
                    }
                }
                break;
            }
            // next sleep duration is the minimum between MAX_EXPIRATION_SLEEP_DURATION and the duration till next expiration
            // this mitigates the unsoundness of using `tokio::time::sleep_until` with long durations
            let next_expiration_duration = std::time::Duration::from_secs_f32(
                (expiration_info.expiration_time - now).as_seconds_f32(),
            );
            let next_wakeup_instant = tokio::time::Instant::now()
                + tokio::time::Duration::min(
                    MAX_EXPIRATION_SLEEP_DURATION,
                    next_expiration_duration,
                );

            tokio::select! {
                _ = token.cancelled() => break,
                _ = tokio::time::sleep_until(next_wakeup_instant) => {},
            }
        }
    }

    pub struct LinkCertExpirationInfo {
        // Weak is used instead of Arc, in order to allow cleanup at Drop of the underlying link which owns the expiration manager
        link: Weak<dyn LinkUnicastTrait>,
        expiration_time: OffsetDateTime,
        src_locator: Locator,
        dst_locator: Locator,
    }

    impl LinkCertExpirationInfo {
        pub fn new(
            link: Weak<dyn LinkUnicastTrait>,
            expiration_time: OffsetDateTime,
            src_locator: &Locator,
            dst_locator: &Locator,
        ) -> Self {
            Self {
                link,
                expiration_time,
                src_locator: src_locator.clone(),
                dst_locator: dst_locator.clone(),
            }
        }
    }
}
