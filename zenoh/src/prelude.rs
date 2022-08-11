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

//! A "prelude" for crates using the `zenoh` crate.
//!
//! This prelude is similar to the standard library's prelude in that you'll
//! almost always want to import its entire contents, but unlike the standard
//! library's prelude you'll have to do so manually. An example of using this is:
//!
//! ```
//! use zenoh::prelude::*;
//! ```

pub mod sync {
    pub use super::common::*;
    pub use zenoh_core::SyncResolve;
}
pub mod r#async {
    pub use super::common::*;
    pub use zenoh_core::AsyncResolve;
}

pub use common::*;
pub(crate) mod common {
    pub use crate::key_expr::{keyexpr, KeyExpr, OwnedKeyExpr};
    pub use zenoh_buffers::SplitBuffer;

    pub(crate) type Id = usize;

    pub use crate::config;
    pub use crate::handlers::IntoCallbackReceiverPair;
    pub use crate::properties::Properties;
    pub use crate::publication::Priority;
    pub use crate::sample::Sample;
    pub use crate::selector::{KeyValuePair, Selector, ValueSelector};
    pub use crate::session::SessionDeclarations;
    pub use crate::value::Value;
    pub use zenoh_config::ValidatedMap;

    /// A [`Locator`] contains a choice of protocol, an address and port, as well as optional additional properties to work with.
    pub use zenoh_protocol_core::Locator;

    /// The encoding of a zenoh [`Value`].
    pub use zenoh_protocol_core::{Encoding, KnownEncoding};

    pub use zenoh_protocol_core::SampleKind;
    pub use zenoh_protocol_core::ZInt;
    /// The global unique id of a zenoh peer.
    pub use zenoh_protocol_core::ZenohId;
}
