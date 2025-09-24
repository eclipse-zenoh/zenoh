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

use zenoh_buffers::ZBuf;

use crate::{common::ZExtUnknown, core::Encoding};

/// # Err message
///
/// ```text
/// Flags:
/// - X: Reserved
/// - E: Encoding       If E==1 then the encoding is present
/// - Z: Extension      If Z==1 then at least one extension is present
///
///   7 6 5 4 3 2 1 0
///  +-+-+-+-+-+-+-+-+
///  |Z|E|X|   ERR   |
///  +-+-+-+---------+
///  ~   encoding    ~  if E==1
///  +---------------+
///  ~  [err_exts]   ~  if Z==1
///  +---------------+
///  ~ pl: <u8;z32>  ~  -- Payload
///  +---------------+
/// ```
pub mod flag {
    // pub const X: u8 = 1 << 5; // 0x20 Reserved
    pub const E: u8 = 1 << 6; // 0x40 Encoding      if E==1 then the encoding is present
    pub const Z: u8 = 1 << 7; // 0x80 Extensions        if Z==1 then an extension will follow
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Err {
    pub encoding: Encoding,
    pub ext_sinfo: Option<ext::SourceInfoType>,
    #[cfg(feature = "shared-memory")]
    pub ext_shm: Option<ext::ShmType>,
    pub ext_unknown: Vec<ZExtUnknown>,
    pub payload: ZBuf,
}

pub mod ext {
    #[cfg(feature = "shared-memory")]
    use crate::{common::ZExtUnit, zextunit};
    use crate::{common::ZExtZBuf, zextzbuf};

    /// # SourceInfo extension
    /// Used to carry additional information about the source of data
    pub type SourceInfo = zextzbuf!(0x1, false);
    pub type SourceInfoType = crate::zenoh::ext::SourceInfoType<{ SourceInfo::ID }>;

    /// # Shared Memory extension
    /// Used to carry additional information about the shared-memory layout of data
    #[cfg(feature = "shared-memory")]
    pub type Shm = zextunit!(0x2, true);
    #[cfg(feature = "shared-memory")]
    pub type ShmType = crate::zenoh::ext::ShmType<{ Shm::ID }>;
}

impl Err {
    #[cfg(feature = "test")]
    #[doc(hidden)]
    pub fn rand() -> Self {
        use rand::Rng;

        use crate::common::iext;
        let mut rng = rand::thread_rng();

        let encoding = Encoding::rand();
        let ext_sinfo = rng.gen_bool(0.5).then_some(ext::SourceInfoType::rand());
        #[cfg(feature = "shared-memory")]
        let ext_shm = rng.gen_bool(0.5).then_some(ext::ShmType::rand());
        let mut ext_unknown = Vec::new();
        for _ in 0..rng.gen_range(0..4) {
            ext_unknown.push(ZExtUnknown::rand2(
                iext::mid(ext::SourceInfo::ID) + 1,
                false,
            ));
        }
        let payload = ZBuf::rand(rng.gen_range(0..=64));

        Self {
            encoding,
            ext_sinfo,
            #[cfg(feature = "shared-memory")]
            ext_shm,
            ext_unknown,
            payload,
        }
    }
}
