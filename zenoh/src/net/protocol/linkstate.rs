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
use std::{collections::HashMap, fmt::Debug, num::NonZeroU16};

use zenoh_config::TransportWeight;
use zenoh_protocol::core::{Locator, WhatAmI, ZenohIdProto};
use zenoh_result::ZResult;

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

#[derive(Default, Copy, Clone, PartialEq, Eq)]
pub(crate) struct LinkEdgeWeight(pub(crate) Option<NonZeroU16>);

impl Debug for LinkEdgeWeight {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.0 {
            Some(w) => w.fmt(f),
            None => self.0.fmt(f),
        }
    }
}

impl LinkEdgeWeight {
    const DEFAULT_LINK_WEIGHT: u16 = 100;

    pub(crate) fn new(val: NonZeroU16) -> Self {
        LinkEdgeWeight(Some(val))
    }

    pub(crate) fn from_raw(val: u16) -> Self {
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
pub(crate) struct LocalLinkState {
    pub(crate) sn: u64,
    pub(crate) zid: ZenohIdProto,
    pub(crate) whatami: WhatAmI,
    pub(crate) locators: Option<Vec<Locator>>,
    pub(crate) links: HashMap<ZenohIdProto, LinkEdgeWeight>,
}

impl LinkState {
    #[cfg(test)]
    #[doc(hidden)]
    #[allow(dead_code)]
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
        let link_weights = if rng.gen_bool(0.5) {
            let n = rng.gen_range(MIN..=MAX);
            let weights = (0..n).map(|_| rng.gen()).collect::<Vec<u16>>();
            Some(weights)
        } else {
            None
        };

        Self {
            psid,
            sn,
            zid,
            whatami,
            locators,
            links,
            link_weights,
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
    #[cfg(test)]
    #[doc(hidden)]
    #[allow(dead_code)]
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

pub(crate) fn link_weights_from_config(
    link_weights: Vec<TransportWeight>,
    network_name: &str,
) -> ZResult<HashMap<ZenohIdProto, LinkEdgeWeight>> {
    let mut link_weights_by_zid = HashMap::new();
    for lw in link_weights {
        if link_weights_by_zid
            .insert(lw.dst_zid.into(), LinkEdgeWeight::new(lw.weight))
            .is_some()
        {
            bail!(
                "{} config contains a duplicate zid value for transport weight: {}",
                network_name,
                lw.dst_zid
            );
        }
    }
    Ok(link_weights_by_zid)
}

impl From<LinkEdgeWeight> for Option<u16> {
    fn from(value: LinkEdgeWeight) -> Self {
        value.is_set().then_some(value.value())
    }
}

#[derive(PartialEq, Debug, serde::Serialize)]
pub(crate) struct LinkInfo {
    pub(crate) src_weight: Option<u16>,
    pub(crate) dst_weight: Option<u16>,
    pub(crate) actual_weight: u16,
}
