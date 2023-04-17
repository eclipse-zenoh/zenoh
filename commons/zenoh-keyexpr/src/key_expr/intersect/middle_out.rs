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
// use crate::key_expr::{utils::Split, DELIMITER, DOUBLE_WILD};

// pub struct MiddleOutIntersector<ChunkIntersector>(pub ChunkIntersector);
// impl<ChunkIntersector: for<'a> Intersector<NoBigWilds<&'a [u8]>, NoBigWilds<&'a [u8]>>>
//     Intersector<&[u8], &[u8]> for MiddleOutIntersector<ChunkIntersector>
// {
//     fn intersect(&self, left: &[u8], right: &[u8]) -> bool {
//         let mut split_left = left.splitter(DOUBLE_WILD);
//         let mut split_right = right.splitter(DOUBLE_WILD);
//         let ll = split_left
//             .left()
//             .map(|x| NoBigWilds(&x[..(x.len().saturating_sub(1))]));
//         let lr = split_left
//             .right()
//             .map(|x| NoBigWilds(&x[(!x.is_empty() as usize)..]));
//         let rl = split_right
//             .left()
//             .map(|x| NoBigWilds(&x[..(x.len().saturating_sub(1))]));
//         let rr = split_right
//             .right()
//             .map(|x| NoBigWilds(&x[(!x.is_empty() as usize)..]));
//         let lm = split_left.unwrap();
//         let lm = &lm[lm.len().min(ll.is_some() as usize)..(lm.len() - lr.is_some() as usize)];
//         let rm = split_right.unwrap();
//         let rm = &rm[rm.len().min(rl.is_some() as usize)..(rm.len() - rr.is_some() as usize)];
//         match (ll, lr, rl, rr) {
//             (None, None, None, None) => self.intersect(NoBigWilds(lm), NoBigWilds(rm)),
//             (None, None, Some(rl), None) => self.intersect(NoBigWilds(lm), (rl, NoBigWilds(rm))),
//             (None, None, Some(rl), Some(rr)) => self.intersect(NoBigWilds(lm), (rl, rm, rr)),
//             (Some(ll), None, None, None) => self.intersect(NoBigWilds(rm), (ll, NoBigWilds(lm))),
//             (Some(ll), None, Some(rl), None) => {
//                 self.intersect((ll, NoBigWilds(lm)), (rl, NoBigWilds(rm)))
//             }
//             (Some(ll), None, Some(rl), Some(rr)) => self.intersect((ll, NoBigWilds(lm)), (rl, rr)),
//             (Some(ll), Some(lr), None, None) => self.intersect(NoBigWilds(rm), (ll, lm, lr)),
//             (Some(ll), Some(lr), Some(rl), None) => self.intersect((ll, lr), (rl, NoBigWilds(rm))),
//             (Some(ll), Some(lr), Some(rl), Some(rr)) => self.intersect((ll, lr), (rl, rr)),
//             _ => unreachable!(),
//         }
//     }
// }

// impl<'a, ChunkIntersector: Intersector<NoBigWilds<&'a [u8]>, NoBigWilds<&'a [u8]>>>
//     Intersector<NoBigWilds<&'a [u8]>, NoBigWilds<&'a [u8]>>
//     for MiddleOutIntersector<ChunkIntersector>
// {
//     fn intersect(&self, left: NoBigWilds<&'a [u8]>, right: NoBigWilds<&'a [u8]>) -> bool {
//         let left = left.splitter(&DELIMITER);
//         let mut right = right.splitter(&DELIMITER);
//         for left in left {
//             if let Some(right) = right.next() {
//                 if !self.0.intersect(NoBigWilds(left), NoBigWilds(right)) {
//                     return false;
//                 }
//             } else {
//                 return false;
//             }
//         }
//         right.inner().is_none()
//     }
// }

// impl<ChunkIntersector: for<'a> Intersector<NoBigWilds<&'a [u8]>, NoBigWilds<&'a [u8]>>>
//     Intersector<NoBigWilds<&[u8]>, (NoBigWilds<&[u8]>, NoBigWilds<&[u8]>)>
//     for MiddleOutIntersector<ChunkIntersector>
// {
//     fn intersect(
//         &self,
//         left: NoBigWilds<&[u8]>,
//         right: (NoBigWilds<&[u8]>, NoBigWilds<&[u8]>),
//     ) -> bool {
//         self.strip_prefix(&left, &right.0)
//             .map(|l| self.is_suffix(l, &right.1))
//             .unwrap_or(false)
//     }
// }

// impl<ChunkIntersector: for<'a> Intersector<NoBigWilds<&'a [u8]>, NoBigWilds<&'a [u8]>>>
//     Intersector<NoBigWilds<&[u8]>, (NoBigWilds<&[u8]>, &[u8], NoBigWilds<&[u8]>)>
//     for MiddleOutIntersector<ChunkIntersector>
// {
//     fn intersect(
//         &self,
//         left: NoBigWilds<&[u8]>,
//         right: (NoBigWilds<&[u8]>, &[u8], NoBigWilds<&[u8]>),
//     ) -> bool {
//         self.strip_prefix(&left, &right.0)
//             .and_then(|l| self.strip_suffix(l, &right.2))
//             .map(|l| self.contains(l, right.1))
//             .unwrap_or(false)
//     }
// }

// impl<ChunkIntersector: for<'a> Intersector<NoBigWilds<&'a [u8]>, NoBigWilds<&'a [u8]>>>
//     Intersector<(NoBigWilds<&[u8]>, NoBigWilds<&[u8]>), (NoBigWilds<&[u8]>, NoBigWilds<&[u8]>)>
//     for MiddleOutIntersector<ChunkIntersector>
// {
//     fn intersect(
//         &self,
//         left: (NoBigWilds<&[u8]>, NoBigWilds<&[u8]>),
//         right: (NoBigWilds<&[u8]>, NoBigWilds<&[u8]>),
//     ) -> bool {
//         (self.is_prefix(&left.0, &right.0) || self.is_prefix(&right.0, &left.0))
//             && (self.is_suffix(&left.1, &right.1) || self.is_suffix(&right.1, &left.1))
//     }
// }

// impl<ChunkIntersector: for<'a> Intersector<NoBigWilds<&'a [u8]>, NoBigWilds<&'a [u8]>>>
//     MiddleOutIntersector<ChunkIntersector>
// {
//     fn is_prefix(&self, str: &[u8], needle: &[u8]) -> bool {
//         self.strip_prefix(str, needle).is_some()
//     }
//     fn is_suffix(&self, str: &[u8], needle: &[u8]) -> bool {
//         self.strip_suffix(str, needle).is_some()
//     }
//     fn strip_prefix<'a>(&self, str: &'a [u8], needle: &[u8]) -> Option<&'a [u8]> {
//         if needle.is_empty() {
//             return Some(str);
//         }
//         let needle = needle.splitter(&DELIMITER);
//         let mut str = str.splitter(&DELIMITER);
//         for needle in needle {
//             if let Some(str) = str.next() {
//                 if !self.0.intersect(NoBigWilds(str), NoBigWilds(needle)) {
//                     return None;
//                 }
//             } else {
//                 return None;
//             }
//         }
//         str.inner().or(Some(b""))
//     }
//     fn strip_suffix<'a>(&self, str: &'a [u8], needle: &[u8]) -> Option<&'a [u8]> {
//         if needle.is_empty() {
//             return Some(str);
//         }
//         let mut needle = needle.splitter(&DELIMITER);
//         let mut str = str.splitter(&DELIMITER);
//         while let Some(needle) = needle.next_back() {
//             if let Some(str) = str.next_back() {
//                 if !self.0.intersect(NoBigWilds(str), NoBigWilds(needle)) {
//                     return None;
//                 }
//             } else {
//                 return None;
//             }
//         }
//         str.inner().or(Some(b""))
//     }
//     /// This is only called if `str` doesn't contain any `**`. `needle` might contain some though.
//     fn contains(&self, str: &[u8], needle: &[u8]) -> bool {
//         let mut chunks = str.splitter(&DELIMITER);
//         for needle in needle.splitter(DOUBLE_WILD) {
//             let original_needle = needle
//                 .splitter(&DELIMITER)
//                 .filter(|chunk| !chunk.is_empty());
//             'current_needle: loop {
//                 let mut chunks_preview = chunks.clone();
//                 for needle in original_needle.clone() {
//                     if let Some(chunk) = chunks_preview.next() {
//                         if !self.0.intersect(NoBigWilds(chunk), NoBigWilds(needle)) {
//                             chunks.next();
//                             continue 'current_needle;
//                         }
//                     } else {
//                         return false;
//                     }
//                 }
//                 chunks = chunks_preview;
//                 break 'current_needle;
//             }
//         }
//         true
//     }
// }

// impl<ChunkIntersector> Intersector<NoSubWilds<&[u8]>, NoSubWilds<&[u8]>>
//     for MiddleOutIntersector<ChunkIntersector>
// {
//     fn intersect(&self, left: NoSubWilds<&[u8]>, right: NoSubWilds<&[u8]>) -> bool {
//         let mut split_left = left.0.splitter(DOUBLE_WILD);
//         let mut split_right = right.0.splitter(DOUBLE_WILD);
//         let ll = split_left
//             .left()
//             .map(|x| NoBigWilds(&x[..(x.len().saturating_sub(1))]));
//         let lr = split_left
//             .right()
//             .map(|x| NoBigWilds(&x[(!x.is_empty() as usize)..]));
//         let rl = split_right
//             .left()
//             .map(|x| NoBigWilds(&x[..(x.len().saturating_sub(1))]));
//         let rr = split_right
//             .right()
//             .map(|x| NoBigWilds(&x[(!x.is_empty() as usize)..]));
//         let lm = split_left.unwrap();
//         let lm = &lm[lm.len().min(ll.is_some() as usize)..(lm.len() - lr.is_some() as usize)];
//         let rm = split_right.unwrap();
//         let rm = &rm[rm.len().min(rl.is_some() as usize)..(rm.len() - rr.is_some() as usize)];
//         match (ll, lr, rl, rr) {
//             (None, None, None, None) => {
//                 self.intersect(NoSubWilds(NoBigWilds(lm)), NoSubWilds(NoBigWilds(rm)))
//             }
//             (None, None, Some(rl), None) => {
//                 self.intersect(NoSubWilds(NoBigWilds(lm)), NoSubWilds((rl, NoBigWilds(rm))))
//             }
//             (None, None, Some(rl), Some(rr)) => {
//                 self.intersect(NoSubWilds(NoBigWilds(lm)), NoSubWilds((rl, rm, rr)))
//             }
//             (Some(ll), None, None, None) => {
//                 self.intersect(NoSubWilds(NoBigWilds(rm)), NoSubWilds((ll, NoBigWilds(lm))))
//             }
//             (Some(ll), None, Some(rl), None) => self.intersect(
//                 NoSubWilds((ll, NoBigWilds(lm))),
//                 NoSubWilds((rl, NoBigWilds(rm))),
//             ),
//             (Some(ll), None, Some(rl), Some(rr)) => {
//                 self.intersect(NoSubWilds((ll, NoBigWilds(lm))), NoSubWilds((rl, rr)))
//             }
//             (Some(ll), Some(lr), None, None) => {
//                 self.intersect(NoSubWilds(NoBigWilds(rm)), NoSubWilds((ll, lm, lr)))
//             }
//             (Some(ll), Some(lr), Some(rl), None) => {
//                 self.intersect(NoSubWilds((ll, lr)), NoSubWilds((rl, NoBigWilds(rm))))
//             }
//             (Some(ll), Some(lr), Some(rl), Some(rr)) => {
//                 self.intersect(NoSubWilds((ll, lr)), NoSubWilds((rl, rr)))
//             }
//             _ => unreachable!(),
//         }
//     }
// }

// impl<'a, ChunkIntersector>
//     Intersector<NoSubWilds<NoBigWilds<&'a [u8]>>, NoSubWilds<NoBigWilds<&'a [u8]>>>
//     for MiddleOutIntersector<ChunkIntersector>
// {
//     fn intersect(
//         &self,
//         left: NoSubWilds<NoBigWilds<&'a [u8]>>,
//         right: NoSubWilds<NoBigWilds<&'a [u8]>>,
//     ) -> bool {
//         let left = left.0.splitter(&DELIMITER);
//         let mut right = right.0.splitter(&DELIMITER);
//         for left in left {
//             if let Some(right) = right.next() {
//                 if !(left == right || left == b"*" || right == b"*") {
//                     return false;
//                 }
//             } else {
//                 return false;
//             }
//         }
//         right.inner().is_none()
//     }
// }

// impl<ChunkIntersector>
//     Intersector<NoSubWilds<NoBigWilds<&[u8]>>, NoSubWilds<(NoBigWilds<&[u8]>, NoBigWilds<&[u8]>)>>
//     for MiddleOutIntersector<ChunkIntersector>
// {
//     fn intersect(
//         &self,
//         left: NoSubWilds<NoBigWilds<&[u8]>>,
//         right: NoSubWilds<(NoBigWilds<&[u8]>, NoBigWilds<&[u8]>)>,
//     ) -> bool {
//         NoSubWildMatcher
//             .strip_prefix(&left.0, &right.0 .0)
//             .map(|l| NoSubWildMatcher.is_suffix(l, &right.0 .1))
//             .unwrap_or(false)
//     }
// }

// impl<ChunkIntersector>
//     Intersector<
//         NoSubWilds<NoBigWilds<&[u8]>>,
//         NoSubWilds<(NoBigWilds<&[u8]>, &[u8], NoBigWilds<&[u8]>)>,
//     > for MiddleOutIntersector<ChunkIntersector>
// {
//     fn intersect(
//         &self,
//         left: NoSubWilds<NoBigWilds<&[u8]>>,
//         right: NoSubWilds<(NoBigWilds<&[u8]>, &[u8], NoBigWilds<&[u8]>)>,
//     ) -> bool {
//         NoSubWildMatcher
//             .strip_prefix(&left.0, &right.0 .0)
//             .and_then(|l| NoSubWildMatcher.strip_suffix(l, &right.0 .2))
//             .map(|l| NoSubWildMatcher.contains(l, right.0 .1))
//             .unwrap_or(false)
//     }
// }

// impl<ChunkIntersector>
//     Intersector<
//         NoSubWilds<(NoBigWilds<&[u8]>, NoBigWilds<&[u8]>)>,
//         NoSubWilds<(NoBigWilds<&[u8]>, NoBigWilds<&[u8]>)>,
//     > for MiddleOutIntersector<ChunkIntersector>
// {
//     fn intersect(
//         &self,
//         left: NoSubWilds<(NoBigWilds<&[u8]>, NoBigWilds<&[u8]>)>,
//         right: NoSubWilds<(NoBigWilds<&[u8]>, NoBigWilds<&[u8]>)>,
//     ) -> bool {
//         (NoSubWildMatcher.is_prefix(&left.0 .0, &right.0 .0)
//             || NoSubWildMatcher.is_prefix(&right.0 .0, &left.0 .0))
//             && (NoSubWildMatcher.is_suffix(&left.0 .1, &right.0 .1)
//                 || NoSubWildMatcher.is_suffix(&right.0 .1, &left.0 .1))
//     }
// }

// struct NoSubWildMatcher;
// impl NoSubWildMatcher {
//     fn strip_prefix<'a>(&self, from: &'a [u8], prefix: &[u8]) -> Option<&'a [u8]> {
//         let mut from = from.splitter(&DELIMITER);
//         let prefix = prefix.splitter(&DELIMITER);
//         for prefix_chunk in prefix {
//             if prefix_chunk.is_empty() {
//                 break;
//             }
//             let from_chunk = from.next()?;
//             if !(from_chunk == prefix_chunk || prefix_chunk == b"*" || from_chunk == b"*") {
//                 return None;
//             }
//         }
//         Some(from.inner().unwrap_or(&[]))
//     }
//     fn is_prefix<'a>(&self, from: &'a [u8], prefix: &[u8]) -> bool {
//         self.strip_prefix(from, prefix).is_some()
//     }
//     fn strip_suffix<'a>(&self, from: &'a [u8], suffix: &[u8]) -> Option<&'a [u8]> {
//         let mut from = from.splitter(&DELIMITER);
//         let mut suffix = suffix.splitter(&DELIMITER);
//         while let Some(suffix_chunk) = suffix.next_back() {
//             if suffix_chunk.is_empty() {
//                 break;
//             }
//             let from_chunk = from.next_back()?;
//             if !(from_chunk == suffix_chunk || suffix_chunk == b"*" || from_chunk == b"*") {
//                 return None;
//             }
//         }
//         Some(from.inner().unwrap_or(&[]))
//     }
//     fn is_suffix<'a>(&self, from: &'a [u8], prefix: &[u8]) -> bool {
//         self.strip_suffix(from, prefix).is_some()
//     }
//     //    fn contains(&self, str: &[u8], needle: &[u8]) -> bool {

//     fn contains(&self, container: &[u8], needle: &[u8]) -> bool {
//         let mut chunks = container.splitter(&DELIMITER);
//         for needle in needle.splitter(DOUBLE_WILD) {
//             let original_needle = needle
//                 .splitter(&DELIMITER)
//                 .filter(|chunk| !chunk.is_empty());
//             'current_needle: loop {
//                 let mut chunks_preview = chunks.clone();
//                 for needle in original_needle.clone() {
//                     if let Some(chunk) = chunks_preview.next() {
//                         if !(needle == chunk || chunk == b"*" || needle == b"*") {
//                             chunks.next();
//                             continue 'current_needle;
//                         }
//                     } else {
//                         return false;
//                     }
//                 }
//                 chunks = chunks_preview;
//                 break 'current_needle;
//             }
//         }
//         true
//     }
// }
