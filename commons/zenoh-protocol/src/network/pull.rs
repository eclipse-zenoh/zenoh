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

pub type PullId = u32;

pub mod flag {
    // pub const X: u8 = 1 << 5; // 0x20 Reserved
    // pub const X: u8 = 1 << 6; // 0x40 Reserved
    pub const Z: u8 = 1 << 7; // 0x80 Extensions    if Z==1 then an extension will follow
}

/// ```text
/// Flags:
/// - X: Reserved
/// - X: Reserved
/// - Z: Extension      If Z==1 then at least one extension is present
///
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |Z|X|X|  PULL   |
/// +-+-+-+---------+
/// ~    id:z32     ~  (*)
/// +---------------+
/// ~  [pull_exts]  ~  (*)
/// +---------------+
///
/// (*) ID refers to the ID used in a previous target declaration
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pull {
    pub id: PullId,
}

impl Pull {
    pub const T_SUBSCRIBER: u8 = 0x01;

    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use rand::Rng;
        let mut rng = rand::thread_rng();

        let id: PullId = rng.gen();

        Self { id }
    }
}
