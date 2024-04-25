// Copyright (c) 2024 ZettaScale Technology
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

use crate::common::ZExtUnknown;
use alloc::vec::Vec;
use uhlc::Timestamp;

/// # Err message
///
/// ```text
/// Flags:
/// - T: Timestamp      If T==1 then the timestamp if present
/// - I: Infrastructure If I==1 then the error is related to the infrastructure else to the user
/// - Z: Extension      If Z==1 then at least one extension is present
///
///   7 6 5 4 3 2 1 0
///  +-+-+-+-+-+-+-+-+
///  |Z|I|T|   ERR   |
///  +-+-+-+---------+
///  %   code:z16    %
///  +---------------+
///  ~ ts: <u8;z16>  ~  if T==1
///  +---------------+
///  ~  [err_exts]   ~  if Z==1
///  +---------------+
/// ```
pub mod flag {
    pub const T: u8 = 1 << 5; // 0x20 Timestamp         if T==0 then the timestamp if present
    pub const I: u8 = 1 << 6; // 0x40 Infrastructure    if I==1 then the error is related to the infrastructure else to the user
    pub const Z: u8 = 1 << 7; // 0x80 Extensions        if Z==1 then an extension will follow
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Err {
    pub code: u16,
    pub is_infrastructure: bool,
    pub timestamp: Option<Timestamp>,
    pub ext_sinfo: Option<ext::SourceInfoType>,
    pub ext_body: Option<ext::ErrBodyType>,
    pub ext_unknown: Vec<ZExtUnknown>,
}

pub mod ext {
    use crate::{common::ZExtZBuf, zextzbuf};

    /// # SourceInfo extension
    /// Used to carry additional information about the source of data
    pub type SourceInfo = zextzbuf!(0x1, false);
    pub type SourceInfoType = crate::zenoh::ext::SourceInfoType<{ SourceInfo::ID }>;

    /// # ErrBody extension
    /// Used to carry a body attached to the query
    /// Shared Memory extension is automatically defined by ValueType extension if
    /// #[cfg(feature = "shared-memory")] is defined.
    pub type ErrBodyType = crate::zenoh::ext::ValueType<{ ZExtZBuf::<0x02>::id(false) }, 0x03>;
}

impl Err {
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use crate::{common::iext, core::ZenohId};
        use rand::Rng;
        let mut rng = rand::thread_rng();

        let code: u16 = rng.gen();
        let is_infrastructure = rng.gen_bool(0.5);
        let timestamp = rng.gen_bool(0.5).then_some({
            let time = uhlc::NTP64(rng.gen());
            let id = uhlc::ID::try_from(ZenohId::rand().to_le_bytes()).unwrap();
            Timestamp::new(time, id)
        });
        let ext_sinfo = rng.gen_bool(0.5).then_some(ext::SourceInfoType::rand());
        let ext_body = rng.gen_bool(0.5).then_some(ext::ErrBodyType::rand());
        let mut ext_unknown = Vec::new();
        for _ in 0..rng.gen_range(0..4) {
            ext_unknown.push(ZExtUnknown::rand2(
                iext::mid(ext::ErrBodyType::SID) + 1,
                false,
            ));
        }

        Self {
            code,
            is_infrastructure,
            timestamp,
            ext_sinfo,
            ext_body,
            ext_unknown,
        }
    }
}
