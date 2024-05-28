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
//! [Click here for Zenoh's documentation](../zenoh/index.html)
use async_trait::async_trait;
use zenoh_core::zconfigurable;
use zenoh_link_commons::LocatorInspector;
use zenoh_protocol::{core::Locator, transport::BatchSize};
use zenoh_result::ZResult;

mod unicast;
mod utils;
pub use unicast::*;
pub use utils::TlsConfigurator;

// Default MTU (TLS PDU) in bytes.
// NOTE: Since TLS is a byte-stream oriented transport, theoretically it has
//       no limit regarding the MTU. However, given the batching strategy
//       adopted in Zenoh and the usage of 16 bits in Zenoh to encode the
//       payload length in byte-streamed, the TLS MTU is constrained to
//       2^16 - 1 bytes (i.e., 65535).
const TLS_MAX_MTU: BatchSize = BatchSize::MAX;
pub const TLS_LOCATOR_PREFIX: &str = "tls";

#[derive(Default, Clone, Copy)]
pub struct TlsLocatorInspector;
#[async_trait]
impl LocatorInspector for TlsLocatorInspector {
    fn protocol(&self) -> &str {
        TLS_LOCATOR_PREFIX
    }

    async fn is_multicast(&self, _locator: &Locator) -> ZResult<bool> {
        Ok(false)
    }
}

zconfigurable! {
    // Default MTU (TLS PDU) in bytes.
    static ref TLS_DEFAULT_MTU: BatchSize = TLS_MAX_MTU;
    // The LINGER option causes the shutdown() call to block until (1) all application data is delivered
    // to the remote end or (2) a timeout expires. The timeout is expressed in seconds.
    // More info on the LINGER option and its dynamics can be found at:
    // https://blog.netherlabs.nl/articles/2009/01/18/the-ultimate-so_linger-page-or-why-is-my-tcp-not-reliable
    static ref TLS_LINGER_TIMEOUT: i32 = 10;
    // Amount of time in microseconds to throttle the accept loop upon an error.
    // Default set to 100 ms.
    static ref TLS_ACCEPT_THROTTLE_TIME: u64 = 100_000;
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
}
