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
//! There are 2 variants of prelude: sync and async. The only difference is that the async prelude
//! reexports the [`AsyncResolve`] trait and the sync prelude reexports the [`SyncResolve`] trait.
//! Both traits provides method `res()` for finalizing the operation, whcih is synchronous in the
//! sync API and asynchronous in the async API.
//!
//! If both sync and async preludes are imported, the [`res_sync()`] and [`res_async()`] methods
//! should be used explicitly.
//!
//! # Examples
//!
//! ```
//! use zenoh::prelude::r#async::*;
//! ```
//!
//! ```
//! use zenoh::prelude::sync::*;
//! ```

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

pub(crate) mod common {
    pub use zenoh_config::Config;
    pub use zenoh_config::Locator;
    pub use zenoh_config::ValidatedMap;
    pub use zenoh_protocol::core::{CongestionControl, Reliability, WhatAmI};
    pub use zenoh_result::ZResult as Result;

    #[zenoh_macros::unstable]
    pub use crate::sample::Attachment;
    pub use crate::sample::{
        Locality, QoSBuilderTrait, Sample, SampleBuilderTrait, SampleKind, SourceInfo,
        TimestampBuilderTrait, ValueBuilderTrait,
    };

    pub use crate::query::Reply;

    pub use crate::session::{open, Session, SessionDeclarations};

    pub use crate::publication::Priority;

    pub use crate::key_expr::{kedefine, keformat};

    //     /// A zenoh error.
    //     // pub use zenoh_result::Error;
    //     /// A zenoh result.
    //     // pub use zenoh_result::ZResult as Result;
    // #[cfg(feature = "shared-memory")]
    // pub use zenoh_shm as shm;

    //     pub use crate::key_expr::{keyexpr, KeyExpr, OwnedKeyExpr};
    //     pub use zenoh_buffers::{
    //         buffer::{Buffer, SplitBuffer},
    //         reader::HasReader,
    //         writer::HasWriter,
    //     };
    //     pub use zenoh_core::Resolve;
    //     pub use zenoh_macros::{ke, kedefine, keformat, kewrite};

    //     pub use zenoh_protocol::core::{EndPoint, Locator, ZenohId};
    //     #[zenoh_macros::unstable]
    //     pub use zenoh_protocol::core::{EntityGlobalId, EntityId};

    //     pub use crate::config::{self, Config, ValidatedMap};
    //     pub use crate::handlers::IntoHandler;
    //     pub use crate::selector::{Parameter, Parameters, Selector};
    //     pub use crate::session::{Session, SessionDeclarations};

    //     pub use crate::query::{ConsolidationMode, QueryConsolidation, QueryTarget};

    //     pub use crate::encoding::Encoding;
    //     /// The encoding of a zenoh `Value`.
    //     pub use crate::payload::{Deserialize, Payload, Serialize};
    //     pub use crate::value::Value;

    //     #[zenoh_macros::unstable]
    //     pub use crate::sample::Locality;
    //     #[cfg(not(feature = "unstable"))]
    //     pub(crate) use crate::sample::Locality;
    //     #[zenoh_macros::unstable]
    //     pub use crate::sample::SourceInfo;
    //     pub use crate::sample::{Sample, SampleKind};

    //     pub use crate::publication::Priority;
    //     #[zenoh_macros::unstable]
    //     pub use crate::publication::PublisherDeclarations;

    //     pub use crate::sample::builder::{
    //         QoSBuilderTrait, SampleBuilderTrait, TimestampBuilderTrait, ValueBuilderTrait,
    //     };
    // /// A collection of useful buffers used by zenoh internally and exposed to the user to facilitate
    // /// reading and writing data.
    //  pub use zenoh_buffers as buffers;
    // pub use scouting::scout;
    // pub use session::open;
}
