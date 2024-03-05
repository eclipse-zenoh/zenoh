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

//! A "prelude" for crates using the `zenoh` crate.
//!
//! This prelude is similar to the standard library's prelude in that you'll
//! almost always want to import its entire contents, but unlike the standard
//! library's prelude you'll have to do so manually. An example of using this is:
//!
//! ```
//! use zenoh::prelude::r#async::*;
//! ```

pub use common::*;
pub(crate) mod common {
    pub use crate::key_expr::{keyexpr, KeyExpr, OwnedKeyExpr};
    pub use zenoh_buffers::{
        buffer::{Buffer, SplitBuffer},
        reader::HasReader,
        writer::HasWriter,
    };
    pub use zenoh_core::Resolve;
    pub use zenoh_protocol::core::{Encoding, EncodingId, EndPoint, Locator, ZenohId};

    pub use crate::config::{self, Config, ValidatedMap};
    pub use crate::handlers::IntoCallbackReceiverPair;
    pub use crate::session::{Session, SessionDeclarations};

    pub use crate::query::{ConsolidationMode, QueryConsolidation, QueryTarget};
    pub use crate::selector::{Parameter, Parameters, Selector};

    /// The encoding of a zenoh `Value`.
    pub use crate::payload::{DefaultSerializer, Deserialize, Payload, Serialize};
    pub use crate::value::{DefaultEncoding, Value};

    #[zenoh_macros::unstable]
    pub use crate::sample::Locality;
    #[cfg(not(feature = "unstable"))]
    pub(crate) use crate::sample::Locality;
    pub use crate::sample::{Sample, SampleKind};

    pub use crate::publication::Priority;
    #[zenoh_macros::unstable]
    pub use crate::publication::PublisherDeclarations;
    pub use zenoh_protocol::core::{CongestionControl, Reliability, WhatAmI};
}

/// Prelude to import when using Zenoh's sync API.
pub mod sync {
    pub use super::common::*;
    pub use zenoh_core::SyncResolve;
}
/// Prelude to import when using Zenoh's async API.
pub mod r#async {
    pub use super::common::*;
    pub use zenoh_core::AsyncResolve;
}
