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

use crate::key_expr::utils::Split;

use super::{keyexpr, DELIMITER, DOUBLE_WILD, SINGLE_WILD};

pub const DEFAULT_INTERSECTOR: LeftToRightIntersector<LTRChunkIntersector> =
    LeftToRightIntersector(LTRChunkIntersector);

/// The trait used to implement key expression intersectors.
///
/// Note that `Intersector<&keyexpr, &keyexpr>` is auto-implemented with quickchecks (`streq->true`, `strne&nowild->false`)
/// for any `Intersector<&[u8], &[u8]>`. Implementing `Intersector<&[u8], &[u8]>` is the recommended way to implement intersectors.
pub trait Intersector<Left, Right> {
    fn intersect(&self, left: Left, right: Right) -> bool;
}

impl<T: for<'a> Intersector<&'a [u8], &'a [u8]>> Intersector<&keyexpr, &keyexpr> for T {
    fn intersect(&self, left: &keyexpr, right: &keyexpr) -> bool {
        let left = left.as_bytes();
        let right = right.as_bytes();
        if left == right {
            return true;
        }
        if !left.contains(&b'*') && !right.contains(&b'*') {
            return false;
        }
        self.intersect(left, right)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct LeftToRightIntersector<ChunkIntersector>(pub ChunkIntersector);
pub struct LTRChunkIntersector;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NoBigWilds<T>(pub T);
impl<T> std::ops::Deref for NoBigWilds<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'a, ChunkIntersector: Intersector<NoBigWilds<&'a [u8]>, NoBigWilds<&'a [u8]>>>
    Intersector<&'a [u8], &'a [u8]> for LeftToRightIntersector<ChunkIntersector>
{
    fn intersect(&self, mut left: &'a [u8], mut right: &'a [u8]) -> bool {
        loop {
            let (l, new_left) = left.split_once(&DELIMITER);
            let (r, new_right) = right.split_once(&DELIMITER);
            match ((l, new_left), (r, new_right)) {
                (([], []), ([], [])) | ((DOUBLE_WILD, []), _) | (_, (DOUBLE_WILD, [])) => {
                    return true
                }
                ((DOUBLE_WILD, _), _) => {
                    if self.intersect(new_left, right) {
                        return true;
                    }
                    if r.is_empty() && new_right.is_empty() {
                        left = new_left;
                    }
                    right = new_right
                }
                (_, (DOUBLE_WILD, _)) => {
                    if self.intersect(left, new_right) {
                        return true;
                    }
                    if l.is_empty() && new_left.is_empty() {
                        right = new_right;
                    }
                    left = new_left
                }
                (([], []), _) | (_, ([], [])) => return false,
                _ => {
                    if self.0.intersect(NoBigWilds(l), NoBigWilds(r)) {
                        left = new_left;
                        right = new_right
                    } else {
                        return false;
                    }
                }
            }
        }
    }
}
impl Intersector<NoBigWilds<&[u8]>, NoBigWilds<&[u8]>> for LTRChunkIntersector {
    fn intersect(&self, mut left: NoBigWilds<&[u8]>, mut right: NoBigWilds<&[u8]>) -> bool {
        loop {
            match (left.0, right.0) {
                ([], []) | ([SINGLE_WILD], _) | (_, [SINGLE_WILD]) => return true,
                ([SINGLE_WILD, new_left @ ..], [_, new_right @ ..]) => {
                    if self.intersect(NoBigWilds(new_left), right) {
                        return true;
                    }
                    right = NoBigWilds(new_right)
                }
                ([_, new_left @ ..], [SINGLE_WILD, new_right @ ..]) => {
                    if self.intersect(left, NoBigWilds(new_right)) {
                        return true;
                    }
                    left = NoBigWilds(new_left)
                }
                ([a, b @ ..], [c, d @ ..]) if a == c => {
                    left = NoBigWilds(b);
                    right = NoBigWilds(d)
                }
                _ => return false,
            }
        }
    }
}

pub struct MiddleOutIntersector<ChunkIntersector>(pub ChunkIntersector);
impl<ChunkIntersector: for<'a> Intersector<NoBigWilds<&'a [u8]>, NoBigWilds<&'a [u8]>>>
    Intersector<&[u8], &[u8]> for MiddleOutIntersector<ChunkIntersector>
{
    fn intersect(&self, left: &[u8], right: &[u8]) -> bool {
        let mut split_left = left.spliter(DOUBLE_WILD);
        let mut split_right = right.spliter(DOUBLE_WILD);
        let ll = split_left
            .left()
            .map(|x| NoBigWilds(&x[..(x.len().saturating_sub(1))]));
        let lr = split_left
            .right()
            .map(|x| NoBigWilds(&x[(!x.is_empty() as usize)..]));
        let rl = split_right
            .left()
            .map(|x| NoBigWilds(&x[..(x.len().saturating_sub(1))]));
        let rr = split_right
            .right()
            .map(|x| NoBigWilds(&x[(!x.is_empty() as usize)..]));
        let lm = split_left.unwrap();
        let lm = &lm[lm.len().min(ll.is_some() as usize)..(lm.len() - lr.is_some() as usize)];
        let rm = split_right.unwrap();
        let rm = &rm[rm.len().min(rl.is_some() as usize)..(rm.len() - rr.is_some() as usize)];
        match (ll, lr, rl, rr) {
            (None, None, None, None) => self.intersect(NoBigWilds(lm), NoBigWilds(rm)),
            (None, None, Some(rl), None) => self.intersect(NoBigWilds(lm), (rl, NoBigWilds(rm))),
            (None, None, Some(rl), Some(rr)) => self.intersect(NoBigWilds(lm), (rl, rm, rr)),
            (Some(ll), None, None, None) => self.intersect(NoBigWilds(rm), (ll, NoBigWilds(lm))),
            (Some(ll), None, Some(rl), None) => {
                self.intersect((ll, NoBigWilds(lm)), (rl, NoBigWilds(rm)))
            }
            (Some(ll), None, Some(rl), Some(rr)) => self.intersect((ll, NoBigWilds(lm)), (rl, rr)),
            (Some(ll), Some(lr), None, None) => self.intersect(NoBigWilds(rm), (ll, lm, lr)),
            (Some(ll), Some(lr), Some(rl), None) => self.intersect((ll, lr), (rl, NoBigWilds(rm))),
            (Some(ll), Some(lr), Some(rl), Some(rr)) => self.intersect((ll, lr), (rl, rr)),
            _ => unreachable!(),
        }
    }
}

impl<'a, ChunkIntersector: Intersector<NoBigWilds<&'a [u8]>, NoBigWilds<&'a [u8]>>>
    Intersector<NoBigWilds<&'a [u8]>, NoBigWilds<&'a [u8]>>
    for MiddleOutIntersector<ChunkIntersector>
{
    fn intersect(&self, left: NoBigWilds<&'a [u8]>, right: NoBigWilds<&'a [u8]>) -> bool {
        let left = left.spliter(&DELIMITER);
        let mut right = right.spliter(&DELIMITER);
        for left in left {
            if let Some(right) = right.next() {
                if !self.0.intersect(NoBigWilds(left), NoBigWilds(right)) {
                    return false;
                }
            } else {
                return false;
            }
        }
        right.into_inner().is_none()
    }
}

impl<ChunkIntersector: for<'a> Intersector<NoBigWilds<&'a [u8]>, NoBigWilds<&'a [u8]>>>
    Intersector<NoBigWilds<&[u8]>, (NoBigWilds<&[u8]>, NoBigWilds<&[u8]>)>
    for MiddleOutIntersector<ChunkIntersector>
{
    fn intersect(
        &self,
        left: NoBigWilds<&[u8]>,
        right: (NoBigWilds<&[u8]>, NoBigWilds<&[u8]>),
    ) -> bool {
        self.strip_prefix(&left, &right.0)
            .map(|l| self.is_suffix(l, &right.1))
            .unwrap_or(false)
    }
}

impl<ChunkIntersector: for<'a> Intersector<NoBigWilds<&'a [u8]>, NoBigWilds<&'a [u8]>>>
    Intersector<NoBigWilds<&[u8]>, (NoBigWilds<&[u8]>, &[u8], NoBigWilds<&[u8]>)>
    for MiddleOutIntersector<ChunkIntersector>
{
    fn intersect(
        &self,
        left: NoBigWilds<&[u8]>,
        right: (NoBigWilds<&[u8]>, &[u8], NoBigWilds<&[u8]>),
    ) -> bool {
        self.strip_prefix(&left, &right.0)
            .and_then(|l| self.strip_suffix(l, &right.2))
            .map(|l| self.contains(l, right.1))
            .unwrap_or(false)
    }
}

impl<ChunkIntersector: for<'a> Intersector<NoBigWilds<&'a [u8]>, NoBigWilds<&'a [u8]>>>
    Intersector<(NoBigWilds<&[u8]>, NoBigWilds<&[u8]>), (NoBigWilds<&[u8]>, NoBigWilds<&[u8]>)>
    for MiddleOutIntersector<ChunkIntersector>
{
    fn intersect(
        &self,
        left: (NoBigWilds<&[u8]>, NoBigWilds<&[u8]>),
        right: (NoBigWilds<&[u8]>, NoBigWilds<&[u8]>),
    ) -> bool {
        (self.is_prefix(&left.0, &right.0) || self.is_prefix(&right.0, &left.0))
            && (self.is_suffix(&left.1, &right.1) || self.is_suffix(&right.1, &left.1))
    }
}

impl<ChunkIntersector: for<'a> Intersector<NoBigWilds<&'a [u8]>, NoBigWilds<&'a [u8]>>>
    MiddleOutIntersector<ChunkIntersector>
{
    fn is_prefix(&self, str: &[u8], needle: &[u8]) -> bool {
        self.strip_prefix(str, needle).is_some()
    }
    fn is_suffix(&self, str: &[u8], needle: &[u8]) -> bool {
        self.strip_suffix(str, needle).is_some()
    }
    fn strip_prefix<'a>(&self, str: &'a [u8], needle: &[u8]) -> Option<&'a [u8]> {
        if needle.is_empty() {
            return Some(str);
        }
        let needle = needle.spliter(&DELIMITER);
        let mut str = str.spliter(&DELIMITER);
        for needle in needle {
            if let Some(str) = str.next() {
                if !self.0.intersect(NoBigWilds(str), NoBigWilds(needle)) {
                    return None;
                }
            } else {
                return None;
            }
        }
        str.into_inner().or(Some(b""))
    }
    fn strip_suffix<'a>(&self, str: &'a [u8], needle: &[u8]) -> Option<&'a [u8]> {
        if needle.is_empty() {
            return Some(str);
        }
        let mut needle = needle.spliter(&DELIMITER);
        let mut str = str.spliter(&DELIMITER);
        while let Some(needle) = needle.next_back() {
            if let Some(str) = str.next_back() {
                if !self.0.intersect(NoBigWilds(str), NoBigWilds(needle)) {
                    return None;
                }
            } else {
                return None;
            }
        }
        str.into_inner().or(Some(b""))
    }
    /// This is only called if `str` doesn't contain any `**`. `needle` might contain some though.
    fn contains(&self, str: &[u8], needle: &[u8]) -> bool {
        let mut chunks = str.spliter(&DELIMITER);
        for needle in needle.spliter(DOUBLE_WILD) {
            let original_needle = needle.spliter(&DELIMITER).filter(|chunk| !chunk.is_empty());
            'current_needle: loop {
                let mut chunks_preview = chunks.clone();
                for needle in original_needle.clone() {
                    if let Some(chunk) = chunks_preview.next() {
                        if !self.0.intersect(NoBigWilds(chunk), NoBigWilds(needle)) {
                            chunks.next();
                            continue 'current_needle;
                        }
                    } else {
                        return false;
                    }
                }
                chunks = chunks_preview;
                break 'current_needle;
            }
        }
        true
    }
}
