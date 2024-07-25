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

//! KeTrees are specialized data structures to work with sets of values addressed by key expressions.
//!
//! Think of it like HashMaps that were specifically designed to allow iterating over key expressions related to a given query
//! in a fast and semantically correct way.
//!
//! # KeTrees
//! Key Expression Trees (KeTrees) are trees that split any KE inserted into them into its chunks.
//!
//! For example, if you take an empty KeTree, and insert the `(a/b, 0)`, `(a/b/c, 1)`, `(a/b/d, 2)`, `(a/e, 3)` and (a/@f, 4) pairs into it,
//! the tree will be as follows:
//! ```text
//! a --- e  = 3
//!    |- b  = 0 --- c = 1
//!    |          |- d = 2
//!    |- @f = 4
//! ```
//!
//! Note that the `a` node exists, despite no value being assigned to it. If you iterate over all nodes, the iterator may yield
//! `(a, None), (a/e, Some(3)), (a/b, Some(0)), (a/b/c, Some(1)), (a/b/d, Some(2)), (a/@f, Some(4))`.
//!
//! # Ownership
//! KeTrees come in two flavours:
//! - [`KeBoxTree`] is the easier flavour. Much like a HashMap, it uniquely owns all of its nodes and data.
//! - [`KeArcTree`] allows the shared ownership of nodes, allowing you to store subsections of the tree elsewhere
//!   without worrying about lifetimes.
//!
//! # Usage
//! KeTrees were designed to maximize code reuse. As such, their core properties are reflected through the [`IKeyExprTree`] and [`IKeyExprTreeMut`] traits.
//!
//! KeTrees are made up of node, where nodes may or may not have a value (called `weight`) associated with them. To access these weighs, as well as other
//! properties of a node, you can go through the [`IKeyExprTreeNode`] and [`IKeyExprTreeNodeMut`] traits.
//!
//! # Iterators
//! KeTrees provide iterators for the following operations:
//! - Iterating on all nodes ([`IKeyExprTree::tree_iter`]/[`IKeyExprTreeMut::tree_iter_mut`])
//! - Iterating on key-value pairs in the KeTree ([`IKeyExprTree::key_value_pairs`])
//! - Iterating on nodes whose KE intersects with a queried KE ([`IKeyExprTree::intersecting_nodes`], [`IKeyExprTreeMut::intersecting_nodes_mut`])
//! - Iterating on nodes whose KE are included by a queried KE ([`IKeyExprTree::included_nodes`], [`IKeyExprTreeMut::included_nodes_mut`])
//! - Iterating on nodes whose KE includes a queried KE ([`IKeyExprTree::nodes_including`], [`IKeyExprTreeMut::nodes_including_mut`])
//!
//! While the order of iteration is not guaranteed, a node will never be yielded before one of its parents if both are to appear.  
//! For example, iterating on nodes that intersect with `**` in the previously defined tree is guaranteed to yield one of the following sequences:
//! - `(a, None), (a/e, Some(3)), (a/b, Some(0)), (a/b/c, Some(1)), (a/b/d, Some(2))`
//! - `(a, None), (a/e, Some(3)), (a/b, Some(0)), (a/b/d, Some(2)), (a/b/c, Some(1))`
//! - `(a, None), (a/b, Some(0)), (a/b/c, Some(1)), (a/b/d, Some(2)), (a/e, Some(3))`
//! - `(a, None), (a/b, Some(0)), (a/b/d, Some(2)), (a/b/c, Some(1)), (a/e, Some(3))`

use crate::{keyexpr, OwnedKeyExpr};
/// Allows importing all of the KeTree traits at once.
pub mod traits;
pub use traits::*;

/// An implementation of a KeTree with shared-ownership of nodes, using [`token_cell`] to allow safe access to the tree's data.
///
/// This implementation allows sharing ownership of members of the KeTree.
pub mod arc_tree;
pub use arc_tree::{DefaultToken, KeArcTree};
/// An implementation of a KeTree that owns all of its nodes.
pub mod box_tree;
pub use box_tree::KeBoxTree;
/// KeTrees can store their children in different manners.
///
/// This module contains a few implementations.
pub mod impls;
pub use impls::DefaultChildrenProvider;
mod iters;
pub use iters::*;

#[cfg(test)]
mod test;

pub mod support;
