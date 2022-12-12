use zenoh_protocol_core::key_expr::fuzzer::KeyExprFuzzer;

use super::{
    keyed_set_tree::{KeyedSetProvider, VecSetProvider},
    *,
};
use std::{
    collections::HashMap,
    convert::{TryFrom, TryInto},
    fmt::Debug,
    ops::Deref,
};

fn insert<'a, K: TryInto<&'a keyexpr>, V: Clone + PartialEq + Debug + 'static>(
    ketree: &mut KeyExprTree<V, KeyedSetProvider>,
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
    ketree: &mut KeyExprTree<V, VecSetProvider>,
    map: &mut HashMap<OwnedKeyExpr, Option<V>>,
    key: K,
    value: V,
) where
    <K as TryInto<&'a keyexpr>>::Error: Debug,
{
    let key = dbg!(key.try_into().unwrap());
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

fn into_ke(s: &str) -> &keyexpr {
    keyexpr::new(s).unwrap()
}

fn test_keyset<K: Deref<Target = keyexpr>>(keys: &[K]) {
    let mut tree = KeyExprTree::new();
    let mut map = HashMap::new();
    for (v, k) in keys.iter().map(|k| k.deref()).enumerate() {
        insert(&mut tree, &mut map, k, v);
    }
    for node in tree.tree_iter() {
        assert_eq!(node.weight(), map.get(&node.keyexpr()).unwrap().as_ref());
    }
    for target in keys {
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
            assert_eq!(expected.remove(&ke).unwrap().as_ref(), weight)
        }
        for node in tree.intersecting_nodes_mut(target) {
            let ke = node.keyexpr();
            let weight = node.weight();
            assert_eq!(exclone.remove(&ke).unwrap().as_ref(), weight)
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
        exclone = expected.clone();
        for node in tree.included_nodes(target) {
            let ke = node.keyexpr();
            let weight = node.weight();
            assert_eq!(expected.remove(&ke).unwrap().as_ref(), weight)
        }
        for node in tree.included_nodes_mut(target) {
            let ke = node.keyexpr();
            let weight = node.weight();
            assert_eq!(exclone.remove(&ke).unwrap().as_ref(), weight)
        }
        assert!(
            expected.is_empty(),
            "MISSING INCLUDES FOR {}: {:?}",
            target.deref(),
            &expected
        );
        assert!(
            exclone.is_empty(),
            "MISSING MUTABLE INTERSECTS FOR {}: {:?}",
            target.deref(),
            &exclone
        );
        println!("{} OK", target.deref());
    }
}

fn test_keyset_vec<K: Deref<Target = keyexpr>>(keys: &[K]) {
    let mut tree = KeyExprTree::new();
    let mut map = HashMap::new();
    for (v, k) in keys.iter().map(|k| k.deref()).enumerate() {
        insert_vecset(&mut tree, &mut map, k, v);
    }
    for node in tree.tree_iter() {
        assert_eq!(node.weight(), map.get(&node.keyexpr()).unwrap().as_ref());
    }
    for target in keys {
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
            assert_eq!(expected.remove(&ke).unwrap().as_ref(), weight)
        }
        for node in tree.intersecting_nodes_mut(target) {
            let ke = node.keyexpr();
            let weight = node.weight();
            assert_eq!(exclone.remove(&ke).unwrap().as_ref(), weight)
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
        exclone = expected.clone();
        for node in tree.included_nodes(target) {
            let ke = node.keyexpr();
            let weight = node.weight();
            assert_eq!(expected.remove(&ke).unwrap().as_ref(), weight)
        }
        for node in tree.included_nodes_mut(target) {
            let ke = node.keyexpr();
            let weight = node.weight();
            assert_eq!(exclone.remove(&ke).unwrap().as_ref(), weight)
        }
        assert!(
            expected.is_empty(),
            "MISSING INCLUDES FOR {}: {:?}",
            target.deref(),
            &expected
        );
        assert!(
            exclone.is_empty(),
            "MISSING MUTABLE INTERSECTS FOR {}: {:?}",
            target.deref(),
            &exclone
        );
        println!("{} OK", target.deref());
    }
}

#[test]
fn keyed_set_tree() {
    let keys: [&keyexpr; 4] = [
        "a/b/**/c/**",
        "a/b/c",
        "a/b/c",
        "a/*/c",
        // "**/c",
        // "**/d",
        // "d/b/c",
        // "**/b/c",
    ]
    .map(into_ke);
    test_keyset(&keys);
    test_keyset_vec(&keys);
}

#[test]
fn fuzz() {
    let fuzzer = KeyExprFuzzer(rand::thread_rng());
    let keys = fuzzer.take(1000).collect::<Vec<_>>();
    test_keyset(&keys);
    test_keyset_vec(&keys);
}
