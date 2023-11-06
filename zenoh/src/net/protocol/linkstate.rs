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
use zenoh_protocol::core::{Locator, WhatAmI, ZenohId};

pub const PID: u64 = 1; // 0x01
pub const WAI: u64 = 1 << 1; // 0x02
pub const LOC: u64 = 1 << 2; // 0x04

//  7 6 5 4 3 2 1 0
// +-+-+-+-+-+-+-+-+
// ~X|X|X|X|X|L|W|P~
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LinkState {
    pub(crate) psid: u64,
    pub(crate) sn: u64,
    pub(crate) zid: Option<ZenohId>,
    pub(crate) whatami: Option<WhatAmI>,
    pub(crate) locators: Option<Vec<Locator>>,
    pub(crate) links: Vec<u64>,
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
            Some(ZenohId::default())
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
