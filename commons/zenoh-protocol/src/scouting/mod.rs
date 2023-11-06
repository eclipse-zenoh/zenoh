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
pub mod hello;
pub mod scout;

pub use hello::Hello;
pub use scout::Scout;

pub mod id {
    // Scouting Messages
    pub const SCOUT: u8 = 0x01;
    pub const HELLO: u8 = 0x02;
}

// Zenoh messages at scouting level
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScoutingBody {
    Scout(Scout),
    Hello(Hello),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScoutingMessage {
    pub body: ScoutingBody,
    #[cfg(feature = "stats")]
    pub size: Option<core::num::NonZeroUsize>,
}

impl ScoutingMessage {
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use rand::Rng;

        let mut rng = rand::thread_rng();

        match rng.gen_range(0..2) {
            0 => ScoutingBody::Scout(Scout::rand()),
            1 => ScoutingBody::Hello(Hello::rand()),
            _ => unreachable!(),
        }
        .into()
    }
}

impl From<ScoutingBody> for ScoutingMessage {
    fn from(body: ScoutingBody) -> Self {
        Self {
            body,
            #[cfg(feature = "stats")]
            size: None,
        }
    }
}

impl From<Scout> for ScoutingMessage {
    fn from(scout: Scout) -> Self {
        ScoutingBody::Scout(scout).into()
    }
}

impl From<Hello> for ScoutingMessage {
    fn from(hello: Hello) -> Self {
        ScoutingBody::Hello(hello).into()
    }
}
