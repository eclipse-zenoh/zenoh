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
use crate::core::Encoding;
use uhlc::Timestamp;
use zenoh_buffers::ZBuf;

/// # Reply message
///
/// ```text
/// Flags:
/// - T: Timestamp      If T==1 then the timestamp if present
/// - E: Encoding       If E==1 then the encoding is present
/// - Z: Extension      If Z==1 then at least one extension is present
///
///   7 6 5 4 3 2 1 0
///  +-+-+-+-+-+-+-+-+
///  |Z|E|T|  REPLY  |
///  +-+-+-+---------+
///  ~ ts: <u8;z16>  ~  if T==1
///  +---------------+
///  ~   encoding    ~  if E==1
///  +---------------+
///  ~  [repl_exts]  ~  if Z==1
///  +---------------+
///  ~ pl: <u8;z32>  ~ -- Payload
///  +---------------+
/// ```
pub mod flag {
    pub const T: u8 = 1 << 5; // 0x20 Timestamp     if T==0 then the timestamp if present
    pub const E: u8 = 1 << 6; // 0x40 Encoding      if E==1 then the encoding is present
    pub const Z: u8 = 1 << 7; // 0x80 Extensions    if Z==1 then an extension will follow
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    pub timestamp: Option<Timestamp>,
    pub encoding: Encoding,
    pub ext_sinfo: Option<ext::SourceInfoType>,
    pub payload: ZBuf,
}

pub mod ext {
    /// # SourceInfo extension
    /// Used to carry additional information about the source of data
    pub type SourceInfo = crate::zenoh_new::put::ext::SourceInfo;
    pub type SourceInfoType = crate::zenoh_new::put::ext::SourceInfoType;
}

impl Reply {
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use crate::core::ZenohId;
        use core::convert::TryFrom;
        use rand::Rng;
        let mut rng = rand::thread_rng();

        let timestamp = rng.gen_bool(0.5).then_some({
            let time = uhlc::NTP64(rng.gen());
            let id = uhlc::ID::try_from(ZenohId::rand().as_slice()).unwrap();
            Timestamp::new(time, id)
        });
        let encoding = Encoding::rand();
        let ext_sinfo = rng.gen_bool(0.5).then_some(ext::SourceInfoType::rand());
        let payload = ZBuf::rand(rng.gen_range(1..=64));

        Self {
            timestamp,
            encoding,
            ext_sinfo,
            payload,
        }
    }
}
