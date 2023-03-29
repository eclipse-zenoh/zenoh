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

/// # Put message
///
/// ```text
/// Flags:
/// - T: Timestamp      If T==1 then the timestamp if present
/// - E: Encoding       If E==1 then the encoding is present
/// - Z: Extension      If Z==1 then at least one extension is present
///
///   7 6 5 4 3 2 1 0
///  +-+-+-+-+-+-+-+-+
///  |Z|E|T|   PUT   |
///  +-+-+-+---------+
///  ~ ts: <u8;z16>  ~  if T==1
///  +---------------+
///  ~   encoding    ~  if E==1
///  +---------------+
///  ~  [put_exts]   ~  if Z==1
///  +---------------+
///  ~ pl: <u8;z64>  ~ -- Payload
///  +---------------+
/// ```
pub mod flag {
    pub const T: u8 = 1 << 5; // 0x20 Timestamp     if T==0 then the timestamp if present
    pub const E: u8 = 1 << 6; // 0x40 Encoding      if E==1 then the encoding is present
    pub const Z: u8 = 1 << 7; // 0x80 Extensions    if Z==1 then an extension will follow
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Put {
    pub timestamp: Option<Timestamp>,
    pub encoding: Encoding,
    pub ext_sinfo: Option<ext::SourceInfoType>,
    pub payload: ZBuf,
}

pub mod ext {
    use crate::core::ZenohId;
    use crate::{common::ZExtZBuf, zextzbuf};

    /// # SourceInfo extension
    /// Used to carry additional information about
    pub type SourceInfo = zextzbuf!(0x1, false);

    ///  7 6 5 4 3 2 1 0
    /// +-+-+-+-+-+-+-+-+
    /// |zid_len|X|X|X|X|
    /// +-------+-+-+---+
    /// ~      zid      ~
    /// +---------------+
    /// %      eid      %  -- Counter decided by the Zenoh Node
    /// +---------------+
    /// %      sn       %
    /// +---------------+
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct SourceInfoType {
        pub zid: ZenohId,
        pub eid: u32,
        pub sn: u32,
    }

    impl SourceInfoType {
        #[cfg(feature = "test")]
        pub fn rand() -> Self {
            use rand::Rng;
            let mut rng = rand::thread_rng();

            let zid = ZenohId::rand();
            let eid: u32 = rng.gen();
            let sn: u32 = rng.gen();
            Self { zid, eid, sn }
        }
    }
}

impl Put {
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
