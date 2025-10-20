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
pub mod del;
pub mod err;
pub mod put;
pub mod query;
pub mod reply;

pub use del::Del;
pub use err::Err;
pub use put::Put;
pub use query::{ConsolidationMode, Query};
pub use reply::Reply;

use crate::core::Encoding;

pub mod id {
    pub const OAM: u8 = 0x00;
    pub const PUT: u8 = 0x01;
    pub const DEL: u8 = 0x02;
    pub const QUERY: u8 = 0x03;
    pub const REPLY: u8 = 0x04;
    pub const ERR: u8 = 0x05;
}

// DataInfo
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataInfo {
    pub encoding: Encoding,
}

// Push
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PushBody {
    Put(Put),
    Del(Del),
}

impl PushBody {
    #[cfg(feature = "test")]
    #[doc(hidden)]
    pub fn rand() -> Self {
        use rand::Rng;

        let mut rng = rand::thread_rng();

        match rng.gen_range(0..2) {
            0 => PushBody::Put(Put::rand()),
            1 => PushBody::Del(Del::rand()),
            _ => unreachable!(),
        }
    }
}

impl From<Put> for PushBody {
    fn from(p: Put) -> PushBody {
        PushBody::Put(p)
    }
}

impl From<Del> for PushBody {
    fn from(d: Del) -> PushBody {
        PushBody::Del(d)
    }
}

// Request
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestBody {
    Query(Query),
}

impl RequestBody {
    #[cfg(feature = "test")]
    #[doc(hidden)]
    pub fn rand() -> Self {
        use rand::Rng;

        let mut rng = rand::thread_rng();

        match rng.gen_range(0..1) {
            0 => RequestBody::Query(Query::rand()),
            _ => unreachable!(),
        }
    }
}

impl From<Query> for RequestBody {
    fn from(q: Query) -> RequestBody {
        RequestBody::Query(q)
    }
}

// Response
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResponseBody {
    Reply(Reply),
    Err(Err),
}

impl ResponseBody {
    #[cfg(feature = "test")]
    #[doc(hidden)]
    pub fn rand() -> Self {
        use rand::Rng;
        let mut rng = rand::thread_rng();

        match rng.gen_range(0..2) {
            0 => ResponseBody::Reply(Reply::rand()),
            1 => ResponseBody::Err(Err::rand()),
            _ => unreachable!(),
        }
    }
}

impl From<Reply> for ResponseBody {
    fn from(r: Reply) -> ResponseBody {
        ResponseBody::Reply(r)
    }
}

impl From<Err> for ResponseBody {
    fn from(r: Err) -> ResponseBody {
        ResponseBody::Err(r)
    }
}

pub mod ext {
    use zenoh_buffers::ZBuf;

    use crate::core::{Encoding, EntityGlobalIdProto};

    /// ```text
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
    /// ```
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct SourceInfoType<const ID: u8> {
        pub id: EntityGlobalIdProto,
        pub sn: u32,
    }

    impl<const ID: u8> SourceInfoType<{ ID }> {
        #[cfg(feature = "test")]
        #[doc(hidden)]
        pub fn rand() -> Self {
            use rand::Rng;
            let mut rng = rand::thread_rng();

            let id = EntityGlobalIdProto::rand();
            let sn: u32 = rng.gen();
            Self { id, sn }
        }
    }

    ///  7 6 5 4 3 2 1 0
    /// +-+-+-+-+-+-+-+-+
    /// +-+-+-+-+-+-+-+-+
    #[cfg(feature = "shared-memory")]
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ShmType<const ID: u8>;

    #[cfg(feature = "shared-memory")]
    impl<const ID: u8> ShmType<{ ID }> {
        pub const fn new() -> Self {
            Self
        }

        #[cfg(feature = "test")]
        pub const fn rand() -> Self {
            Self
        }
    }
    #[cfg(feature = "shared-memory")]
    impl<const ID: u8> Default for ShmType<{ ID }> {
        fn default() -> Self {
            Self::new()
        }
    }

    /// ```text
    ///   7 6 5 4 3 2 1 0
    ///  +-+-+-+-+-+-+-+-+
    ///  ~   encoding    ~
    ///  +---------------+
    ///  ~ pl: [u8;z32]  ~  -- Payload
    ///  +---------------+
    /// ```
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ValueType<const VID: u8, const SID: u8> {
        #[cfg(feature = "shared-memory")]
        pub ext_shm: Option<ShmType<{ SID }>>,
        pub encoding: Encoding,
        pub payload: ZBuf,
    }

    impl<const VID: u8, const SID: u8> ValueType<{ VID }, { SID }> {
        pub const VID: u8 = VID;
        pub const SID: u8 = SID;

        #[cfg(feature = "test")]
        #[doc(hidden)]
        pub fn rand() -> Self {
            use rand::Rng;
            let mut rng = rand::thread_rng();

            #[cfg(feature = "shared-memory")]
            let ext_shm = rng.gen_bool(0.5).then_some(ShmType::rand());
            let encoding = Encoding::rand();
            let payload = ZBuf::rand(rng.gen_range(1..=64));

            Self {
                #[cfg(feature = "shared-memory")]
                ext_shm,
                encoding,
                payload,
            }
        }
    }

    /// ```text
    /// 7 6 5 4 3 2 1 0
    /// +-+-+-+-+-+-+-+-+
    /// %   num elems   %
    /// +-------+-+-+---+
    /// ~ key: <u8;z16> ~
    /// +---------------+
    /// ~ val: <u8;z32> ~
    /// +---------------+
    ///       ...         -- N times (key, value) tuples
    /// ```
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct AttachmentType<const ID: u8> {
        pub buffer: ZBuf,
    }

    impl<const ID: u8> AttachmentType<{ ID }> {
        #[cfg(feature = "test")]
        #[doc(hidden)]
        pub fn rand() -> Self {
            use rand::Rng;
            let mut rng = rand::thread_rng();

            Self {
                buffer: ZBuf::rand(rng.gen_range(3..=1_024)),
            }
        }
    }
}
