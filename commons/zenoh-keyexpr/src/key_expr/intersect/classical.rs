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
#[cold]
fn star_dsl_intersect(mut it1: &[u8], mut it2: &[u8]) -> bool {
    fn next(s: &[u8]) -> (u8, &[u8]) {
        (s[0], &s[1..])
    }
    while !it1.is_empty() && !it2.is_empty() {
        let (current1, advanced1) = next(it1);
        let (current2, advanced2) = next(it2);
        match (current1, current2) {
            (b'$', b'$') => {
                if advanced1.len() == 1 || advanced2.len() == 1 {
                    return true;
                }
                if star_dsl_intersect(&advanced1[1..], it2) {
                    return true;
                } else {
                    return star_dsl_intersect(it1, &advanced2[1..]);
                };
            }
            (b'$', _) => {
                if advanced1.len() == 1 {
                    return true;
                }
                if star_dsl_intersect(&advanced1[1..], it2) {
                    return true;
                }
                it2 = advanced2;
            }
            (_, b'$') => {
                if advanced2.len() == 1 {
                    return true;
                }
                if star_dsl_intersect(it1, &advanced2[1..]) {
                    return true;
                }
                it1 = advanced1;
            }
            (sub1, sub2) if sub1 == sub2 => {
                it1 = advanced1;
                it2 = advanced2;
            }
            (_, _) => return false,
        }
    }
    it1.is_empty() && it2.is_empty() || it1 == b"$*" || it2 == b"$*"
}

fn chunk_it_intersect<const STAR_DSL: bool>(it1: &[u8], it2: &[u8]) -> bool {
    it1 == b"*" || it2 == b"*" || (STAR_DSL && star_dsl_intersect(it1, it2))
}
#[inline(always)]
fn chunk_intersect<const STAR_DSL: bool>(c1: &[u8], c2: &[u8]) -> bool {
    if c1 == c2 {
        return true;
    }
    if c1.has_direct_verbatim() || c2.has_direct_verbatim() {
        return false;
    }
    chunk_it_intersect::<STAR_DSL>(c1, c2)
}

#[inline(always)]
fn next(s: &[u8]) -> (&[u8], &[u8]) {
    match s.iter().position(|c| *c == b'/') {
        Some(i) => (&s[..i], &s[(i + 1)..]),
        None => (s, b""),
    }
}

fn it_intersect<const STAR_DSL: bool>(mut it1: &[u8], mut it2: &[u8]) -> bool {
    while !it1.is_empty() && !it2.is_empty() {
        let (current1, advanced1) = next(it1);
        let (current2, advanced2) = next(it2);
        match (current1, current2) {
            (b"**", _) => {
                if advanced1.is_empty() {
                    return !it2.has_verbatim();
                }
                return (!unsafe { current2.has_direct_verbatim_non_empty() }
                    && it_intersect::<STAR_DSL>(it1, advanced2))
                    || it_intersect::<STAR_DSL>(advanced1, it2);
            }
            (_, b"**") => {
                if advanced2.is_empty() {
                    return !it1.has_verbatim();
                }
                return (!unsafe { current1.has_direct_verbatim_non_empty() }
                    && it_intersect::<STAR_DSL>(advanced1, it2))
                    || it_intersect::<STAR_DSL>(it1, advanced2);
            }
            (sub1, sub2) if chunk_intersect::<STAR_DSL>(sub1, sub2) => {
                it1 = advanced1;
                it2 = advanced2;
            }
            (_, _) => return false,
        }
    }
    (it1.is_empty() || it1 == b"**") && (it2.is_empty() || it2 == b"**")
}
/// Returns `true` if the given key expressions intersect.
///
/// I.e. if it exists a resource key (with no wildcards) that matches
/// both given key expressions.
#[inline(always)]
pub fn intersect<const STAR_DSL: bool>(s1: &[u8], s2: &[u8]) -> bool {
    it_intersect::<STAR_DSL>(s1, s2)
}

use super::{restriction::NoSubWilds, Intersector, MayHaveVerbatim};

pub struct ClassicIntersector;
impl Intersector<NoSubWilds<&[u8]>, NoSubWilds<&[u8]>> for ClassicIntersector {
    fn intersect(&self, left: NoSubWilds<&[u8]>, right: NoSubWilds<&[u8]>) -> bool {
        intersect::<false>(left.0, right.0)
    }
}

impl Intersector<&[u8], &[u8]> for ClassicIntersector {
    fn intersect(&self, left: &[u8], right: &[u8]) -> bool {
        intersect::<true>(left, right)
    }
}
