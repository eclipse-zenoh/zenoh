use super::*;
use std::convert::TryInto;
#[test]
fn keyed_set_tree() {
    let mut tree = KeyExprTree::<u8>::new();
    assert!(tree.insert("a/b/c".try_into().unwrap(), 0).is_none());
    assert_eq!(tree.insert("a/b/c".try_into().unwrap(), 1), Some(0));
    assert!(tree.insert("a/*/c".try_into().unwrap(), 2).is_none());
    assert!(tree.insert("**/c".try_into().unwrap(), 3).is_none());
    assert!(tree.insert("**/d".try_into().unwrap(), 4).is_none());
    assert!(tree.insert("d/b/c".try_into().unwrap(), 5).is_none());
    assert!(tree.insert("**/b/c".try_into().unwrap(), 6).is_none());
    println!("ALL NODES");
    for node in tree.tree_iter() {
        println!("{}: {:?}", node.keyexpr(), node.weight())
    }
    println!("INTERSECTION a/b/c");
    for node in tree.intersecting_nodes("a/b/c".try_into().unwrap()) {
        println!("{}: {:?}", node.keyexpr(), node.weight())
    }
}
