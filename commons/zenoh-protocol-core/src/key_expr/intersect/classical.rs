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

fn chunk_it_intersect(it1: &[u8], it2: &[u8]) -> bool {
    it1 == it2 || it1 == b"*" || it2 == b"*"
}
#[inline(always)]
fn chunk_intersect(c1: &[u8], c2: &[u8]) -> bool {
    if c1 == c2 {
        return true;
    }
    if c1.is_empty() != c2.is_empty() {
        return false;
    }
    chunk_it_intersect(c1, c2)
}

fn next(s: &[u8]) -> (&[u8], &[u8]) {
    match s.iter().position(|c| *c == b'/') {
        Some(i) => (&s[..i], &s[(i + 1)..]),
        None => (s, b""),
    }
}
fn it_intersect<'a>(mut it1: &'a [u8], mut it2: &'a [u8]) -> bool {
    while !it1.is_empty() && !it2.is_empty() {
        let (current1, advanced1) = next(it1);
        let (current2, advanced2) = next(it2);
        match (current1, current2) {
            (b"**", _) => {
                return advanced1.is_empty()
                    || it_intersect(advanced1, it2)
                    || it_intersect(it1, advanced2);
            }
            (_, b"**") => {
                return advanced2.is_empty()
                    || it_intersect(it1, advanced2)
                    || it_intersect(advanced1, it2);
            }
            (sub1, sub2) if chunk_intersect(sub1, sub2) => {
                it1 = advanced1;
                it2 = advanced2;
            }
            (_, _) => return false,
        }
    }
    (it1.is_empty() || it1 == b"**") && (it2.is_empty() || it2 == b"**")
}
/// Retruns `true` if the given key expressions intersect.
///
/// I.e. if it exists a resource key (with no wildcards) that matches
/// both given key expressions.
#[inline(always)]
pub fn intersect<'a>(s1: &'a [u8], s2: &'a [u8]) -> bool {
    it_intersect(s1, s2)
}

use super::restiction::NoSubWilds;
use super::Intersector;

pub struct ClassicIntersector;
impl Intersector<NoSubWilds<&[u8]>, NoSubWilds<&[u8]>> for ClassicIntersector {
    fn intersect(&self, left: NoSubWilds<&[u8]>, right: NoSubWilds<&[u8]>) -> bool {
        // crate::wire_expr::intersect(left, right)
        intersect(left.0, right.0)
    }
}

impl Intersector<&[u8], &[u8]> for ClassicIntersector {
    fn intersect(&self, left: &[u8], right: &[u8]) -> bool {
        crate::wire_expr::intersect(unsafe { std::str::from_utf8_unchecked(left) }, unsafe {
            std::str::from_utf8_unchecked(right)
        })
    }
}
