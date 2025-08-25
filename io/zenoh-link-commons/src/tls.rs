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

pub mod config {
    pub const TLS_ROOT_CA_CERTIFICATE_FILE: &str = "root_ca_certificate_file";
    pub const TLS_ROOT_CA_CERTIFICATE_RAW: &str = "root_ca_certificate_raw";
    pub const TLS_ROOT_CA_CERTIFICATE_BASE64: &str = "root_ca_certificate_base64";

    pub const TLS_LISTEN_PRIVATE_KEY_FILE: &str = "listen_private_key_file";
    pub const TLS_LISTEN_PRIVATE_KEY_RAW: &str = "listen_private_key_raw";
    pub const TLS_LISTEN_PRIVATE_KEY_BASE64: &str = "listen_private_key_base64";

    pub const TLS_LISTEN_CERTIFICATE_FILE: &str = "listen_certificate_file";
    pub const TLS_LISTEN_CERTIFICATE_RAW: &str = "listen_certificate_raw";
    pub const TLS_LISTEN_CERTIFICATE_BASE64: &str = "listen_certificate_base64";

    pub const TLS_CONNECT_PRIVATE_KEY_FILE: &str = "connect_private_key_file";
    pub const TLS_CONNECT_PRIVATE_KEY_RAW: &str = "connect_private_key_raw";
    pub const TLS_CONNECT_PRIVATE_KEY_BASE64: &str = "connect_private_key_base64";

    pub const TLS_CONNECT_CERTIFICATE_FILE: &str = "connect_certificate_file";
    pub const TLS_CONNECT_CERTIFICATE_RAW: &str = "connect_certificate_raw";
    pub const TLS_CONNECT_CERTIFICATE_BASE64: &str = "connect_certificate_base64";

    pub const TLS_ENABLE_MTLS: &str = "enable_mtls";
    pub const TLS_ENABLE_MTLS_DEFAULT: bool = false;

    pub const TLS_VERIFY_NAME_ON_CONNECT: &str = "verify_name_on_connect";
    pub const TLS_VERIFY_NAME_ON_CONNECT_DEFAULT: bool = true;

    pub const TLS_CLOSE_LINK_ON_EXPIRATION: &str = "close_link_on_expiration";
    pub const TLS_CLOSE_LINK_ON_EXPIRATION_DEFAULT: bool = false;

    /// The time duration in milliseconds to wait for the TLS handshake to complete.
    pub const TLS_HANDSHAKE_TIMEOUT_MS: &str = "tls_handshake_timeout_ms";
    pub const TLS_HANDSHAKE_TIMEOUT_MS_DEFAULT: u64 = 10_000;
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

pub mod expiration {
    use std::{
        net::SocketAddr,
        sync::{atomic::AtomicBool, Weak},
    };

    use async_trait::async_trait;
    use time::OffsetDateTime;
    use tokio::{sync::Mutex as AsyncMutex, task::JoinHandle};
    use tokio_util::sync::CancellationToken;
    use zenoh_result::ZResult;

    #[async_trait]
    pub trait LinkWithCertExpiration: Send + Sync {
        async fn expire(&self) -> ZResult<()>;
    }

    #[derive(Debug)]
    pub struct LinkCertExpirationManager {
        token: CancellationToken,
        handle: AsyncMutex<Option<JoinHandle<ZResult<()>>>>,
        /// Closing the link is a critical section that requires exclusive access to expiration_task
        /// or the transport. `link_closing` is used to synchronize the access to this operation.
        link_closing: AtomicBool,
    }

    impl LinkCertExpirationManager {
        pub fn new(
            link: Weak<dyn LinkWithCertExpiration>,
            src_addr: SocketAddr,
            dst_addr: SocketAddr,
            link_type: &'static str,
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
                handle: AsyncMutex::new(Some(handle)),
                link_closing: AtomicBool::new(false),
            }
        }

        /// Takes exclusive access to closing the link.
        ///
        /// Returns `true` if successful, `false` if another task is (or finished) closing the link.
        pub fn set_closing(&self) -> bool {
            !self
                .link_closing
                .swap(true, std::sync::atomic::Ordering::Relaxed)
        }

        /// Sends cancelation signal to expiration_task
        pub fn cancel_expiration_task(&self) {
            self.token.cancel()
        }

        /// Waits for expiration task to complete, returning its return value.
        pub async fn wait_for_expiration_task(&self) -> ZResult<()> {
            let mut lock = self.handle.lock().await;
            let handle = lock.take().expect("handle should be set");
            handle.await?
        }
    }

    async fn expiration_task(
        link: Weak<dyn LinkWithCertExpiration>,
        src_addr: SocketAddr,
        dst_addr: SocketAddr,
        link_type: &'static str,
        expiration_time: OffsetDateTime,
        token: CancellationToken,
    ) -> ZResult<()> {
        tracing::trace!(
            "Expiration task started for {} link {:?} => {:?}",
            link_type.to_uppercase(),
            src_addr,
            dst_addr,
        );
        tokio::select! {
            _ = token.cancelled() => {},
            _ = sleep_until_date(expiration_time) => {
                // expire the link
                if let Some(link) = link.upgrade() {
                    tracing::warn!(
                        "Closing {} link {:?} => {:?} : remote certificate chain expired",
                        link_type.to_uppercase(),
                        src_addr,
                        dst_addr,
                    );
                    return link.expire().await;
                }
            },
        }
        Ok(())
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
