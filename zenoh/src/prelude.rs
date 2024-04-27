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

// Reexport API in flat namespace
pub(crate) mod flat {
    pub use crate::buffers::*;
    pub use crate::bytes::*;
    pub use crate::config::*;
    pub use crate::core::{Error as ZError, Resolvable, Resolve, Result as ZResult};
    pub use crate::encoding::*;
    pub use crate::handlers::*;
    pub use crate::key_expr::*;
    pub use crate::publication::*;
    pub use crate::query::*;
    pub use crate::queryable::*;
    pub use crate::sample::*;
    pub use crate::scouting::*;
    pub use crate::selector::*;
    pub use crate::session::*;
    #[cfg(feature = "shared-memory")]
    pub use crate::shm::*;
    pub use crate::subscriber::*;
    pub use crate::time::*;
    pub use crate::value::*;
}

// Reexport API in hierarchical namespace
pub(crate) mod mods {
    pub use crate::buffers;
    pub use crate::bytes;
    pub use crate::config;
    pub use crate::core;
    pub use crate::encoding;
    pub use crate::handlers;
    pub use crate::key_expr;
    pub use crate::publication;
    pub use crate::query;
    pub use crate::queryable;
    pub use crate::sample;
    pub use crate::scouting;
    pub use crate::selector;
    pub use crate::session;
    #[cfg(feature = "shared-memory")]
    pub use crate::shm;
    pub use crate::subscriber;
    pub use crate::time;
    pub use crate::value;
}

pub use flat::*;
pub use mods::*;
pub use zenoh_core::Wait;

/// Prelude to import when using Zenoh's sync API.
#[deprecated = "use `zenoh::prelude` instead"]
pub mod sync {
    pub use super::flat::*;
    pub use super::mods::*;
    #[allow(deprecated)]
    pub use zenoh_core::SyncResolve;
}
/// Prelude to import when using Zenoh's async API.
#[deprecated = "use `zenoh::prelude` instead"]
pub mod r#async {
    pub use super::flat::*;
    pub use super::mods::*;
    #[allow(deprecated)]
    pub use zenoh_core::AsyncResolve;
}
