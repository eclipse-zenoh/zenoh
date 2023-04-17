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
use crate::core::{Locator, WhatAmI, ZInt, ZenohId};
use alloc::vec::Vec;

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
pub struct LinkState {
    pub psid: ZInt,
    pub sn: ZInt,
    pub zid: Option<ZenohId>,
    pub whatami: Option<WhatAmI>,
    pub locators: Option<Vec<Locator>>,
    pub links: Vec<ZInt>,
}

impl LinkState {
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use rand::Rng;

        const MIN: usize = 1;
        const MAX: usize = 16;

        let mut rng = rand::thread_rng();

        let psid: ZInt = rng.gen();
        let sn: ZInt = rng.gen();
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
        let links = (0..n).map(|_| rng.gen()).collect::<Vec<ZInt>>();

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
pub struct LinkStateList {
    pub link_states: Vec<LinkState>,
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
