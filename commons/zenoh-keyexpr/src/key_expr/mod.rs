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

//! This module implements the Key Expression Language, as explained in details in [`keyexpr`]'s documentation.

pub(crate) const DELIMITER: u8 = b'/';
pub(crate) const SINGLE_WILD: u8 = b'*';
pub(crate) const DOUBLE_WILD: &[u8] = b"**";
pub(crate) const STAR_DSL: &[u8] = b"$*";

pub(crate) mod owned;
pub use owned::{OwnedKeyExpr, OwnedNonWildKeyExpr};

pub(crate) mod borrowed;
pub use borrowed::*;

/// Used to implement and expose the tools to implement canonization of Key Expressions for string-like types.
/// The average user doesn't need to bother with it.
pub mod canon;
/// Used to implement and expose the tools to implement algorithms to detect Key Expression inclusivity.
/// The average user doesn't need to bother with it.
pub mod include;
/// Used to implement and expose the tools to implement algorithms to detect Key Expression intersection.
/// The average user doesn't need to bother with it.
pub mod intersect;
pub(crate) mod utils;

/// Exposes a random Key Expression generator to help with testing.
#[cfg(feature = "std")]
pub mod fuzzer;

pub mod format;

#[cfg(test)]
mod tests;
