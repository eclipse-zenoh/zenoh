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
use crate::core::{WhatAmI, ZInt, ZenohId};
use zenoh_buffers::ZSlice;

/// # Init message
///
/// ```text
/// NOTE: 16 bits (2 bytes) may be prepended to the serialized message indicating the total length
///       in bytes of the message, resulting in the maximum length of a message being 65_535 bytes.
///       This is necessary in those stream-oriented transports (e.g., TCP) that do not preserve
///       the boundary of the serialized messages. The length is encoded as little-endian.
///       In any case, the length of a message must not exceed 65_535 bytes.
///
/// The INIT message is sent on a specific Locator to initiate a transport with the peer associated
/// with that Locator. The initiator MUST send an INIT message with the A flag set to 0.  If the
/// corresponding peer deems appropriate to initialize a transport with the initiator, the corresponding
/// peer MUST reply with an INIT message with the A flag set to 1.
///
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |O|S|A|   INIT  |
/// +-+-+-+-+-------+
/// ~             |Q~ if O==1
/// +---------------+
/// | v_maj | v_min | if A==0 -- Protocol Version VMaj.VMin
/// +-------+-------+
/// ~    whatami    ~ -- Client, Router, Peer or a combination of them
/// +---------------+
/// ~    peer_id    ~ -- PID of the sender of the INIT message
/// +---------------+
/// ~ sn_resolution ~ if S==1 -- the sequence number resolution(*)
/// +---------------+
/// ~     cookie    ~ if A==1
/// +---------------+
///
/// (*) if A==0 and S==0 then 2^28 is assumed.
///     if A==1 and S==0 then the agreed resolution is the one communicated by the initiator.
///
/// - if Q==1 then the initiator/responder support QoS.
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitSyn {
    pub version: u8,
    pub whatami: WhatAmI,
    pub zid: ZenohId,
    pub sn_resolution: ZInt,
    pub is_qos: bool,
}

impl InitSyn {
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use crate::defaults::SEQ_NUM_RES;
        use rand::Rng;

        let mut rng = rand::thread_rng();

        let version: u8 = rng.gen();
        let whatami = WhatAmI::rand();
        let zid = ZenohId::default();
        let sn_resolution = if rng.gen_bool(0.5) {
            rng.gen()
        } else {
            SEQ_NUM_RES
        };
        let is_qos = rng.gen_bool(0.5);

        Self {
            version,
            whatami,
            zid,
            sn_resolution,
            is_qos,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitAck {
    pub whatami: WhatAmI,
    pub zid: ZenohId,
    pub sn_resolution: Option<ZInt>,
    pub is_qos: bool,
    pub cookie: ZSlice,
}

impl InitAck {
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use rand::Rng;

        const MIN: usize = 32;
        const MAX: usize = 1_024;

        let mut rng = rand::thread_rng();

        let whatami = WhatAmI::rand();
        let zid = ZenohId::default();
        let sn_resolution = if rng.gen_bool(0.5) {
            Some(rng.gen())
        } else {
            None
        };
        let is_qos = rng.gen_bool(0.5);
        let cookie = ZSlice::rand(rng.gen_range(MIN..=MAX));

        Self {
            whatami,
            zid,
            sn_resolution,
            is_qos,
            cookie,
        }
    }
}
