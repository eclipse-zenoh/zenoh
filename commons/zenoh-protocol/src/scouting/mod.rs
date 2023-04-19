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
mod hello;
mod scout;

use crate::{
    common::Attachment,
    core::{whatami::WhatAmIMatcher, Locator, WhatAmI, ZenohId},
};
use alloc::vec::Vec;
pub use hello::*;
pub use scout::*;

// Zenoh messages at scouting level
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScoutingBody {
    Scout(Scout),
    Hello(Hello),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScoutingMessage {
    pub body: ScoutingBody,
    pub attachment: Option<Attachment>,
    #[cfg(feature = "stats")]
    pub size: Option<core::num::NonZeroUsize>,
}

impl ScoutingMessage {
    pub fn make_scout(
        what: Option<WhatAmIMatcher>,
        zid_request: bool,
        attachment: Option<Attachment>,
    ) -> ScoutingMessage {
        ScoutingMessage {
            body: ScoutingBody::Scout(Scout { what, zid_request }),
            attachment,
            #[cfg(feature = "stats")]
            size: None,
        }
    }

    pub fn make_hello(
        zid: Option<ZenohId>,
        whatami: Option<WhatAmI>,
        locators: Option<Vec<Locator>>,
        attachment: Option<Attachment>,
    ) -> ScoutingMessage {
        let whatami = whatami.unwrap_or(WhatAmI::Router);
        let locators = locators.unwrap_or_default();

        ScoutingMessage {
            body: ScoutingBody::Hello(Hello {
                zid,
                whatami,
                locators,
            }),
            attachment,
            #[cfg(feature = "stats")]
            size: None,
        }
    }

    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use rand::Rng;

        let mut rng = rand::thread_rng();

        let attachment = if rng.gen_bool(0.5) {
            Some(Attachment::rand())
        } else {
            None
        };

        let body = match rng.gen_range(0..2) {
            0 => ScoutingBody::Hello(Hello::rand()),
            1 => ScoutingBody::Scout(Scout::rand()),
            _ => unreachable!(),
        };

        Self { body, attachment }
    }
}
