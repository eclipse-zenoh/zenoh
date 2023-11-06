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
use crate::{common::ZExtUnknown, core::ConsolidationMode};
use alloc::{string::String, vec::Vec};

/// The kind of consolidation.
#[repr(u8)]
#[derive(Debug, Default, Clone, PartialEq, Eq, Copy)]
pub enum Consolidation {
    /// Apply automatic consolidation based on queryable's preferences
    #[default]
    Auto,
    /// No consolidation applied: multiple samples may be received for the same key-timestamp.
    None,
    /// Monotonic consolidation immediately forwards samples, except if one with an equal or more recent timestamp
    /// has already been sent with the same key.
    ///
    /// This optimizes latency while potentially reducing bandwidth.
    ///
    /// Note that this doesn't cause re-ordering, but drops the samples for which a more recent timestamp has already
    /// been observed with the same key.
    Monotonic,
    /// Holds back samples to only send the set of samples that had the highest timestamp for their key.    
    Latest,
    /// Remove the duplicates of any samples based on the their timestamp.
    Unique,
}

impl Consolidation {
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use rand::prelude::SliceRandom;
        let mut rng = rand::thread_rng();

        *[
            Self::None,
            Self::Monotonic,
            Self::Latest,
            Self::Unique,
            Self::Auto,
        ]
        .choose(&mut rng)
        .unwrap()
    }
}

impl From<ConsolidationMode> for Consolidation {
    fn from(val: ConsolidationMode) -> Self {
        match val {
            ConsolidationMode::None => Consolidation::None,
            ConsolidationMode::Monotonic => Consolidation::Monotonic,
            ConsolidationMode::Latest => Consolidation::Latest,
        }
    }
}

/// # Query message
///
/// ```text
/// Flags:
/// - P: Parameters     If P==1 then the parameters are present
/// - X: Reserved
/// - Z: Extension      If Z==1 then at least one extension is present
///
///   7 6 5 4 3 2 1 0
///  +-+-+-+-+-+-+-+-+
///  |Z|X|P|  QUERY  |
///  +-+-+-+---------+
///  ~ ps: <u8;z16>  ~  if P==1
///  +---------------+
///  ~  [qry_exts]   ~  if Z==1
///  +---------------+
/// ```
pub mod flag {
    pub const P: u8 = 1 << 5; // 0x20 Parameters    if P==1 then the parameters are present
                              // pub const X: u8 = 1 << 6; // 0x40 Reserved
    pub const Z: u8 = 1 << 7; // 0x80 Extensions    if Z==1 then an extension will follow
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Query {
    pub parameters: String,
    pub ext_sinfo: Option<ext::SourceInfoType>,
    pub ext_consolidation: Consolidation,
    pub ext_body: Option<ext::QueryBodyType>,
    pub ext_unknown: Vec<ZExtUnknown>,
}

pub mod ext {
    use crate::{
        common::{ZExtZ64, ZExtZBuf},
        zextz64, zextzbuf,
    };

    /// # SourceInfo extension
    /// Used to carry additional information about the source of data
    pub type SourceInfo = zextzbuf!(0x1, false);
    pub type SourceInfoType = crate::zenoh::ext::SourceInfoType<{ SourceInfo::ID }>;

    /// # Consolidation extension
    pub type Consolidation = zextz64!(0x2, true);
    pub type ConsolidationType = crate::zenoh::query::Consolidation;

    /// # QueryBody extension
    /// Used to carry a body attached to the query
    /// Shared Memory extension is automatically defined by ValueType extension if
    /// #[cfg(feature = "shared-memory")] is defined.
    pub type QueryBodyType = crate::zenoh::ext::ValueType<{ ZExtZBuf::<0x03>::id(false) }, 0x04>;
}

impl Query {
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use crate::common::iext;
        use rand::{
            distributions::{Alphanumeric, DistString},
            Rng,
        };
        let mut rng = rand::thread_rng();

        const MIN: usize = 2;
        const MAX: usize = 16;

        let parameters: String = if rng.gen_bool(0.5) {
            let len = rng.gen_range(MIN..MAX);
            Alphanumeric.sample_string(&mut rng, len)
        } else {
            String::new()
        };
        let ext_sinfo = rng.gen_bool(0.5).then_some(ext::SourceInfoType::rand());
        let ext_consolidation = Consolidation::rand();
        let ext_body = rng.gen_bool(0.5).then_some(ext::QueryBodyType::rand());
        let mut ext_unknown = Vec::new();
        for _ in 0..rng.gen_range(0..4) {
            ext_unknown.push(ZExtUnknown::rand2(
                iext::mid(ext::QueryBodyType::SID) + 1,
                false,
            ));
        }

        Self {
            parameters,
            ext_sinfo,
            ext_consolidation,
            ext_body,
            ext_unknown,
        }
    }
}
