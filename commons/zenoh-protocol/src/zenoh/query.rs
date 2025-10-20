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
use alloc::{string::String, vec::Vec};

use serde::Deserialize;

use crate::common::ZExtUnknown;

/// The kind of consolidation to apply to a query.
#[repr(u8)]
#[derive(Debug, Default, Clone, PartialEq, Eq, Copy, Deserialize)]
pub enum ConsolidationMode {
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
    // Remove the duplicates of any samples based on the their timestamp.
    // Unique,
}

impl ConsolidationMode {
    pub const DEFAULT: Self = Self::Auto;

    #[cfg(feature = "test")]
    #[doc(hidden)]
    pub fn rand() -> Self {
        use rand::prelude::SliceRandom;
        let mut rng = rand::thread_rng();

        *[Self::None, Self::Monotonic, Self::Latest, Self::Auto]
            .choose(&mut rng)
            .unwrap()
    }
}

/// # Query message
///
/// ```text
/// Flags:
/// - C: Consolidation  if C==1 then consolidation is present
/// - P: Parameters     If P==1 then the parameters are present
/// - Z: Extension      If Z==1 then at least one extension is present
///
///   7 6 5 4 3 2 1 0
///  +-+-+-+-+-+-+-+-+
///  |Z|P|C|  QUERY  |
///  +-+-+-+---------+
///  % consolidation %  if C==1
///  +---------------+
///  ~ ps: <u8;z16>  ~  if P==1
///  +---------------+
///  ~  [qry_exts]   ~  if Z==1
///  +---------------+
/// ```
pub mod flag {
    pub const C: u8 = 1 << 5; // 0x20 Consolidation if C==1 then consolidation is present
    pub const P: u8 = 1 << 6; // 0x40 Parameters    if P==1 then the parameters are present
    pub const Z: u8 = 1 << 7; // 0x80 Extensions    if Z==1 then an extension will follow
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Query {
    pub consolidation: ConsolidationMode,
    pub parameters: String,
    pub ext_sinfo: Option<ext::SourceInfoType>,
    pub ext_body: Option<ext::QueryBodyType>,
    pub ext_attachment: Option<ext::AttachmentType>,
    pub ext_unknown: Vec<ZExtUnknown>,
}

pub mod ext {
    use crate::{common::ZExtZBuf, zextzbuf};

    /// # SourceInfo extension
    /// Used to carry additional information about the source of data
    pub type SourceInfo = zextzbuf!(0x1, false);
    pub type SourceInfoType = crate::zenoh::ext::SourceInfoType<{ SourceInfo::ID }>;

    /// # QueryBody extension
    /// Used to carry a body attached to the query
    /// Shared Memory extension is automatically defined by ValueType extension if
    /// #[cfg(feature = "shared-memory")] is defined.
    pub type QueryBodyType = crate::zenoh::ext::ValueType<{ ZExtZBuf::<0x03>::id(false) }, 0x04>;

    /// # User attachment
    pub type Attachment = zextzbuf!(0x5, false);
    pub type AttachmentType = crate::zenoh::ext::AttachmentType<{ Attachment::ID }>;
}

impl Query {
    #[cfg(feature = "test")]
    #[doc(hidden)]
    pub fn rand() -> Self {
        use rand::{
            distributions::{Alphanumeric, DistString},
            Rng,
        };

        use crate::common::iext;
        let mut rng = rand::thread_rng();

        const MIN: usize = 2;
        const MAX: usize = 16;

        let consolidation = ConsolidationMode::rand();
        let parameters: String = if rng.gen_bool(0.5) {
            let len = rng.gen_range(MIN..MAX);
            Alphanumeric.sample_string(&mut rng, len)
        } else {
            String::new()
        };
        let ext_sinfo = rng.gen_bool(0.5).then_some(ext::SourceInfoType::rand());
        let ext_body = rng.gen_bool(0.5).then_some(ext::QueryBodyType::rand());
        let ext_attachment = rng.gen_bool(0.5).then_some(ext::AttachmentType::rand());
        let mut ext_unknown = Vec::new();
        for _ in 0..rng.gen_range(0..4) {
            ext_unknown.push(ZExtUnknown::rand2(
                iext::mid(ext::Attachment::ID) + 1,
                false,
            ));
        }

        Self {
            consolidation,
            parameters,
            ext_sinfo,
            ext_body,
            ext_attachment,
            ext_unknown,
        }
    }
}
