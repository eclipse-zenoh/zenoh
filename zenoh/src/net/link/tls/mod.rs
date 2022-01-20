//
// Copyright (c) 2017, 2020 ADLINK Technology Inc.
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ADLINK zenoh team, <zenoh@adlink-labs.tech>
//
mod endpoint;
mod unicast;

use super::*;
pub use endpoint::*;
pub use unicast::*;
use zenoh_core::zconfigurable;

// Default MTU (TLS PDU) in bytes.
// NOTE: Since TLS is a byte-stream oriented transport, theoretically it has
//       no limit regarding the MTU. However, given the batching strategy
//       adopted in Zenoh and the usage of 16 bits in Zenoh to encode the
//       payload length in byte-streamed, the TLS MTU is constrained to
//       2^16 - 1 bytes (i.e., 65535).
const TLS_MAX_MTU: u16 = u16::MAX;

zconfigurable! {
    // Default MTU (TLS PDU) in bytes.
    static ref TLS_DEFAULT_MTU: u16 = TLS_MAX_MTU;
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
    use zenoh_util::properties::config::*;

    pub const TLS_ROOT_CA_CERTIFICATE_FILE: &str = ZN_TLS_ROOT_CA_CERTIFICATE_STR;
    pub const TLS_ROOT_CA_CERTIFICATE_RAW: &str = "tls_root_ca_certificate_raw";

    pub const TLS_SERVER_PRIVATE_KEY_FILE: &str = ZN_TLS_SERVER_PRIVATE_KEY_STR;
    pub const TLS_SERVER_PRIVATE_KEY_RAW: &str = "tls_server_private_key_raw";

    pub const TLS_SERVER_CERTIFICATE_FILE: &str = ZN_TLS_SERVER_CERTIFICATE_STR;
    pub const TLS_SERVER_CERTIFICATE_RAW: &str = "tls_server_certificate_raw";

    pub const TLS_CLIENT_PRIVATE_KEY_FILE: &str = ZN_TLS_CLIENT_PRIVATE_KEY_STR;
    pub const TLS_CLIENT_PRIVATE_KEY_RAW: &str = "tls_client_private_key_raw";

    pub const TLS_CLIENT_CERTIFICATE_FILE: &str = ZN_TLS_CLIENT_CERTIFICATE_STR;
    pub const TLS_CLIENT_CERTIFICATE_RAW: &str = "tls_client_certificate_raw";

    pub const TLS_CLIENT_AUTH: &str = ZN_TLS_CLIENT_AUTH_STR;
    pub const TLS_CLIENT_AUTH_DEFAULT: &str = ZN_TLS_CLIENT_AUTH_DEFAULT;
}
