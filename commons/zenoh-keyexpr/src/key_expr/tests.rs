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

use std::{convert::TryInto, fmt::Debug};

use crate::key_expr::{fuzzer, intersect::*, keyexpr};

type BoxedIntersectors = Vec<Box<dyn for<'a> Intersector<&'a keyexpr, &'a keyexpr> + Send + Sync>>;

lazy_static::lazy_static! {
    static ref INTERSECTORS: BoxedIntersectors =
    vec![
        // Box::new(LeftToRightIntersector(LTRChunkIntersector)),
        // Box::new(MiddleOutIntersector(LTRChunkIntersector)),
        Box::new(ClassicIntersector)
    ];
}

fn intersect<'a, A: TryInto<&'a keyexpr>, B: TryInto<&'a keyexpr>>(l: A, r: B) -> bool
where
    <A as TryInto<&'a keyexpr>>::Error: Debug,
    <B as TryInto<&'a keyexpr>>::Error: Debug,
{
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
fn intersections() {
    assert!(intersect("a", "a"));
    assert!(intersect("a/b", "a/b"));
    assert!(intersect("*", "abc"));
    assert!(intersect("*", "xxx"));
    assert!(intersect("ab$*", "abcd"));
    assert!(intersect("ab$*d", "abcd"));
    assert!(intersect("ab$*", "ab"));
    assert!(!intersect("ab/*", "ab"));
    assert!(intersect("a/*/c/*/e", "a/b/c/d/e"));
    assert!(intersect("a/$*b/c/$*d/e", "a/xb/c/xd/e"));
    assert!(!intersect("a/*/c/*/e", "a/c/e"));
    assert!(!intersect("a/*/c/*/e", "a/b/c/d/x/e"));
    assert!(!intersect("ab$*cd", "abxxcxxd"));
    assert!(intersect("ab$*cd", "abxxcxxcd"));
    assert!(!intersect("ab$*cd", "abxxcxxcdx"));
    assert!(intersect("**", "abc"));
    assert!(intersect("**", "a/b/c"));
    assert!(intersect("ab/**", "ab"));
    assert!(intersect("**/xyz", "a/b/xyz/d/e/f/xyz"));
    assert!(!intersect("**/xyz$*xyz", "a/b/xyz/d/e/f/xyz"));
    assert!(intersect("**/xyz$*xyz", "a/b/xyzdefxyz"));
    assert!(intersect("a/**/c/**/e", "a/b/b/b/c/d/d/d/e"));
    assert!(intersect("a/**/c/**/e", "a/c/e"));
    assert!(intersect("a/**/c/*/e/*", "a/b/b/b/c/d/d/c/d/e/f"));
    assert!(!intersect("a/**/c/*/e/*", "a/b/b/b/c/d/d/c/d/d/e/f"));
    assert!(!intersect("ab$*cd", "abxxcxxcdx"));
    assert!(intersect("x/abc", "x/abc"));
    assert!(!intersect("x/abc", "abc"));
    assert!(intersect("x/*", "x/abc"));
    assert!(!intersect("x/*", "abc"));
    assert!(!intersect("*", "x/abc"));
    assert!(intersect("x/*", "x/abc$*"));
    assert!(intersect("x/$*abc", "x/abc$*"));
    assert!(intersect("x/a$*", "x/abc$*"));
    assert!(intersect("x/a$*de", "x/abc$*de"));
    assert!(intersect("x/a$*d$*e", "x/a$*e"));
    assert!(intersect("x/a$*d$*e", "x/a$*c$*e"));
    assert!(intersect("x/a$*d$*e", "x/ade"));
    assert!(!intersect("x/c$*", "x/abc$*"));
    assert!(!intersect("x/$*d", "x/$*e"));

    assert!(intersect("@a", "@a"));
    assert!(!intersect("@a", "@ab"));
    assert!(!intersect("@a", "@a/b"));
    assert!(!intersect("@a", "@a/*"));
    assert!(!intersect("@a", "@a/*/**"));
    assert!(!intersect("@a", "@a$*/**"));
    assert!(intersect("@a", "@a/**"));
    assert!(!intersect("**/xyz$*xyz", "@a/b/xyzdefxyz"));
    assert!(intersect("@a/**/c/**/e", "@a/b/b/b/c/d/d/d/e"));
    assert!(!intersect("@a/**/c/**/e", "@a/@b/b/b/c/d/d/d/e"));
    assert!(intersect("@a/**/@c/**/e", "@a/b/b/b/@c/d/d/d/e"));
    assert!(intersect("@a/**/e", "@a/b/b/d/d/d/e"));
    assert!(intersect("@a/**/e", "@a/b/b/b/d/d/d/e"));
    assert!(intersect("@a/**/e", "@a/b/b/c/d/d/d/e"));
    assert!(!intersect("@a/**/e", "@a/b/b/@c/b/d/d/d/e"));
    assert!(!intersect("@a/*", "@a/@b"));
    assert!(!intersect("@a/**", "@a/@b"));
    assert!(intersect("@a/**/@b", "@a/@b"));
    assert!(intersect("@a/@b/**", "@a/@b"));
    assert!(intersect("@a/**/@c/**/@b", "@a/**/@c/@b"));
    assert!(intersect("@a/**/@c/**/@b", "@a/@c/**/@b"));
    assert!(intersect("@a/**/@c/@b", "@a/@c/**/@b"));
    assert!(!intersect("@a/**/@b", "@a/**/@c/**/@b"));
}

fn includes<
    'a,
    A: TryInto<&'a keyexpr, Error = zenoh_result::Error>,
    B: TryInto<&'a keyexpr, Error = zenoh_result::Error>,
>(
    l: A,
    r: B,
) -> bool {
    let left = l.try_into().unwrap();
    let right = r.try_into().unwrap();
    dbg!(left, right);
    dbg!(left.includes(right))
}

#[test]
fn inclusions() {
    assert!(includes("a", "a"));
    assert!(includes("a/b", "a/b"));
    assert!(includes("*", "abc"));
    assert!(includes("*", "xxx"));
    assert!(includes("ab$*", "abcd"));
    assert!(includes("ab$*d", "abcd"));
    assert!(includes("ab$*", "ab"));
    assert!(!includes("ab/*", "ab"));
    assert!(includes("a/*/c/*/e", "a/b/c/d/e"));
    assert!(includes("a/$*b/c/$*d/e", "a/xb/c/xd/e"));
    assert!(!includes("a/*/c/*/e", "a/c/e"));
    assert!(!includes("a/*/c/*/e", "a/b/c/d/x/e"));
    assert!(!includes("ab$*cd", "abxxcxxd"));
    assert!(includes("ab$*c$*d", "abxxcxxd"));
    assert!(includes("ab$*cd", "abxxcxxcd"));
    assert!(!includes("ab$*cd", "abxxcxxcdx"));
    assert!(includes("**", "abc"));
    assert!(includes("**", "a/b/c"));
    assert!(includes("ab/**", "ab"));
    assert!(includes("**/xyz", "a/b/xyz/d/e/f/xyz"));
    assert!(!includes("**/xyz$*xyz", "a/b/xyz/d/e/f/xyz"));
    assert!(includes("**/xyz$*xyz", "a/b/xyzdefxyz"));
    assert!(includes("a/**/c/**/e", "a/b/b/b/c/d/d/d/e"));
    assert!(includes("a/**/c/**/e", "a/c/e"));
    assert!(includes("a/**/c/*/e/*", "a/b/b/b/c/d/d/c/d/e/f"));
    assert!(!includes("a/**/c/*/e/*", "a/b/b/b/c/d/d/c/d/d/e/f"));
    assert!(!includes("ab$*cd", "abxxcxxcdx"));
    assert!(includes("x/abc", "x/abc"));
    assert!(!includes("x/abc", "abc"));
    assert!(includes("x/*", "x/abc"));
    assert!(!includes("x/*", "abc"));
    assert!(!includes("*", "x/abc"));
    assert!(includes("x/*", "x/abc$*"));
    assert!(!includes("x/$*abc", "x/abc$*"));
    assert!(includes("x/a$*", "x/abc$*"));
    assert!(!includes("x/abc$*", "x/a$*"));
    assert!(includes("x/a$*de", "x/abc$*de"));
    assert!(includes("x/a$*e", "x/a$*d$*e"));
    assert!(!includes("x/a$*d$*e", "x/a$*e"));
    assert!(!includes("x/a$*d$*e", "x/a$*c$*e"));
    assert!(includes("x/a$*d$*e", "x/ade"));
    assert!(!includes("x/c$*", "x/abc$*"));
    assert!(includes("x/$*c$*", "x/abc$*"));
    assert!(!includes("x/$*d", "x/$*e"));

    assert!(includes("@a", "@a"));
    assert!(!includes("@a", "@ab"));
    assert!(!includes("@a", "@a/b"));
    assert!(!includes("@a", "@a/*"));
    assert!(!includes("@a", "@a/*/**"));
    assert!(!includes("@a$*/**", "@a"));
    assert!(!includes("@a", "@a/**"));
    assert!(includes("@a/**", "@a"));
    assert!(!includes("**/xyz$*xyz", "@a/b/xyzdefxyz"));
    assert!(includes("@a/**/c/**/e", "@a/b/b/b/c/d/d/d/e"));
    assert!(!includes("@a/*", "@a/@b"));
    assert!(!includes("@a/**", "@a/@b"));
    assert!(includes("@a/**/@b", "@a/@b"));
    assert!(includes("@a/@b/**", "@a/@b"));
}

#[test]
fn fuzz() {
    const FUZZ_ROUNDS: usize = 100_000;
    let rng = rand::thread_rng();
    let mut fuzzer = fuzzer::KeyExprFuzzer(rng);
    let mut ke1 = fuzzer.next().unwrap();
    for ke2 in fuzzer.take(FUZZ_ROUNDS) {
        {
            intersect(&*ke1, &*ke2);
        }
        ke1 = ke2;
    }
}
