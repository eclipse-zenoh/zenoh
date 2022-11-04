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
use crate::core::{WireExpr, ZInt};

/// # Pull message
///
/// ```text
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |K|N|F|  PULL   |
/// +-+-+-+---------+
/// ~    KeyExpr     ~ if K==1 then key_expr has suffix
/// +---------------+
/// ~    pullid     ~
/// +---------------+
/// ~  max_samples  ~ if N==1
/// +---------------+
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pull {
    pub key: WireExpr<'static>,
    pub pull_id: ZInt,
    pub max_samples: Option<ZInt>,
    pub is_final: bool,
}

// Functions mainly used for testing
impl Pull {
    #[doc(hidden)]
    pub fn rand() -> Self {
        use rand::{
            distributions::{Alphanumeric, DistString},
            Rng,
        };

        const MIN: usize = 2;
        const MAX: usize = 16;

        let mut rng = rand::thread_rng();

        let scope: ZInt = rng.gen_range(0..20);
        let suffix: String = if rng.gen_bool(0.5) {
            let len = rng.gen_range(MIN..MAX);
            Alphanumeric.sample_string(&mut rng, len)
        } else {
            String::new()
        };
        let key = WireExpr {
            scope,
            suffix: suffix.into(),
        };

        let pull_id: ZInt = rng.gen();
        let max_samples = if rng.gen_bool(0.5) {
            Some(rng.gen())
        } else {
            None
        };
        let is_final = rng.gen_bool(0.5);

        Self {
            key,
            pull_id,
            max_samples,
            is_final,
        }
    }
}
