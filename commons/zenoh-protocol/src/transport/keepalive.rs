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
use crate::core::ZenohId;

/// # KeepAlive message
///
/// ```text
/// NOTE: 16 bits (2 bytes) may be prepended to the serialized message indicating the total length
///       in bytes of the message, resulting in the maximum length of a message being 65_535 bytes.
///       This is necessary in those stream-oriented transports (e.g., TCP) that do not preserve
///       the boundary of the serialized messages. The length is encoded as little-endian.
///       In any case, the length of a message must not exceed 65_535 bytes.
///
/// The KEEP_ALIVE message can be sent periodically to avoid the expiration of the transport lease
/// period in case there are no messages to be sent.
///
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |X|X|I| K_ALIVE |
/// +-+-+-+-+-------+
/// ~    zenoh_id   ~ if I==1 -- Zenoh ID of the KEEP_ALIVE sender.
/// +---------------+
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeepAlive {
    pub zid: Option<ZenohId>,
}

impl KeepAlive {
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use rand::Rng;

        let mut rng = rand::thread_rng();

        let zid = if rng.gen_bool(0.5) {
            Some(ZenohId::default())
        } else {
            None
        };

        Self { zid }
    }
}
