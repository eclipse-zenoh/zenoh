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
            1 => ScoutingBody::Hello(HelloProto::rand()),
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

impl From<HelloProto> for ScoutingMessage {
    fn from(hello: HelloProto) -> Self {
        ScoutingBody::Hello(hello).into()
    }
}

// Extensions
pub mod ext {
    use core::ops::Deref;
    use zenoh_buffers::ZSlice;

    /// ```text
    ///  7 6 5 4 3 2 1 0
    /// +-+-+-+-+-+-+-+-+
    /// |Z|1_0|    ID   |
    /// +-+-+-+---------+
    /// %     len       %
    /// +---------------+
    /// ~    [[u8]]     ~
    /// +---------------+
    /// ```
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct GroupsType<const ID: u8> {
        inner: Vec<ZSlice>,
    }

    impl<const ID: u8> GroupsType<{ ID }> {
        pub fn new(gs: Vec<ZSlice>) -> Self {
            Self { inner: gs }
        }

        #[cfg(feature = "test")]
        pub fn rand() -> Self {
            use rand::Rng;
            let mut rng = rand::thread_rng();

            let mut inner = vec![];
            for _ in 0..rng.gen_range(1..4) {
                let g = ZSlice::rand(rng.gen_range(1..64));
                inner.push(g);
            }

            Self { inner }
        }
    }

    impl<const ID: u8> Default for GroupsType<{ ID }> {
        fn default() -> Self {
            Self { inner: Vec::new() }
        }
    }

    impl<const ID: u8> Deref for GroupsType<{ ID }> {
        type Target = [ZSlice];

        fn deref(&self) -> &Self::Target {
            &self.inner
        }
    }
}
