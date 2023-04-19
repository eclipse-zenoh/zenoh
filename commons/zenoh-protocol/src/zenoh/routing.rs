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
use crate::core::ZInt;

/// -- RoutingContext decorator
///
/// ```text
/// The **RoutingContext** is a message decorator containing
/// informations for routing the concerned message.
///
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |X|X|X| RT_CTX  |
/// +-+-+-+---------+
/// ~      tid      ~
/// +---------------+
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoutingContext {
    pub tree_id: ZInt,
}

impl RoutingContext {
    pub fn new(tree_id: ZInt) -> RoutingContext {
        RoutingContext { tree_id }
    }
}

impl RoutingContext {
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use rand::Rng;

        let mut rng = rand::thread_rng();

        Self { tree_id: rng.gen() }
    }
}
