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
use zenoh_buffers::ZSlice;
use zenoh_protocol::core::{WhatAmI, ZInt, ZenohId};
use zenoh_protocol::{proto::defaults::SEQ_NUM_RES, transport::tmsg};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitSyn {
    pub version: u8,
    pub whatami: WhatAmI,
    pub zid: ZenohId,
    pub sn_resolution: ZInt,
    pub is_qos: bool,
}

// impl Header for InitSyn {
//     #[inline(always)]
//     fn header(&self) -> u8 {
//         let mut header = tmsg::id::INIT;
//         if self.sn_resolution != SEQ_NUM_RES {
//             header |= tmsg::flag::S;
//         }
//         if self.has_options() {
//             header |= tmsg::flag::O;
//         }
//         header
//     }
// }

// impl Options for InitSyn {
//     fn options(&self) -> ZInt {
//         let mut options = 0;
//         if self.is_qos {
//             options |= tmsg::init_options::QOS;
//         }
//         options
//     }

//     fn has_options(&self) -> bool {
//         self.is_qos
//     }
// }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitAck {
    pub whatami: WhatAmI,
    pub zid: ZenohId,
    pub sn_resolution: Option<ZInt>,
    pub is_qos: bool,
    pub cookie: ZSlice,
}

// impl Header for InitAck {
//     #[inline(always)]
//     fn header(&self) -> u8 {
//         let mut header = tmsg::id::INIT;
//         header |= tmsg::flag::A;
//         if self.sn_resolution.is_some() {
//             header |= tmsg::flag::S;
//         }
//         if self.has_options() {
//             header |= tmsg::flag::O;
//         }
//         header
//     }
// }

// impl Options for InitAck {
//     fn options(&self) -> ZInt {
//         let mut options = 0;
//         if self.is_qos {
//             options |= tmsg::init_options::QOS;
//         }
//         options
//     }

//     fn has_options(&self) -> bool {
//         self.is_qos
//     }
// }
