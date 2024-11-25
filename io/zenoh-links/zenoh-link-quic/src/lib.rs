//
// Copyright (c) 2023 ZettaScale Technology
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

//! ⚠️ WARNING ⚠️
//!
//! This crate is intended for Zenoh's internal use.
//!
//! [Click here for Zenoh's documentation](https://docs.rs/zenoh/latest/zenoh)
use std::str::FromStr;

use async_trait::async_trait;
use zenoh_core::zconfigurable;
use zenoh_link_commons::LocatorInspector;
use zenoh_protocol::{
    core::{Locator, Metadata, Reliability},
    transport::BatchSize,
};
use zenoh_result::ZResult;

mod unicast;
mod utils;
pub use unicast::*;
pub use utils::TlsConfigurator as QuicConfigurator;

// Default ALPN protocol
pub const ALPN_QUIC_HTTP: &[&[u8]] = &[b"hq-29"];

// Default MTU (QUIC PDU) in bytes.
// NOTE: Since QUIC is a byte-stream oriented transport, theoretically it has
//       no limit regarding the MTU. However, given the batching strategy
//       adopted in Zenoh and the usage of 16 bits in Zenoh to encode the
//       payload length in byte-streamed, the QUIC MTU is constrained to
//       2^16 - 1 bytes (i.e., 65535).
const QUIC_MAX_MTU: BatchSize = BatchSize::MAX;
pub const QUIC_LOCATOR_PREFIX: &str = "quic";

const IS_RELIABLE: bool = true;

#[derive(Default, Clone, Copy, Debug)]
pub struct QuicLocatorInspector;

#[async_trait]
impl LocatorInspector for QuicLocatorInspector {
    fn protocol(&self) -> &str {
        QUIC_LOCATOR_PREFIX
    }

    async fn is_multicast(&self, _locator: &Locator) -> ZResult<bool> {
        Ok(false)
    }

    fn is_reliable(&self, locator: &Locator) -> ZResult<bool> {
        if let Some(reliability) = locator
            .metadata()
            .get(Metadata::RELIABILITY)
            .map(Reliability::from_str)
            .transpose()?
        {
            Ok(reliability == Reliability::Reliable)
        } else {
            Ok(IS_RELIABLE)
        }
    }
}

zconfigurable! {
    // Default MTU (QUIC PDU) in bytes.
    static ref QUIC_DEFAULT_MTU: BatchSize = QUIC_MAX_MTU;
    // The LINGER option causes the shutdown() call to block until (1) all application data is delivered
    // to the remote end or (2) a timeout expires. The timeout is expressed in seconds.
    // More info on the LINGER option and its dynamics can be found at:
    // https://blog.netherlabs.nl/articles/2009/01/18/the-ultimate-so_linger-page-or-why-is-my-tcp-not-reliable
    static ref QUIC_LINGER_TIMEOUT: i32 = 10;
    // Amount of time in microseconds to throttle the accept loop upon an error.
    // Default set to 100 ms.
    static ref QUIC_ACCEPT_THROTTLE_TIME: u64 = 100_000;
}

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
}
