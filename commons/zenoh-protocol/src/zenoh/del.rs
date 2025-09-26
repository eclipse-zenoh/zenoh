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

use uhlc::Timestamp;

use crate::common::ZExtUnknown;

/// # Put message
///
/// ```text
/// Flags:
/// - T: Timestamp      If T==1 then the timestamp if present
/// - X: Reserved
/// - Z: Extension      If Z==1 then at least one extension is present
///
///   7 6 5 4 3 2 1 0
///  +-+-+-+-+-+-+-+-+
///  |Z|X|T|   DEL   |
///  +-+-+-+---------+
///  ~ ts: <u8;z16>  ~  if T==1
///  +---------------+
///  ~  [del_exts]   ~  if Z==1
///  +---------------+
/// ```
pub mod flag {
    pub const T: u8 = 1 << 5; // 0x20 Timestamp     if T==0 then the timestamp if present
                              // pub const X: u8 = 1 << 6; // 0x40 Reserved
    pub const Z: u8 = 1 << 7; // 0x80 Extensions    if Z==1 then an extension will follow
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Del {
    pub timestamp: Option<Timestamp>,
    pub ext_sinfo: Option<ext::SourceInfoType>,
    pub ext_attachment: Option<ext::AttachmentType>,
    pub ext_unknown: Vec<ZExtUnknown>,
}

pub mod ext {
    use crate::{common::ZExtZBuf, zextzbuf};

    /// # SourceInfo extension
    /// Used to carry additional information about the source of data
    pub type SourceInfo = zextzbuf!(0x1, false);
    pub type SourceInfoType = crate::zenoh::ext::SourceInfoType<{ SourceInfo::ID }>;

    /// # User attachment
    pub type Attachment = zextzbuf!(0x2, false);
    pub type AttachmentType = crate::zenoh::ext::AttachmentType<{ Attachment::ID }>;
}

impl Del {
    #[cfg(feature = "test")]
    #[doc(hidden)]
    pub fn rand() -> Self {
        use rand::Rng;

        use crate::{common::iext, core::ZenohIdProto};
        let mut rng = rand::thread_rng();

        let timestamp = rng.gen_bool(0.5).then_some({
            let time = uhlc::NTP64(rng.gen());
            let id = uhlc::ID::try_from(ZenohIdProto::rand().to_le_bytes()).unwrap();
            Timestamp::new(time, id)
        });
        let ext_sinfo = rng.gen_bool(0.5).then_some(ext::SourceInfoType::rand());
        let ext_attachment = rng.gen_bool(0.5).then_some(ext::AttachmentType::rand());
        let mut ext_unknown = Vec::new();
        for _ in 0..rng.gen_range(0..4) {
            ext_unknown.push(ZExtUnknown::rand2(
                iext::mid(ext::Attachment::ID) + 1,
                false,
            ));
        }

        Self {
            timestamp,
            ext_sinfo,
            ext_attachment,
            ext_unknown,
        }
    }
}
