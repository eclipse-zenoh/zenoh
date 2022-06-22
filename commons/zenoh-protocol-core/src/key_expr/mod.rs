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

pub(crate) const DELIMITER: u8 = b'/';
pub(crate) const SINGLE_WILD: u8 = b'*';
pub(crate) const DOUBLE_WILD: &[u8] = b"**";
pub(crate) const FORBIDDEN_CHARS: [char; 3] = ['#', '?', '$'];

pub(crate) mod owned;
pub use owned::OwnedKeyExpr;

pub(crate) mod borrowed;
pub use borrowed::*;

pub mod canon;
pub mod include;
pub mod intersect;
pub(crate) mod utils;

pub mod fuzzer;
#[cfg(test)]
pub mod test;
