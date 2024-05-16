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

use alloc::vec::Vec;
use core::{
    convert::{TryFrom, TryInto},
    fmt::Debug,
    ops::Deref,
};
#[cfg(feature = "std")]
use std::collections::HashMap;

#[cfg(not(feature = "std"))]
use hashbrown::HashMap;
use rand::Rng;

use super::{
    impls::{KeyedSetProvider, VecSetProvider},
    *,
};
use crate::fuzzer::KeyExprFuzzer;

fn insert<'a, K: TryInto<&'a keyexpr>, V: Clone + PartialEq + Debug + 'static>(
    ketree: &mut KeBoxTree<V, bool, KeyedSetProvider>,
    map: &mut HashMap<OwnedKeyExpr, Option<V>>,
    key: K,
    value: V,
) where
    <K as TryInto<&'a keyexpr>>::Error: Debug,
{
    let key = key.try_into().unwrap();
    for i in key
        .as_bytes()
        .iter()
        .enumerate()
        .filter_map(|(i, c)| (*c == b'/').then_some(i))
    {
        let subkey = OwnedKeyExpr::try_from(&key[..i]).unwrap();
        map.entry(subkey).or_default();
    }
    assert_eq!(
        ketree.insert(key, value.clone()),
        map.insert(key.into(), Some(value)).flatten()
    )
}

fn insert_vecset<'a, K: TryInto<&'a keyexpr>, V: Clone + PartialEq + Debug + 'static>(
    ketree: &mut KeBoxTree<V, bool, VecSetProvider>,
    map: &mut HashMap<OwnedKeyExpr, Option<V>>,
    key: K,
    value: V,
) where
    <K as TryInto<&'a keyexpr>>::Error: Debug,
{
    let key = key.try_into().unwrap();
    for i in key
        .as_bytes()
        .iter()
        .enumerate()
        .filter_map(|(i, c)| (*c == b'/').then_some(i))
    {
        let subkey = OwnedKeyExpr::try_from(&key[..i]).unwrap();
        map.entry(subkey).or_default();
    }
    assert_eq!(
        ketree.insert(key, value.clone()),
        map.insert(key.into(), Some(value)).flatten()
    )
}

fn insert_kearctree<'a, K: TryInto<&'a keyexpr>, V: Clone + PartialEq + Debug + 'static>(
    (ketree, token): &mut (KeArcTree<V>, DefaultToken),
    map: &mut HashMap<OwnedKeyExpr, Option<V>>,
    key: K,
    value: V,
) where
    <K as TryInto<&'a keyexpr>>::Error: Debug,
{
    let key = key.try_into().unwrap();
    for i in key
        .as_bytes()
        .iter()
        .enumerate()
        .filter_map(|(i, c)| (*c == b'/').then_some(i))
    {
        let subkey = OwnedKeyExpr::try_from(&key[..i]).unwrap();
        map.entry(subkey).or_default();
    }
    assert_eq!(
        ketree.insert(token, key, value.clone()),
        map.insert(key.into(), Some(value)).flatten()
    )
}

fn into_ke(s: &str) -> &keyexpr {
    keyexpr::new(s).unwrap()
}

fn test_keyset<K: Deref<Target = keyexpr> + Debug>(keys: &[K]) {
    let mut tree = KeBoxTree::new();
    let mut map = HashMap::new();
    for (v, k) in keys.iter().map(|k| k.deref()).enumerate() {
        insert(&mut tree, &mut map, k, v);
    }
    for node in tree.tree_iter() {
        assert_eq!(node.weight(), map.get(&node.keyexpr()).unwrap().as_ref());
    }
    for target in keys {
        let target = target.deref();
        let mut expected = HashMap::new();
        for (k, v) in &map {
            if target.intersects(k) {
                assert!(expected.insert(k, v).is_none());
            }
        }
        let mut exclone = expected.clone();
        for node in tree.intersecting_nodes(target) {
            let ke = node.keyexpr();
            let weight = node.weight();
            assert_eq!(
                expected
                    .remove(&ke)
                    .unwrap_or_else(|| panic!("Couldn't find {ke} in {target}'s expected output"))
                    .as_ref(),
                weight
            )
        }
        for node in tree.intersecting_nodes_mut(target) {
            let ke = node.keyexpr();
            let weight = node.weight();
            assert_eq!(
                exclone
                    .remove(&ke)
                    .unwrap_or_else(|| panic!("Couldn't find {ke} in {target}'s expected output"))
                    .as_ref(),
                weight
            )
        }
        assert!(
            expected.is_empty(),
            "MISSING INTERSECTS FOR {}: {:?}",
            target.deref(),
            &expected
        );
        assert!(
            exclone.is_empty(),
            "MISSING MUTABLE INTERSECTS FOR {}: {:?}",
            target.deref(),
            &exclone
        );
        for (k, v) in &map {
            if target.includes(k) {
                assert!(expected.insert(k, v).is_none());
            }
        }
        exclone.clone_from(&expected);
        for node in tree.included_nodes(target) {
            let ke = node.keyexpr();
            let weight = node.weight();
            assert_eq!(
                expected
                    .remove(&ke)
                    .unwrap_or_else(|| panic!("Couldn't find {ke} in {target}'s expected output"))
                    .as_ref(),
                weight
            )
        }
        for node in tree.included_nodes_mut(target) {
            let ke = node.keyexpr();
            let weight = node.weight();
            assert_eq!(
                exclone
                    .remove(&ke)
                    .unwrap_or_else(|| panic!("Couldn't find {ke} in {target}'s expected output"))
                    .as_ref(),
                weight
            )
        }
        assert!(
            expected.is_empty(),
            "MISSING INCLUDES FOR {}: {:?}",
            target.deref(),
            &expected
        );
        assert!(
            exclone.is_empty(),
            "MISSING MUTABLE INCLUDES FOR {}: {:?}",
            target.deref(),
            &exclone
        );
        for (k, v) in &map {
            if k.includes(target) {
                assert!(expected.insert(k, v).is_none());
            }
        }
        exclone.clone_from(&expected);
        for node in tree.nodes_including(dbg!(target)) {
            let ke = node.keyexpr();
            let weight = node.weight();
            assert_eq!(
                expected
                    .remove(dbg!(&ke))
                    .unwrap_or_else(|| panic!("Couldn't find {ke} in {target}'s expected output"))
                    .as_ref(),
                weight
            )
        }
        for node in tree.nodes_including_mut(target) {
            let ke = node.keyexpr();
            let weight = node.weight();
            assert_eq!(
                exclone
                    .remove(&ke)
                    .unwrap_or_else(|| panic!("Couldn't find {ke} in {target}'s expected output"))
                    .as_ref(),
                weight
            )
        }
        assert!(
            expected.is_empty(),
            "MISSING INCLUDES FOR {}: {:?}",
            target.deref(),
            &expected
        );
        assert!(
            exclone.is_empty(),
            "MISSING MUTABLE INCLUDES FOR {}: {:?}",
            target.deref(),
            &exclone
        );
        #[cfg(feature = "std")]
        {
            println!("{} OK", target.deref());
        }
    }
}

fn test_keyset_vec<K: Deref<Target = keyexpr>>(keys: &[K]) {
    let mut tree = KeBoxTree::default();
    let mut map = HashMap::new();
    for (v, k) in keys.iter().map(|k| k.deref()).enumerate() {
        insert_vecset(&mut tree, &mut map, k, v);
    }
    for node in tree.tree_iter() {
        assert_eq!(node.weight(), map.get(&node.keyexpr()).unwrap().as_ref());
    }
    for target in keys {
        let target = target.deref();
        let mut expected = HashMap::new();
        for (k, v) in &map {
            if target.intersects(k) {
                assert!(expected.insert(k, v).is_none());
            }
        }
        let mut exclone = expected.clone();
        for node in tree.intersecting_nodes(target) {
            let ke = node.keyexpr();
            let weight = node.weight();
            assert_eq!(
                expected
                    .remove(&ke)
                    .unwrap_or_else(|| panic!("Couldn't find {ke} in {target}'s expected output"))
                    .as_ref(),
                weight
            )
        }
        for node in tree.intersecting_nodes_mut(target) {
            let ke = node.keyexpr();
            let weight = node.weight();
            assert_eq!(
                exclone
                    .remove(&ke)
                    .unwrap_or_else(|| panic!("Couldn't find {ke} in {target}'s expected output"))
                    .as_ref(),
                weight
            )
        }
        assert!(
            expected.is_empty(),
            "MISSING INTERSECTS FOR {}: {:?}",
            target.deref(),
            &expected
        );
        assert!(
            exclone.is_empty(),
            "MISSING MUTABLE INTERSECTS FOR {}: {:?}",
            target.deref(),
            &exclone
        );
        for (k, v) in &map {
            if target.includes(k) {
                assert!(expected.insert(k, v).is_none());
            }
        }
        exclone.clone_from(&expected);
        for node in tree.included_nodes(target) {
            let ke = node.keyexpr();
            let weight = node.weight();
            assert_eq!(
                expected
                    .remove(&ke)
                    .unwrap_or_else(|| panic!("Couldn't find {ke} in {target}'s expected output"))
                    .as_ref(),
                weight
            )
        }
        for node in tree.included_nodes_mut(target) {
            let ke = node.keyexpr();
            let weight = node.weight();
            assert_eq!(
                exclone
                    .remove(&ke)
                    .unwrap_or_else(|| panic!("Couldn't find {ke} in {target}'s expected output"))
                    .as_ref(),
                weight
            )
        }
        assert!(
            expected.is_empty(),
            "MISSING INCLUDES FOR {}: {:?}",
            target.deref(),
            &expected
        );
        assert!(
            exclone.is_empty(),
            "MISSING MUTABLE INCLUDES FOR {}: {:?}",
            target.deref(),
            &exclone
        );
        #[cfg(feature = "std")]
        {
            println!("{} OK", target.deref());
        }
    }
}

fn test_keyarctree<K: Deref<Target = keyexpr>>(keys: &[K]) {
    let mut tree = KeArcTree::new().unwrap();
    let mut map = HashMap::new();
    for (v, k) in keys.iter().map(|k| k.deref()).enumerate() {
        insert_kearctree(&mut tree, &mut map, k, v);
    }
    for node in tree.0.tree_iter(&tree.1) {
        assert_eq!(node.weight(), map.get(&node.keyexpr()).unwrap().as_ref());
    }
    for target in keys {
        let target = target.deref();
        let mut expected = HashMap::new();
        for (k, v) in &map {
            if target.intersects(k) {
                assert!(expected.insert(k, v).is_none());
            }
        }
        for node in tree.0.intersecting_nodes(&tree.1, target) {
            let ke = node.keyexpr();
            let weight = node.weight();
            assert_eq!(
                expected
                    .remove(&ke)
                    .unwrap_or_else(|| panic!("Couldn't find {ke} in {target}'s expected output"))
                    .as_ref(),
                weight
            )
        }
        assert!(
            expected.is_empty(),
            "MISSING INTERSECTS FOR {}: {:?}",
            target.deref(),
            &expected
        );
        for (k, v) in &map {
            if target.includes(k) {
                assert!(expected.insert(k, v).is_none());
            }
        }
        for node in tree.0.included_nodes(&tree.1, target) {
            let ke = node.keyexpr();
            let weight = node.weight();
            assert_eq!(
                expected
                    .remove(&ke)
                    .unwrap_or_else(|| panic!("Couldn't find {ke} in {target}'s expected output"))
                    .as_ref(),
                weight
            )
        }
        assert!(
            expected.is_empty(),
            "MISSING INCLUDES FOR {}: {:?}",
            target.deref(),
            &expected
        );
        #[cfg(feature = "std")]
        {
            println!("{} OK", target.deref());
        }
    }
}

#[test]
fn keyed_set_tree() {
    let keys: [&keyexpr; 16] = [
        "a/b/**/c/**",
        "a/b/c",
        "a/b/c",
        "a/*/c",
        "**/c",
        "**/d",
        "d/b/c",
        "**/b/c",
        "**/@c/**",
        "**",
        "**/@c",
        "@c/**",
        "@c/a",
        "a/@c",
        "b/a$*a/b/bb",
        "**/b$*/bb",
    ]
    .map(into_ke);
    test_keyset(&keys);
    test_keyset_vec(&keys);
    test_keyarctree(&keys)
}

#[test]
fn fuzz() {
    let fuzzer = KeyExprFuzzer(rand::thread_rng());
    let keys = fuzzer.take(400).collect::<Vec<_>>();
    test_keyset(&keys);
    test_keyset_vec(&keys);
    test_keyarctree(&keys)
}

#[test]
fn pruning() {
    let mut rng = rand::thread_rng();
    let mut fuzzer = KeyExprFuzzer(rand::thread_rng());
    let mut set: KeBoxTree<i32> = KeBoxTree::new();
    let dist = rand::distributions::Uniform::new(0, 3);
    while !set
        .tree_iter()
        .any(|node| node.weight().is_none() && node.children().is_empty())
    {
        for key in fuzzer.by_ref().take(100) {
            let node = set.node_mut_or_create(&key);
            let sample = rng.sample(dist);
            if sample != 0 {
                node.insert_weight(sample);
            }
        }
    }
    let expected = set
        .tree_iter()
        .filter_map(|node| node.weight().map(|w| (node.keyexpr(), *w)))
        .collect::<HashMap<_, _>>();
    set.prune();
    for node in set.tree_iter() {
        assert!(node.weight().is_some() || !node.children().is_empty())
    }
    for (k, v) in expected {
        assert_eq!(*set.weight_at(&k).unwrap(), v)
    }
}
