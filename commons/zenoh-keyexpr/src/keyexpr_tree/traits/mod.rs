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

use crate::{keyexpr, OwnedKeyExpr};
use alloc::boxed::Box;
pub mod default_impls;

/// The basic immutable methods of all all KeTrees
pub trait IKeyExprTree<'a, Weight> {
    type Node: IKeyExprTreeNodeMut<Weight>;
    /// Accesses the node at `key` if it exists, treating KEs as if they were litteral keys.
    fn node(&'a self, key: &keyexpr) -> Option<&Self::Node>;
    type TreeIterItem;
    type TreeIter: Iterator<Item = Self::TreeIterItem>;
    /// Iterates over the whole tree, including nodes with no  weight.
    fn tree_iter(&'a self) -> Self::TreeIter;
    type IntersectionItem;
    type Intersection: Iterator<Item = Self::IntersectionItem>;
    /// Iterates over all nodes of the tree whose KE intersects with the given `key`.
    ///
    /// Note that nodes without a `Weight` will also be yielded by the iterator.
    fn intersecting_nodes(&'a self, key: &'a keyexpr) -> Self::Intersection;
    type InclusionItem;
    type Inclusion: Iterator<Item = Self::InclusionItem>;
    /// Iterates over all nodes of the tree whose KE is included by the given `key`.
    ///
    /// Note that nodes without a `Weight` will also be yielded by the iterator.
    fn included_nodes(&'a self, key: &'a keyexpr) -> Self::Inclusion;
}

/// The basic mutable methods of all all KeTrees
pub trait IKeyExprTreeMut<'a, Weight>: IKeyExprTree<'a, Weight> {
    /// Mutably accesses the node at `key` if it exists, treating KEs as if they were litteral keys.
    fn node_mut<'b>(&'b mut self, key: &keyexpr) -> Option<&'b mut Self::Node>;
    /// Clears the weight of the node at `key`.
    ///
    /// To actually destroy nodes, [`IKeyExprTreeMut::prune_where`] or [`IKeyExprTreeExtMut::prune`] must be called.
    fn remove(&mut self, key: &keyexpr) -> Option<Weight>;
    /// Mutably accesses the node at `key`, creating it if necessary.
    fn node_mut_or_create<'b>(&'b mut self, key: &keyexpr) -> &'b mut Self::Node;
    type TreeIterItemMut;
    type TreeIterMut: Iterator<Item = Self::TreeIterItemMut>;
    /// Iterates over the whole tree, including nodes with no  weight.
    fn tree_iter_mut(&'a mut self) -> Self::TreeIterMut;
    type IntersectionItemMut;
    type IntersectionMut: Iterator<Item = Self::IntersectionItemMut>;
    /// Iterates over all nodes of the tree whose KE intersects with the given `key`.
    ///
    /// Note that nodes without a `Weight` will also be yielded by the iterator.
    fn intersecting_nodes_mut(&'a mut self, key: &'a keyexpr) -> Self::IntersectionMut;
    type InclusionItemMut;
    type InclusionMut: Iterator<Item = Self::InclusionItemMut>;
    /// Iterates over all nodes of the tree whose KE is included by the given `key`.
    ///
    /// Note that nodes without a `Weight` will also be yielded by the iterator.
    fn included_nodes_mut(&'a mut self, key: &'a keyexpr) -> Self::InclusionMut;
    /// Prunes node from the tree where the predicate returns `true`.
    ///
    /// Note that nodes that still have children will not be pruned.
    fn prune_where<F: FnMut(&mut Self::Node) -> bool>(&mut self, predicate: F);
}
/// The basic operations of a KeTree when a Token is necessary to acess data.
pub trait ITokenKeyExprTree<'a, Weight, Token> {
    type Node: IKeyExprTreeNode<Weight>;
    type NodeMut: IKeyExprTreeNodeMut<Weight>;
    fn node(&'a self, token: &'a Token, key: &keyexpr) -> Option<Self::Node>;
    fn node_mut(&'a self, token: &'a mut Token, key: &keyexpr) -> Option<Self::NodeMut>;
    fn node_or_create(&'a self, token: &'a mut Token, key: &keyexpr) -> Self::NodeMut;
    type TreeIterItem;
    type TreeIter: Iterator<Item = Self::TreeIterItem>;
    fn tree_iter(&'a self, token: &'a Token) -> Self::TreeIter;
    type TreeIterItemMut;
    type TreeIterMut: Iterator<Item = Self::TreeIterItemMut>;
    fn tree_iter_mut(&'a self, token: &'a mut Token) -> Self::TreeIterMut;
    type IntersectionItem;
    type Intersection: Iterator<Item = Self::IntersectionItem>;
    fn intersecting_nodes(&'a self, token: &'a Token, key: &'a keyexpr) -> Self::Intersection;
    type IntersectionItemMut;
    type IntersectionMut: Iterator<Item = Self::IntersectionItemMut>;
    fn intersecting_nodes_mut(
        &'a self,
        token: &'a mut Token,
        key: &'a keyexpr,
    ) -> Self::IntersectionMut;
    type InclusionItem;
    type Inclusion: Iterator<Item = Self::InclusionItem>;
    fn included_nodes(&'a self, token: &'a Token, key: &'a keyexpr) -> Self::Inclusion;
    type InclusionItemMut;
    type InclusionMut: Iterator<Item = Self::InclusionItemMut>;
    fn included_nodes_mut(&'a self, token: &'a mut Token, key: &'a keyexpr) -> Self::InclusionMut;
    type PruneNode: IKeyExprTreeNodeMut<Weight>;
    fn prune_where<F: FnMut(&mut Self::PruneNode) -> bool>(&self, token: &mut Token, predicate: F);
}

pub trait IKeyExprTreeNode<Weight>: UIKeyExprTreeNode<Weight> {
    fn parent(&self) -> Option<&Self::Parent> {
        unsafe { self.__parent() }
    }
    fn keyexpr(&self) -> OwnedKeyExpr {
        unsafe { self.__keyexpr() }
    }
    fn weight(&self) -> Option<&Weight> {
        unsafe { self.__weight() }
    }
    fn children(&self) -> &Self::Children {
        unsafe { self.__children() }
    }
}

#[doc(hidden)]
pub trait UIKeyExprTreeNode<Weight> {
    type Parent;
    unsafe fn __parent(&self) -> Option<&Self::Parent>;
    unsafe fn __keyexpr(&self) -> OwnedKeyExpr;
    unsafe fn __weight(&self) -> Option<&Weight>;
    type Child;
    type Children: IChildren<Self::Child>;
    unsafe fn __children(&self) -> &Self::Children;
}

pub trait IKeyExprTreeNodeMut<Weight>: IKeyExprTreeNode<Weight> {
    fn parent_mut(&mut self) -> Option<&mut Self::Parent>;
    fn weight_mut(&mut self) -> Option<&mut Weight>;
    fn take_weight(&mut self) -> Option<Weight>;
    fn insert_weight(&mut self, weight: Weight) -> Option<Weight>;
    fn children_mut(&mut self) -> &mut Self::Children;
}
pub trait ITokenKeyExprTreeNode<'a, Weight, Token> {
    type Tokenized: IKeyExprTreeNode<Weight>;
    fn tokenize(&'a self, token: &'a Token) -> Self::Tokenized;
    type TokenizedMut: IKeyExprTreeNodeMut<Weight>;
    fn tokenize_mut(&'a self, token: &'a mut Token) -> Self::TokenizedMut;
}
impl<'a, T: 'a, Weight, Token: 'a> ITokenKeyExprTreeNode<'a, Weight, Token> for T
where
    (&'a T, &'a Token): IKeyExprTreeNode<Weight>,
    (&'a T, &'a mut Token): IKeyExprTreeNodeMut<Weight>,
{
    type Tokenized = (&'a T, &'a Token);
    fn tokenize(&'a self, token: &'a Token) -> Self::Tokenized {
        (self, token)
    }
    type TokenizedMut = (&'a T, &'a mut Token);
    fn tokenize_mut(&'a self, token: &'a mut Token) -> Self::TokenizedMut {
        (self, token)
    }
}

/// Provides a data-structure to store children to the KeTree.
pub trait IChildrenProvider<T> {
    type Assoc: Default + 'static;
}

pub trait IChildren<T: ?Sized> {
    type Node: HasChunk + AsNode<T> + AsNodeMut<T>;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    fn child_at<'a>(&'a self, chunk: &keyexpr) -> Option<&'a Self::Node>;
    fn child_at_mut<'a>(&'a mut self, chunk: &keyexpr) -> Option<&'a mut Self::Node>;
    type Entry<'a, 'b>: IEntry<'a, 'b, T>
    where
        Self: 'a,
        'a: 'b,
        T: 'b;
    fn remove(&mut self, chunk: &keyexpr) -> Option<Self::Node>;
    fn entry<'a, 'b>(&'a mut self, chunk: &'b keyexpr) -> Self::Entry<'a, 'b>
    where
        Self: 'a + 'b,
        T: 'b;
    type Iter<'a>: Iterator<Item = &'a Self::Node>
    where
        Self: 'a,
        Self::Node: 'a;
    fn children<'a>(&'a self) -> Self::Iter<'a>
    where
        Self: 'a;
    type IterMut<'a>: Iterator<Item = &'a mut Self::Node>
    where
        Self: 'a,
        Self::Node: 'a;
    fn children_mut<'a>(&'a mut self) -> Self::IterMut<'a>
    where
        Self: 'a;
    type Intersection<'a>: Iterator<Item = &'a Self::Node>
    where
        Self: 'a,
        Self::Node: 'a;
    fn intersection<'a>(&'a self, key: &'a keyexpr) -> Self::Intersection<'a>;
    type IntersectionMut<'a>: Iterator<Item = &'a mut Self::Node>
    where
        Self: 'a,
        Self::Node: 'a;
    fn intersection_mut<'a>(&'a mut self, key: &'a keyexpr) -> Self::IntersectionMut<'a>;
    type Inclusion<'a>: Iterator<Item = &'a Self::Node>
    where
        Self: 'a,
        Self::Node: 'a;
    fn inclusion<'a>(&'a self, key: &'a keyexpr) -> Self::Inclusion<'a>;
    type InclusionMut<'a>: Iterator<Item = &'a mut Self::Node>
    where
        Self: 'a,
        Self::Node: 'a;
    fn inclusion_mut<'a>(&'a mut self, key: &'a keyexpr) -> Self::InclusionMut<'a>;
    fn filter_out<F: FnMut(&mut T) -> bool>(&mut self, predicate: &mut F);
}

pub trait IEntry<'a, 'b, T: ?Sized> {
    fn get_or_insert_with<F: FnOnce(&'b keyexpr) -> T>(self, f: F) -> &'a mut T;
}

pub trait HasChunk {
    fn chunk(&self) -> &keyexpr;
}

pub trait AsNode<T: ?Sized> {
    fn as_node(&self) -> &T;
}
pub trait AsNodeMut<T: ?Sized>: AsNode<T> {
    fn as_node_mut(&mut self) -> &mut T;
}

type Keys<I, Item> = core::iter::FilterMap<I, fn(Item) -> Option<OwnedKeyExpr>>;
fn filter_map_weighted_node_to_key<N: IKeyExprTreeNodeMut<W>, I: AsNode<N>, W>(
    item: I,
) -> Option<OwnedKeyExpr> {
    let node: &N = item.as_node();
    node.weight().is_some().then(|| node.keyexpr())
}

/// Extension methods for KeTrees
pub trait IKeyExprTreeExt<'a, Weight>: IKeyExprTree<'a, Weight> {
    /// Returns a reference to the weight of the node at `key`
    fn weight_at(&'a self, key: &keyexpr) -> Option<&'a Weight> {
        self.node(key)
            .and_then(<Self::Node as IKeyExprTreeNode<Weight>>::weight)
    }
    /// Returns an iterator over the KEs contained in the tree that intersect with `key`
    fn intersecting_keys(
        &'a self,
        key: &'a keyexpr,
    ) -> Keys<Self::Intersection, Self::IntersectionItem>
    where
        Self::IntersectionItem: AsNode<Self::Node>,
        Self::Node: IKeyExprTreeNode<Weight>,
    {
        self.intersecting_nodes(key)
            .filter_map(filter_map_weighted_node_to_key)
    }
    /// Returns an iterator over the KEs contained in the tree that are included by `key`
    fn included_keys(&'a self, key: &'a keyexpr) -> Keys<Self::Inclusion, Self::InclusionItem>
    where
        Self::InclusionItem: AsNode<Self::Node>,
        Self::Node: IKeyExprTreeNode<Weight>,
    {
        self.included_nodes(key)
            .filter_map(filter_map_weighted_node_to_key)
    }
    /// Iterates through weighted nodes, yielding their KE and Weight.
    #[allow(clippy::type_complexity)]
    fn key_value_pairs(
        &'a self,
    ) -> core::iter::FilterMap<
        Self::TreeIter,
        fn(Self::TreeIterItem) -> Option<(OwnedKeyExpr, &'a Weight)>,
    >
    where
        Self::TreeIterItem: AsNode<Box<Self::Node>>,
    {
        self.tree_iter().filter_map(|node| {
            unsafe { core::mem::transmute::<_, Option<&Weight>>(node.as_node().weight()) }
                .map(|w| (node.as_node().keyexpr(), w))
        })
    }
}

/// Extension methods for mutable KeTrees.
pub trait IKeyExprTreeExtMut<'a, Weight>: IKeyExprTreeMut<'a, Weight> {
    /// Returns a mutable reference to the weight of the node at `key`.
    fn weight_at_mut(&'a mut self, key: &keyexpr) -> Option<&'a mut Weight> {
        self.node_mut(key)
            .and_then(<Self::Node as IKeyExprTreeNodeMut<Weight>>::weight_mut)
    }
    /// Inserts a weight at `key`, returning the previous weight if it existed.
    fn insert(&mut self, key: &keyexpr, weight: Weight) -> Option<Weight> {
        self.node_mut_or_create(key).insert_weight(weight)
    }
    /// Prunes empty nodes from the tree, unless they have at least one non-empty descendent.
    fn prune(&mut self) {
        self.prune_where(|node| node.weight().is_none())
    }
}

impl<'a, Weight, T: IKeyExprTree<'a, Weight>> IKeyExprTreeExt<'a, Weight> for T {}
impl<'a, Weight, T: IKeyExprTreeMut<'a, Weight>> IKeyExprTreeExtMut<'a, Weight> for T {}

pub trait ITokenKeyExprTreeExt<'a, Weight, Token>: ITokenKeyExprTree<'a, Weight, Token> {
    fn insert(&'a self, token: &'a mut Token, at: &keyexpr, weight: Weight) -> Option<Weight> {
        self.node_or_create(token, at).insert_weight(weight)
    }

    fn prune(&self, token: &mut Token) {
        self.prune_where(token, |node| node.weight().is_none())
    }
}
impl<'a, Weight, Token, T: ITokenKeyExprTree<'a, Weight, Token>>
    ITokenKeyExprTreeExt<'a, Weight, Token> for T
{
}
