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
//! There are three variants of the prelude: full, sync and async. The sync one excludes the [`AsyncResolve`](crate::core::AsyncResolve) trait and the async one excludes the [`SyncResolve`](crate::core::SyncResolve) trait.
//! When specific sync or async prelude is included, the `res()` function of buildes works synchronously or asynchronously, respectively.
//!
//! If root prelude is included, the `res_sync()` or `res_async()` function of builders should be called explicitly.
//!
//! Examples:
//!
//! ```
//!`use zenoh::prelude::*;
//! ```
//! ```
//!`use zenoh::prelude::sync::*;
//! ```
//! ```
//!`use zenoh::prelude::r#async::*;
//! ```

// All API types and traits in flat namespace
pub(crate) mod flat {
    pub use crate::buffers::*;
    pub use crate::config::*;
    pub use crate::core::{AsyncResolve, Error, Resolvable, Resolve, Result, SyncResolve};
    pub use crate::encoding::*;
    pub use crate::handlers::*;
    pub use crate::key_expr::*;
    pub use crate::payload::*;
    pub use crate::plugins::*;
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

pub use crate::core::AsyncResolve;
pub use crate::core::SyncResolve;
pub use flat::*;

/// Prelude to import when using Zenoh's sync API.
pub mod sync {
    pub use super::flat::*;
    pub use crate::core::SyncResolve;
}
/// Prelude to import when using Zenoh's async API.
pub mod r#async {
    pub use super::flat::*;
    pub use crate::core::AsyncResolve;
}
