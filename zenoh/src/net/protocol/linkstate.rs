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
use std::num::NonZeroU16;

use zenoh_protocol::core::{Locator, WhatAmI, ZenohIdProto};

pub const PID: u64 = 1; // 0x01
pub const WAI: u64 = 1 << 1; // 0x02
pub const LOC: u64 = 1 << 2; // 0x04
pub const WGT: u64 = 1 << 3; // 0x08

//  7 6 5 4 3 2 1 0
// +-+-+-+-+-+-+-+-+
// ~X|X|X|X|H|L|W|P~
// +-+-+-+-+-+-+-+-+
// ~     psid      ~
// +---------------+
// ~      sn       ~
// +---------------+
// ~      zid      ~ if P == 1
// +---------------+
// ~    whatami    ~ if W == 1
// +---------------+
// ~  [locators]   ~ if L == 1
// +---------------+
// ~    [links]    ~
// +---------------+
// ~    [weights]  ~ if H = 1
// +---------------+
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LinkState {
    pub(crate) psid: u64,
    pub(crate) sn: u64,
    pub(crate) zid: Option<ZenohIdProto>,
    pub(crate) whatami: Option<WhatAmI>,
    pub(crate) locators: Option<Vec<Locator>>,
    pub(crate) links: Vec<u64>,
    pub(crate) link_weights: Option<Vec<u16>>,
}

#[derive(Default, Copy, Debug, Clone, PartialEq, Eq)]
pub(crate) struct LinkEdgeWeight(pub(crate) Option<NonZeroU16>);

impl LinkEdgeWeight {
    const DEFAULT_LINK_WEIGHT: u16 = 100;

    pub(crate) fn new(val: NonZeroU16) -> Self {
        LinkEdgeWeight(Some(val))
    }

    pub(crate) fn from_raw(mut val: u16) -> Self {
        if val == 0 {
            val = Self::DEFAULT_LINK_WEIGHT;
        }
        LinkEdgeWeight(NonZeroU16::new(val))
    }

    pub(crate) fn value(&self) -> u16 {
        match self.0 {
            Some(v) => v.get(),
            None => Self::DEFAULT_LINK_WEIGHT,
        }
    }

    pub(crate) fn as_raw(&self) -> u16 {
        match self.0 {
            Some(v) => v.get(),
            None => 0,
        }
    }

    pub(crate) fn is_set(&self) -> bool {
        self.0.is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LinkEdge {
    pub(crate) dest: ZenohIdProto,
    pub(crate) weight: LinkEdgeWeight,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LocalLinkState {
    pub(crate) sn: u64,
    pub(crate) zid: ZenohIdProto,
    pub(crate) whatami: WhatAmI,
    pub(crate) locators: Option<Vec<Locator>>,
    pub(crate) links: Vec<LinkEdge>,
}

impl LinkState {
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use rand::Rng;

        const MIN: usize = 1;
        const MAX: usize = 16;

        let mut rng = rand::thread_rng();

        let psid: u64 = rng.gen();
        let sn: u64 = rng.gen();
        let zid = if rng.gen_bool(0.5) {
            Some(ZenohIdProto::default())
        } else {
            None
        };
        let whatami = if rng.gen_bool(0.5) {
            Some(WhatAmI::rand())
        } else {
            None
        };
        let locators = if rng.gen_bool(0.5) {
            let n = rng.gen_range(MIN..=MAX);
            let locators = (0..n).map(|_| Locator::rand()).collect::<Vec<Locator>>();
            Some(locators)
        } else {
            None
        };
        let n = rng.gen_range(MIN..=MAX);
        let links = (0..n).map(|_| rng.gen()).collect::<Vec<u64>>();

        Self {
            psid,
            sn,
            zid,
            whatami,
            locators,
            links,
        }
    }
}

//  7 6 5 4 3 2 1 0
// +-+-+-+-+-+-+-+-+
// |X|X|X|LK_ST_LS |
// +-+-+-+---------+
// ~ [link_states] ~
// +---------------+
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LinkStateList {
    pub(crate) link_states: Vec<LinkState>,
}

impl LinkStateList {
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use rand::Rng;

        const MIN: usize = 1;
        const MAX: usize = 16;

        let mut rng = rand::thread_rng();

        let n = rng.gen_range(MIN..=MAX);
        let link_states = (0..n)
            .map(|_| LinkState::rand())
            .collect::<Vec<LinkState>>();

        Self { link_states }
    }
}
