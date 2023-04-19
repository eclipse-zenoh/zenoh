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
use crate::core::ZInt;
use core::time::Duration;
use zenoh_buffers::ZSlice;

/// # Open message
///
/// ```text
/// NOTE: 16 bits (2 bytes) may be prepended to the serialized message indicating the total length
///       in bytes of the message, resulting in the maximum length of a message being 65_535 bytes.
///       This is necessary in those stream-oriented transports (e.g., TCP) that do not preserve
///       the boundary of the serialized messages. The length is encoded as little-endian.
///       In any case, the length of a message must not exceed 65_535 bytes.
///
/// The OPEN message is sent on a link to finally open an initialized transport with the peer.
///
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |X|T|A|   OPEN  |
/// +-+-+-+-+-------+
/// ~    lease      ~ -- Lease period of the sender of the OPEN message(*)
/// +---------------+
/// ~  initial_sn   ~ -- Initial SN proposed by the sender of the OPEN(**)
/// +---------------+
/// ~    cookie     ~ if A==0(***)
/// +---------------+
///
/// (*)   if T==1 then the lease period is expressed in seconds, otherwise in milliseconds
/// (**)  the initial sequence number MUST be compatible with the sequence number resolution agreed in the
///       InitSyn/InitAck message exchange
/// (***) the cookie MUST be the same received in the INIT message with A==1 from the corresponding peer
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenSyn {
    pub lease: Duration,
    pub initial_sn: ZInt,
    pub cookie: ZSlice,
}

impl OpenSyn {
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use rand::Rng;

        const MIN: usize = 32;
        const MAX: usize = 1_024;

        let mut rng = rand::thread_rng();

        let lease = if rng.gen_bool(0.5) {
            Duration::from_secs(rng.gen())
        } else {
            Duration::from_millis(rng.gen())
        };

        let initial_sn: ZInt = rng.gen();
        let cookie = ZSlice::rand(rng.gen_range(MIN..=MAX));
        Self {
            lease,
            initial_sn,
            cookie,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAck {
    pub lease: Duration,
    pub initial_sn: ZInt,
}

impl OpenAck {
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use rand::Rng;

        let mut rng = rand::thread_rng();

        let lease = if rng.gen_bool(0.5) {
            Duration::from_secs(rng.gen())
        } else {
            Duration::from_millis(rng.gen())
        };

        let initial_sn: ZInt = rng.gen();
        Self { lease, initial_sn }
    }
}
