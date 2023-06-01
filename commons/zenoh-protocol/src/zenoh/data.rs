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
use crate::core::{CongestionControl, Encoding, SampleKind, Timestamp, WireExpr, ZInt, ZenohId};
use zenoh_buffers::ZBuf;

/// # ReplyContext decorator
///
/// ```text
/// The **ReplyContext** is a message decorator for either:
///   - the **Data** messages that result from a query
///   - or a **Unit** message in case the message is a
///     SOURCE_FINAL or REPLY_FINAL.
///  The **replier-id** (queryable id) is represented as a byte-array.
///
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |X|X|F|  R_CTX  |
/// +-+-+-+---------+
/// ~      qid      ~
/// +---------------+
/// ~   replier_id  ~ if F==0
/// +---------------+
///
/// - if F==1 then the message is a REPLY_FINAL
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplierInfo {
    pub id: ZenohId,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplyContext {
    pub qid: ZInt,
    pub replier: Option<ReplierInfo>,
}

impl ReplyContext {
    // Note: id replier_id=None flag F is set, meaning it's a REPLY_FINAL
    pub fn new(qid: ZInt, replier: Option<ReplierInfo>) -> Self {
        Self { qid, replier }
    }

    pub fn is_final(&self) -> bool {
        self.replier.is_none()
    }
}

impl ReplyContext {
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use rand::Rng;

        let mut rng = rand::thread_rng();

        let qid: ZInt = rng.gen();
        let replier = if rng.gen_bool(0.5) {
            Some(ReplierInfo {
                id: ZenohId::default(),
            })
        } else {
            None
        };

        Self { qid, replier }
    }
}

/// # DataInfo
///
/// DataInfo data structure is optionally included in Data messages
///
/// ```text
///
/// Options bits
/// -  0: Payload is sliced
/// -  1: Payload kind
/// -  2: Payload encoding
/// -  3: Payload timestamp
/// -  4: Reserved
/// -  5: Reserved
/// -  6: Reserved
/// -  7: Payload source_id
/// -  8: Payload source_sn
/// -  9-63: Reserved
///
///  7 6 5 4 3 2 1 0
/// +-+-+-+---------+
/// ~    options    ~
/// +---------------+
/// ~      kind     ~ if options & (1 << 1)
/// +---------------+
/// ~   encoding    ~ if options & (1 << 2)
/// +---------------+
/// ~   timestamp   ~ if options & (1 << 3)
/// +---------------+
/// ~   source_id   ~ if options & (1 << 7)
/// +---------------+
/// ~   source_sn   ~ if options & (1 << 8)
/// +---------------+
///
/// - if options & (1 << 0) then the payload is sliced
///
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DataInfo {
    #[cfg(feature = "shared-memory")]
    pub sliced: bool,
    pub kind: SampleKind,
    pub encoding: Option<Encoding>,
    pub timestamp: Option<Timestamp>,
    pub source_id: Option<ZenohId>,
    pub source_sn: Option<ZInt>,
}

impl DataInfo {
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use rand::Rng;

        let mut rng = rand::thread_rng();

        #[cfg(feature = "shared-memory")]
        let sliced = rng.gen_bool(0.5);
        let kind = SampleKind::try_from(rng.gen_range(0..=1)).unwrap();
        let encoding = rng.gen_bool(0.5).then(Encoding::rand);
        let timestamp = rng.gen_bool(0.5).then(|| {
            let time = uhlc::NTP64(rng.gen());
            let id = uhlc::ID::try_from(ZenohId::rand().to_le_bytes()).unwrap();
            Timestamp::new(time, id)
        });
        let source_id = rng.gen_bool(0.5).then(ZenohId::rand);
        let source_sn = rng.gen_bool(0.5).then(|| rng.gen());

        Self {
            #[cfg(feature = "shared-memory")]
            sliced,
            kind,
            encoding,
            timestamp,
            source_id,
            source_sn,
        }
    }
}

/// # Data message
///
/// ```text
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |K|I|D|  DATA   |
/// +-+-+-+---------+
/// ~    KeyExpr     ~ if K==1 -- Only numerical id
/// +---------------+
/// ~    DataInfo   ~ if I==1
/// +---------------+
/// ~    Payload    ~
/// +---------------+
///
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Data {
    pub key: WireExpr<'static>,
    pub data_info: Option<DataInfo>,
    pub payload: ZBuf,
    pub congestion_control: CongestionControl,
    pub reply_context: Option<ReplyContext>,
}

impl Data {
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use rand::Rng;

        const MIN: usize = 2;
        const MAX: usize = 16;

        let mut rng = rand::thread_rng();

        let key = WireExpr::rand();
        let data_info = if rng.gen_bool(0.5) {
            Some(DataInfo::rand())
        } else {
            None
        };

        let payload = ZBuf::rand(rng.gen_range(MIN..MAX));

        let congestion_control = if rng.gen_bool(0.5) {
            CongestionControl::Block
        } else {
            CongestionControl::Drop
        };
        let reply_context = if rng.gen_bool(0.5) {
            Some(ReplyContext::rand())
        } else {
            None
        };

        Self {
            key,
            data_info,
            payload,
            congestion_control,
            reply_context,
        }
    }
}
