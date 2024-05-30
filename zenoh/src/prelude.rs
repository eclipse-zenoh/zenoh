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
//! library's prelude you'll have to do so manually.
//!
//! Examples:
//!
//! ```
//!use zenoh::prelude::*;
//! ```

mod _prelude {
    #[zenoh_macros::unstable]
    pub use crate::api::publisher::PublisherDeclarations;
    pub use crate::{
        api::{
            builders::{
                encoding::{AutoEncodingBuilderTrait, EncodingBuilderTrait},
                sample::{QoSBuilderTrait, SampleBuilderTrait, TimestampBuilderTrait},
            },
            session::{SessionDeclarations, Undeclarable},
        },
        config::ValidatedMap,
        core::{Error as ZError, Resolvable, Resolve, Result as ZResult},
    };
}

pub use _prelude::*;

#[allow(deprecated)]
pub use crate::core::AsyncResolve;
#[allow(deprecated)]
pub use crate::core::SyncResolve;
pub use crate::core::Wait;

/// Prelude to import when using Zenoh's sync API.
#[deprecated = "use `zenoh::prelude` instead"]
pub mod sync {
    pub use super::_prelude::*;
    #[allow(deprecated)]
    pub use crate::core::SyncResolve;
}
/// Prelude to import when using Zenoh's async API.
#[deprecated = "use `zenoh::prelude` instead"]
pub mod r#async {
    pub use super::_prelude::*;
    #[allow(deprecated)]
    pub use crate::core::AsyncResolve;
}
