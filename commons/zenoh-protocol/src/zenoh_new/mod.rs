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
pub mod put;

pub use put::*;

pub mod id {
    pub const OAM: u8 = 0x00;
    pub const PUT: u8 = 0x01;
    pub const DEL: u8 = 0x02;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PushBody {
    Put(Put),
}

impl PushBody {
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use rand::Rng;

        let mut rng = rand::thread_rng();

        match rng.gen_range(0..1) {
            0 => PushBody::Put(Put::rand()),
            _ => unreachable!(),
        }
    }
}

impl From<Put> for PushBody {
    fn from(p: Put) -> PushBody {
        PushBody::Put(p)
    }
}
