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
// use super::{
//     restiction::{NoBigWilds, NoSubWilds},
//     Intersector,
// };
// use crate::key_expr::{utils::Split, DELIMITER};

// #[derive(Debug, Clone, Copy)]
// pub struct LeftToRightIntersector<ChunkIntersector>(pub ChunkIntersector);
// impl<'a, ChunkIntersector> Intersector<NoSubWilds<&'a [u8]>, NoSubWilds<&'a [u8]>>
//     for LeftToRightIntersector<ChunkIntersector>
// {
//     fn intersect(&self, left: NoSubWilds<&'a [u8]>, right: NoSubWilds<&'a [u8]>) -> bool {
//         let mut left = left.0;
//         let mut right = right.0;
//         loop {
//             let (l, new_left) = left.split_once(&DELIMITER);
//             let (r, new_right) = right.split_once(&DELIMITER);
//             match ((l, new_left), (r, new_right)) {
//                 (([], []), ([], [])) | ((b"**", []), _) | (_, (b"**", [])) => return true,
//                 ((b"**", _), _) => {
//                     if self.intersect(NoSubWilds(new_left), NoSubWilds(right)) {
//                         return true;
//                     }
//                     if r.is_empty() && new_right.is_empty() {
//                         left = new_left;
//                     }
//                     right = new_right
//                 }
//                 (_, (b"**", _)) => {
//                     if self.intersect(NoSubWilds(left), NoSubWilds(new_right)) {
//                         return true;
//                     }
//                     if l.is_empty() && new_left.is_empty() {
//                         right = new_right;
//                     }
//                     left = new_left
//                 }
//                 (([], []), _) | (_, ([], [])) => return false,
//                 _ => {
//                     if l == r || l == b"*" || r == b"*" {
//                         left = new_left;
//                         right = new_right
//                     } else {
//                         return false;
//                     }
//                 }
//             }
//         }
//     }
// }
// impl<'a, ChunkIntersector: Intersector<NoBigWilds<&'a [u8]>, NoBigWilds<&'a [u8]>>>
//     Intersector<&'a [u8], &'a [u8]> for LeftToRightIntersector<ChunkIntersector>
// {
//     fn intersect(&self, mut left: &'a [u8], mut right: &'a [u8]) -> bool {
//         loop {
//             let (l, new_left) = left.split_once(&DELIMITER);
//             let (r, new_right) = right.split_once(&DELIMITER);
//             match ((l, new_left), (r, new_right)) {
//                 (([], []), ([], [])) | ((b"**", []), _) | (_, (b"**", [])) => return true,
//                 ((b"**", _), _) => {
//                     if self.intersect(new_left, right) {
//                         return true;
//                     }
//                     if r.is_empty() && new_right.is_empty() {
//                         left = new_left;
//                     }
//                     right = new_right
//                 }
//                 (_, (b"**", _)) => {
//                     if self.intersect(left, new_right) {
//                         return true;
//                     }
//                     if l.is_empty() && new_left.is_empty() {
//                         right = new_right;
//                     }
//                     left = new_left
//                 }
//                 (([], []), _) | (_, ([], [])) => return false,
//                 _ => {
//                     if self.0.intersect(NoBigWilds(l), NoBigWilds(r)) {
//                         left = new_left;
//                         right = new_right
//                     } else {
//                         return false;
//                     }
//                 }
//             }
//         }
//     }
// }
