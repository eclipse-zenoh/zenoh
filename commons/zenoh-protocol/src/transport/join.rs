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
use crate::core::{ConduitSnList, WhatAmI, ZInt, ZenohId};
use core::time::Duration;

/// # Join message
///
/// ```text
/// NOTE: 16 bits (2 bytes) may be prepended to the serialized message indicating the total length
///       in bytes of the message, resulting in the maximum length of a message being 65_535 bytes.
///       This is necessary in those stream-oriented transports (e.g., TCP) that do not preserve
///       the boundary of the serialized messages. The length is encoded as little-endian.
///       In any case, the length of a message must not exceed 65_535 bytes.
///
/// The JOIN message is sent on a multicast Locator to advertise the transport parameters.
///
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |O|S|T|   JOIN  |
/// +-+-+-+-+-------+
/// ~             |Q~ if O==1
/// +---------------+
/// | v_maj | v_min | -- Protocol Version VMaj.VMin
/// +-------+-------+
/// ~    whatami    ~ -- Router, Peer or a combination of them
/// +---------------+
/// ~    peer_id    ~ -- PID of the sender of the JOIN message
/// +---------------+
/// ~     lease     ~ -- Lease period of the sender of the JOIN message(*)
/// +---------------+
/// ~ sn_resolution ~ if S==1(*) -- Otherwise 2^28 is assumed(**)
/// +---------------+
/// ~   [next_sn]   ~ (***)
/// +---------------+
///
/// - if Q==1 then the sender supports QoS.
///
/// (*)   if T==1 then the lease period is expressed in seconds, otherwise in milliseconds
/// (**)  if S==0 then 2^28 is assumed.
/// (***) if Q==1 then 8 sequence numbers are present: one for each priority.
///       if Q==0 then only one sequence number is present.
///
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Join {
    pub version: u8,
    pub whatami: WhatAmI,
    pub zid: ZenohId,
    pub lease: Duration,
    pub sn_resolution: ZInt,
    pub next_sns: ConduitSnList,
}

impl Join {
    pub fn is_qos(&self) -> bool {
        match self.next_sns {
            ConduitSnList::QoS(_) => true,
            ConduitSnList::Plain(_) => false,
        }
    }
}

impl Join {
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use crate::core::{ConduitSn, Priority};
        use rand::Rng;

        let mut rng = rand::thread_rng();

        let version: u8 = rng.gen();
        let whatami = WhatAmI::rand();
        let zid = ZenohId::default();
        let lease = if rng.gen_bool(0.5) {
            Duration::from_secs(rng.gen())
        } else {
            Duration::from_millis(rng.gen())
        };
        let sn_resolution: ZInt = rng.gen();
        let next_sns = if rng.gen_bool(0.5) {
            let mut sns = Box::new([ConduitSn::default(); Priority::NUM]);
            for i in 0..Priority::NUM {
                sns[i].reliable = rng.gen();
                sns[i].best_effort = rng.gen();
            }
            ConduitSnList::QoS(sns)
        } else {
            ConduitSnList::Plain(ConduitSn {
                reliable: rng.gen(),
                best_effort: rng.gen(),
            })
        };

        Self {
            version,
            whatami,
            zid,
            lease,
            sn_resolution,
            next_sns,
        }
    }
}
