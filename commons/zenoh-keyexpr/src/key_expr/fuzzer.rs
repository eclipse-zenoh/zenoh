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
use super::OwnedKeyExpr;

fn random_chunk(rng: &'_ mut impl rand::Rng) -> impl Iterator<Item = u8> + '_ {
    let n = rng.gen_range(1..3);
    rng.gen_bool(0.05)
        .then_some(b'@')
        .into_iter()
        .chain((0..n).map(move |_| rng.sample(rand::distributions::Uniform::from(b'a'..b'c'))))
}

fn make(ke: &mut Vec<u8>, rng: &mut impl rand::Rng) {
    let mut iters = 0;
    loop {
        let n = rng.sample(rand::distributions::Uniform::<u8>::from(0..=255));
        match n {
            0..=15 => ke.extend(b"/**"),
            16..=31 => ke.extend(b"/*"),
            32..=47 => {
                if !ke.is_empty() && !ke.ends_with(b"*") {
                    ke.extend(b"$*")
                } else {
                    continue;
                }
            }
            48.. => {
                if n >= 128 || ke.ends_with(b"**") || ke.ends_with(b"/*") {
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

pub struct KeyExprFuzzer<Rng: rand::Rng>(pub Rng);
impl<Rng: rand::Rng> Iterator for KeyExprFuzzer<Rng> {
    type Item = OwnedKeyExpr;
    fn next(&mut self) -> Option<Self::Item> {
        let mut next = Vec::new();
        make(&mut next, &mut self.0);
        let mut next = String::from_utf8(next).unwrap();
        if let Some(n) = next.strip_prefix('/').map(ToOwned::to_owned) {
            next = n
        }
        Some(OwnedKeyExpr::autocanonize(next).unwrap())
    }
}
