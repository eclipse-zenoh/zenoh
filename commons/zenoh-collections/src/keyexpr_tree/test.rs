use super::*;
use std::{
    collections::HashMap,
    convert::{TryFrom, TryInto},
    fmt::Debug,
};

fn insert<'a, K: TryInto<&'a keyexpr>, V: Clone + PartialEq + Debug + 'static>(
    ketree: &mut KeyExprTree<V>,
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

#[test]
fn keyed_set_tree() {
    const KEYS: [&str; 7] = ["a/b/c", "a/b/c", "a/*/c", "**/c", "**/d", "d/b/c", "**/b/c"];
    let mut tree = KeyExprTree::new();
    let mut map = HashMap::new();
    for (v, k) in IntoIterator::into_iter(KEYS).enumerate() {
        insert(&mut tree, &mut map, k, v);
    }
    println!("ALL NODES");
    for node in tree.tree_iter() {
        println!("{}: {:?}", node.keyexpr(), node.weight());
        assert_eq!(node.weight(), map.get(&node.keyexpr()).unwrap().as_ref());
    }
    println!("INTERSECTION a/b/c");
    for node in tree.intersecting_nodes("a/b/c".try_into().unwrap()) {
        println!("{}: {:?}", node.keyexpr(), node.weight())
    }
}
