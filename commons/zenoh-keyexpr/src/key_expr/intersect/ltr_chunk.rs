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
// use crate::key_expr::SINGLE_WILD;

// use super::{restiction::NoBigWilds, Intersector};

// pub struct LTRChunkIntersector;
// impl Intersector<NoBigWilds<&[u8]>, NoBigWilds<&[u8]>> for LTRChunkIntersector {
//     fn intersect(&self, mut left: NoBigWilds<&[u8]>, mut right: NoBigWilds<&[u8]>) -> bool {
//         loop {
//             match (left.0, right.0) {
//                 ([], []) | (b"*", _) | (_, b"*") => return true,
//                 ([SINGLE_WILD, new_left @ ..], [_, new_right @ ..]) => {
//                     if self.intersect(NoBigWilds(new_left), right) {
//                         return true;
//                     }
//                     right = NoBigWilds(new_right)
//                 }
//                 ([_, new_left @ ..], [SINGLE_WILD, new_right @ ..]) => {
//                     if self.intersect(left, NoBigWilds(new_right)) {
//                         return true;
//                     }
//                     left = NoBigWilds(new_left)
//                 }
//                 ([a, b @ ..], [c, d @ ..]) if a == c => {
//                     left = NoBigWilds(b);
//                     right = NoBigWilds(d)
//                 }
//                 _ => return false,
//             }
//         }
//     }
// }
