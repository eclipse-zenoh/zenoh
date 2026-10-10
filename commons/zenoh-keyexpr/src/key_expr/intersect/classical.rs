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

/// A position in each key expression, as the offset of the remaining suffix.
type Positions = (usize, usize);

/// Matches `s1[p1..]` against `s2[p2..]` until the result is known or a `**` lets either side skip
/// a chunk. In that case the positions to try next are pushed on `branches`, and `false` is
/// returned for this path.
fn walk<const STAR_DSL: bool>(
    s1: &[u8],
    s2: &[u8],
    (p1, p2): Positions,
    branches: &mut Vec<Positions>,
) -> bool {
    let (mut it1, mut it2) = (&s1[p1..], &s2[p2..]);
    let pos = |s: &[u8], it: &[u8]| s.len() - it.len();
    while !it1.is_empty() && !it2.is_empty() {
        let (current1, advanced1) = next(it1);
        let (current2, advanced2) = next(it2);
        match (current1, current2) {
            (b"**", _) => {
                if advanced1.is_empty() {
                    return !it2.has_verbatim();
                }
                if !current2.has_direct_verbatim_non_empty() {
                    branches.push((pos(s1, it1), pos(s2, advanced2)));
                }
                branches.push((pos(s1, advanced1), pos(s2, it2)));
                return false;
            }
            (_, b"**") => {
                if advanced2.is_empty() {
                    return !it1.has_verbatim();
                }
                if !current1.has_direct_verbatim_non_empty() {
                    branches.push((pos(s1, advanced1), pos(s2, it2)));
                }
                branches.push((pos(s1, it1), pos(s2, advanced2)));
                return false;
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

fn it_intersect<const STAR_DSL: bool>(s1: &[u8], s2: &[u8]) -> bool {
    // The search branches at each `**`. It runs on an explicit stack rather than by recursion,
    // which overflowed the stack on key expressions with many chunks (#2836), and each pair of
    // positions is walked once, which keeps it from going quadratic with several `**`.
    // Nothing is allocated unless a `**` actually branches.
    let mut branches = Vec::new();
    if walk::<STAR_DSL>(s1, s2, (0, 0), &mut branches) {
        return true;
    }
    let mut walked = BTreeSet::new();
    while let Some(positions) = branches.pop() {
        if walked.insert(positions) && walk::<STAR_DSL>(s1, s2, positions, &mut branches) {
            return true;
        }
    }
    false
}
/// Returns `true` if the given key expressions intersect.
///
/// I.e. if it exists a resource key (with no wildcards) that matches
/// both given key expressions.
#[inline(always)]
pub fn intersect<const STAR_DSL: bool>(s1: &[u8], s2: &[u8]) -> bool {
    it_intersect::<STAR_DSL>(s1, s2)
}

use alloc::{collections::BTreeSet, vec::Vec};

use super::{restriction::NoSubWilds, Intersector, MayHaveVerbatim};

#[derive(Debug)]
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

#[cfg(test)]
mod tests {
    use alloc::{string::String, vec, vec::Vec};

    use super::*;

    /// The recursive implementation `it_intersect` replaced, kept as a reference.
    fn recursive_intersect<const STAR_DSL: bool>(mut it1: &[u8], mut it2: &[u8]) -> bool {
        while !it1.is_empty() && !it2.is_empty() {
            let (current1, advanced1) = next(it1);
            let (current2, advanced2) = next(it2);
            match (current1, current2) {
                (b"**", _) => {
                    if advanced1.is_empty() {
                        return !it2.has_verbatim();
                    }
                    return (!current2.has_direct_verbatim_non_empty()
                        && recursive_intersect::<STAR_DSL>(it1, advanced2))
                        || recursive_intersect::<STAR_DSL>(advanced1, it2);
                }
                (_, b"**") => {
                    if advanced2.is_empty() {
                        return !it1.has_verbatim();
                    }
                    return (!current1.has_direct_verbatim_non_empty()
                        && recursive_intersect::<STAR_DSL>(advanced1, it2))
                        || recursive_intersect::<STAR_DSL>(it1, advanced2);
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

    /// A small deterministic generator, so that failures are reproducible.
    struct XorShift(u64);
    impl XorShift {
        fn below(&mut self, n: usize) -> usize {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 % n as u64) as usize
        }
    }

    fn random_keyexpr(rng: &mut XorShift) -> String {
        const CHUNKS: &[&str] = &["a", "b", "**", "*", "a$*", "$*b", "@v", "ab"];
        let n = 1 + rng.below(6);
        let mut chunks = Vec::new();
        for _ in 0..n {
            let chunk = CHUNKS[rng.below(CHUNKS.len())];
            // `**/**` is not a canonical key expression
            if chunk == "**" && chunks.last() == Some(&"**") {
                continue;
            }
            chunks.push(chunk);
        }
        chunks.join("/")
    }

    #[test]
    fn matches_the_recursive_implementation() {
        let mut rng = XorShift(0x2836);
        for _ in 0..20_000 {
            let (l, r) = (random_keyexpr(&mut rng), random_keyexpr(&mut rng));
            assert_eq!(
                intersect::<true>(l.as_bytes(), r.as_bytes()),
                recursive_intersect::<true>(l.as_bytes(), r.as_bytes()),
                "{l} vs {r}"
            );
            assert_eq!(
                intersect::<false>(l.as_bytes(), r.as_bytes()),
                recursive_intersect::<false>(l.as_bytes(), r.as_bytes()),
                "{l} vs {r}"
            );
        }
    }

    #[test]
    fn long_key_does_not_overflow_the_stack() {
        // On a spawned thread's default stack, a key of this length used to overflow it (#2836).
        std::thread::spawn(|| {
            let key = vec!["a"; 32_000].join("/");
            assert!(!intersect::<true>(b"a/**/b", key.as_bytes()));
            assert!(!intersect::<true>(key.as_bytes(), b"a/**/b"));
            assert!(intersect::<true>(b"a/**/a", key.as_bytes()));
            assert!(!intersect::<true>(b"**/a/**/b", key.as_bytes()));
        })
        .join()
        .unwrap();
    }
}
