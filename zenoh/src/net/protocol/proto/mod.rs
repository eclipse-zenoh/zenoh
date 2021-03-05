//
// Copyright (c) 2017, 2020 ADLINK Technology Inc.
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ADLINK zenoh team, <zenoh@adlink-labs.tech>
//

mod constants;
mod msg;
mod msg_reader;
mod msg_shm;
mod msg_writer;

pub use constants::*;
pub use msg::*;
pub use msg_reader::*;
pub use msg_shm::*;
pub use msg_writer::*;

use super::core;
use super::io;
use super::link;

// pub fn compare_times(a: &Option<DataInfo>, b: &Option<DataInfo>) -> i8 {
//     match (a, b) {
//         (Some(a), Some(b)) => {
//             match (a.timestamp, b.timestamp) {
//                 (Some(a), Some(b)) => {
//                     let ntp_diff = a.get_time().as_u64() - b.get_time().as_u64();
//                     if ntp_diff > 0 {
//                         1
//                     } else if ntp_diff < 0 {
//                         -1
//                     } else {
//                         let id_diff = a.get_id() - b.get_id();
//                         if id_diff > 0 {
//                             1
//                         } else if id_diff < 0 {
//                             -1
//                         } else {
//                             0
//                         }
//                     }
//                 }
//                 (Some(a), None) => 1,
//                 (None, Some(b)) => -1,
//                 (None, None) => 0,
//             }
//         }
//         (Some(a), None) => 1,
//         (None, Some(b)) => -1,
//         (None, None) => 0,
//     }
// }
