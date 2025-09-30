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

pub use hello::HelloProto;
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
    Hello(HelloProto),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScoutingMessage {
    pub body: ScoutingBody,
}

impl ScoutingMessage {
    #[cfg(feature = "test")]
    #[doc(hidden)]
    pub fn rand() -> Self {
        use rand::Rng;

        let mut rng = rand::thread_rng();

        match rng.gen_range(0..2) {
            0 => ScoutingBody::Scout(Scout::rand()),
            1 => ScoutingBody::Hello(HelloProto::rand()),
            _ => unreachable!(),
        }
        .into()
    }
}

impl From<ScoutingBody> for ScoutingMessage {
    fn from(body: ScoutingBody) -> Self {
        Self { body }
    }
}

impl From<Scout> for ScoutingMessage {
    fn from(scout: Scout) -> Self {
        ScoutingBody::Scout(scout).into()
    }
}

impl From<HelloProto> for ScoutingMessage {
    fn from(hello: HelloProto) -> Self {
        ScoutingBody::Hello(hello).into()
    }
}
