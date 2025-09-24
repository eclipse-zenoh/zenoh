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
use alloc::vec::Vec;

use crate::{
    common::ZExtUnknown,
    zenoh::{query::ConsolidationMode, PushBody},
};

/// # Reply message
///
/// ```text
/// Flags:
/// - C: Consolidation  if C==1 then consolidation is present
/// - X: Reserved
/// - Z: Extension      If Z==1 then at least one extension is present
///
///   7 6 5 4 3 2 1 0
///  +-+-+-+-+-+-+-+-+
///  |Z|X|C|  REPLY  |
///  +-+-+-+---------+
///  % consolidation %  if C==1
///  +---------------+
///  ~  [repl_exts]  ~  if Z==1
///  +---------------+
///  ~   ReplyBody   ~  -- Payload
///  +---------------+
/// ```
pub mod flag {
    pub const C: u8 = 1 << 5; // 0x20 Consolidation if C==1 then consolidation is present
                              // pub const X: u8 = 1 << 6; // 0x40 Reserved
    pub const Z: u8 = 1 << 7; // 0x80 Extensions    if Z==1 then an extension will follow
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    pub consolidation: ConsolidationMode,
    pub ext_unknown: Vec<ZExtUnknown>,
    pub payload: ReplyBody,
}

pub type ReplyBody = PushBody;

impl Reply {
    #[cfg(feature = "test")]
    #[doc(hidden)]
    pub fn rand() -> Self {
        use rand::Rng;
        let mut rng = rand::thread_rng();

        let payload = ReplyBody::rand();
        let consolidation = ConsolidationMode::rand();
        let mut ext_unknown = Vec::new();
        for _ in 0..rng.gen_range(0..4) {
            ext_unknown.push(ZExtUnknown::rand2(1, false));
        }

        Self {
            consolidation,
            ext_unknown,
            payload,
        }
    }
}
