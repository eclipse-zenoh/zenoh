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
use super::ReplyContext;
use crate::core::CongestionControl;

/// # Unit message
///
/// ```text
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |X|X|D|  UNIT   |
/// +-+-+-+---------+
///
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unit {
    pub congestion_control: CongestionControl,
    pub reply_context: Option<ReplyContext>,
}

impl Unit {
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use rand::Rng;

        let mut rng = rand::thread_rng();

        let congestion_control = if rng.gen_bool(0.5) {
            CongestionControl::Block
        } else {
            CongestionControl::Drop
        };
        let reply_context = if rng.gen_bool(0.5) {
            Some(ReplyContext::rand())
        } else {
            None
        };
        Self {
            congestion_control,
            reply_context,
        }
    }
}
