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

use std::convert::TryInto;

use super::{
    intersect::{Intersector, DEFAULT_INTERSECTOR},
    keyexpr,
};

type BoxedIntersectors = Vec<
    Box<
        dyn for<'a> crate::key_expr::intersect::Intersector<&'a keyexpr, &'a keyexpr> + Send + Sync,
    >,
>;
use crate::key_expr::intersect::*;
lazy_static::lazy_static! {
    static ref INTERSECTORS: BoxedIntersectors =
    vec![
        Box::new(LeftToRightIntersector(LTRChunkIntersector)),
        Box::new(MiddleOutIntersector(LTRChunkIntersector)),
    ];
}

fn intersect<
    'a,
    A: TryInto<&'a keyexpr, Error = zenoh_core::Error>,
    B: TryInto<&'a keyexpr, Error = zenoh_core::Error>,
>(
    l: A,
    r: B,
) -> bool {
    let left = l.try_into().unwrap();
    let right = r.try_into().unwrap();
    let response = DEFAULT_INTERSECTOR.intersect(left, right);
    for intersector in INTERSECTORS.iter() {
        if intersector.intersect(left, right) != response {
            panic!("DEFAULT_INTERSECTOR ({}) and INTERSECTORS[{:?}] disagreed on intersection between `{}` and `{}`", response, INTERSECTORS.iter().map(|i| i.intersect(left, right)).collect::<Vec<_>>(), left.as_ref(), right.as_ref())
        }
    }
    response
}

#[test]
fn key_expr_test() {
    assert!(intersect("a", "a"));
    assert!(intersect("a/b", "a/b"));
    assert!(intersect("*", "abc"));
    assert!(intersect("*", "xxx"));
    assert!(intersect("ab*", "abcd"));
    assert!(intersect("ab*d", "abcd"));
    assert!(intersect("ab*", "ab"));
    assert!(!intersect("ab/*", "ab"));
    assert!(intersect("a/*/c/*/e", "a/b/c/d/e"));
    assert!(intersect("a/*b/c/*d/e", "a/xb/c/xd/e"));
    assert!(!intersect("a/*/c/*/e", "a/c/e"));
    assert!(!intersect("a/*/c/*/e", "a/b/c/d/x/e"));
    assert!(!intersect("ab*cd", "abxxcxxd"));
    assert!(intersect("ab*cd", "abxxcxxcd"));
    assert!(!intersect("ab*cd", "abxxcxxcdx"));
    assert!(intersect("**", "abc"));
    assert!(intersect("**", "a/b/c"));
    assert!(intersect("ab/**", "ab"));
    assert!(intersect("**/xyz", "a/b/xyz/d/e/f/xyz"));
    assert!(!intersect("**/xyz*xyz", "a/b/xyz/d/e/f/xyz"));
    assert!(intersect("**/xyz*xyz", "a/b/xyzdefxyz"));
    assert!(intersect("a/**/c/**/e", "a/b/b/b/c/d/d/d/e"));
    assert!(intersect("a/**/c/**/e", "a/c/e"));
    assert!(intersect("a/**/c/*/e/*", "a/b/b/b/c/d/d/c/d/e/f"));
    assert!(!intersect("a/**/c/*/e/*", "a/b/b/b/c/d/d/c/d/d/e/f"));
    assert!(!intersect("ab*cd", "abxxcxxcdx"));
    assert!(intersect("x/abc", "x/abc"));
    assert!(!intersect("x/abc", "abc"));
    assert!(intersect("x/*", "x/abc"));
    assert!(!intersect("x/*", "abc"));
    assert!(!intersect("*", "x/abc"));
    assert!(intersect("x/*", "x/abc*"));
    assert!(intersect("x/*abc", "x/abc*"));
    assert!(intersect("x/a*", "x/abc*"));
    assert!(intersect("x/a*de", "x/abc*de"));
    assert!(intersect("x/a*d*e", "x/a*e"));
    assert!(intersect("x/a*d*e", "x/a*c*e"));
    assert!(intersect("x/a*d*e", "x/ade"));
    assert!(!intersect("x/c*", "x/abc*"));
    assert!(!intersect("x/*d", "x/*e"));
}

fn random_chunk(rng: &'_ mut impl rand::Rng) -> impl Iterator<Item = u8> + '_ {
    let n = rng.gen_range(1..3);
    (0..n).map(move |_| rng.sample(rand::distributions::Uniform::from(b'a'..b'c')))
}
pub fn make(ke: &mut Vec<u8>, rng: &mut impl rand::Rng) {
    let mut iters = 0;
    loop {
        let n = rng.sample(rand::distributions::Uniform::<u8>::from(0..=255));
        match n {
            0..=15 => ke.extend(b"/**"),
            16..=31 => ke.extend(b"/*"),
            32..=47 => {
                if !ke.ends_with(b"*") || ke.is_empty() {
                    ke.extend(b"*")
                } else {
                    continue;
                }
            }
            48.. => {
                if n >= 128 || ke.ends_with(b"**") {
                    ke.push(b'/')
                }
                ke.extend(random_chunk(rng))
            }
        }
        if n % (10 - iters) == 0 {
            return;
        }
        iters += 1;
    }
}

pub struct KeyFuzzer<Rng: rand::Rng>(pub Rng);
impl<Rng: rand::Rng> Iterator for KeyFuzzer<Rng> {
    type Item = String;
    fn next(&mut self) -> Option<Self::Item> {
        let mut next = Vec::new();
        make(&mut next, &mut self.0);
        let mut next = String::from_utf8(next).unwrap();
        if let Some(n) = next.strip_prefix('/') {
            next = n.to_owned()
        }
        Some(next)
    }
}

#[test]
fn fuzz() {
    use crate::key_expr::canon::Canonizable;
    const FUZZ_ROUNDS: usize = 10000;
    let rng = rand::thread_rng();
    let mut fuzzer = KeyFuzzer(rng);
    let mut ke1 = fuzzer.next().unwrap();
    ke1.canonize();
    for mut ke2 in fuzzer.take(FUZZ_ROUNDS) {
        {
            ke2.canonize();
            intersect(ke1.as_str(), ke2.as_str());
        }
        ke1 = ke2;
    }
}
