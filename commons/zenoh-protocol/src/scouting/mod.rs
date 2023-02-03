//
// Copyright (c) 2022 ZettaScale Technology
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
pub mod hello;
pub mod scout;

use crate::{
    common::Attachment,
    core::{whatami::WhatAmIMatcher, Locator, WhatAmI, ZenohId},
};
use alloc::vec::Vec;
pub use hello::Hello;
pub use scout::Scout;

pub mod id {
    // Scouting Messages
    pub const SCOUT: u8 = 0x01;
    pub const HELLO: u8 = 0x02;
}

// Zenoh messages at scouting level
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum ScoutingBody {
    Scout(Scout),
    Hello(Hello),
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct ScoutingMessage {
    pub body: ScoutingBody,
    #[cfg(feature = "stats")]
    pub size: Option<core::num::NonZeroUsize>,
}

impl ScoutingMessage {
    pub fn make_scout(
        what: WhatAmIMatcher,
        zid: Option<ZenohId>,
        attachment: Option<Attachment>,
    ) -> ScoutingMessage {
        let version = crate::VERSION;
        ScoutingMessage {
            body: ScoutingBody::Scout(Scout { version, what, zid }),
            #[cfg(feature = "stats")]
            size: None,
        }
    }

    pub fn make_hello(
        whatami: WhatAmI,
        zid: ZenohId,
        locators: Vec<Locator>,
        attachment: Option<Attachment>,
    ) -> ScoutingMessage {
        let version = crate::VERSION;
        ScoutingMessage {
            body: ScoutingBody::Hello(Hello {
                version,
                whatami,
                zid,
                locators,
            }),
            #[cfg(feature = "stats")]
            size: None,
        }
    }

    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use rand::Rng;

        let mut rng = rand::thread_rng();

        let body = match rng.gen_range(0..2) {
            0 => ScoutingBody::Scout(Scout::rand()),
            1 => ScoutingBody::Hello(Hello::rand()),
            _ => unreachable!(),
        };

        Self { body }
    }
}
