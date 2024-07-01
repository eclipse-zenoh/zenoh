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
use super::{intersect::MayHaveVerbatim, keyexpr, utils::Split, DELIMITER, DOUBLE_WILD, STAR_DSL};

pub const DEFAULT_INCLUDER: LTRIncluder = LTRIncluder;

pub trait Includer<Left, Right> {
    /// Returns `true` if the set defined by `left` includes the one defined by `right`
    fn includes(&self, left: Left, right: Right) -> bool;
}

impl<T: for<'a> Includer<&'a [u8], &'a [u8]>> Includer<&keyexpr, &keyexpr> for T {
    fn includes(&self, left: &keyexpr, right: &keyexpr) -> bool {
        let left = left.as_bytes();
        let right = right.as_bytes();
        if left == right {
            return true;
        }
        self.includes(left, right)
    }
}

pub struct LTRIncluder;
impl Includer<&[u8], &[u8]> for LTRIncluder {
    fn includes(&self, mut left: &[u8], mut right: &[u8]) -> bool {
        loop {
            let (lchunk, lrest) = Split::split_once(left, &DELIMITER);
            let lempty = lrest.is_empty();
            if lchunk == DOUBLE_WILD {
                if (lempty && !right.has_verbatim()) || (!lempty && self.includes(lrest, right)) {
                    return true;
                }
                if right.has_direct_verbatim() {
                    return false;
                }
                right = Split::split_once(right, &DELIMITER).1;
                if right.is_empty() {
                    return false;
                }
            } else {
                let (rchunk, rrest) = Split::split_once(right, &DELIMITER);
                if rchunk.is_empty()
                    || rchunk == DOUBLE_WILD
                    || !self.non_double_wild_chunk_includes(lchunk, rchunk)
                {
                    return false;
                }
                let rempty = rrest.is_empty();
                if lempty {
                    return rempty;
                }
                left = lrest;
                right = rrest;
            }
        }
    }
}

impl LTRIncluder {
    fn non_double_wild_chunk_includes(&self, lchunk: &[u8], rchunk: &[u8]) -> bool {
        if lchunk == rchunk {
            true
        } else if unsafe {
            lchunk.has_direct_verbatim_non_empty() || rchunk.has_direct_verbatim_non_empty()
        } {
            false
        } else if lchunk == b"*" {
            true
        } else if lchunk.contains(&b'$') {
            let mut spleft = lchunk.splitter(STAR_DSL);
            if let Some(rchunk) = rchunk.strip_prefix(spleft.next().unwrap()) {
                if let Some(mut rchunk) = rchunk.strip_suffix(spleft.next_back().unwrap()) {
                    for needle in spleft {
                        let needle_len = needle.len();
                        if let Some(position) =
                            rchunk.windows(needle_len).position(|right| right == needle)
                        {
                            rchunk = &rchunk[position + needle_len..]
                        } else {
                            return false;
                        }
                    }
                    true
                } else {
                    false
                }
            } else {
                false
            }
        } else {
            false
        }
    }
}
