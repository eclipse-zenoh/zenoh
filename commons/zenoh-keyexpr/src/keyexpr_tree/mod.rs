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

use crate::{keyexpr, OwnedKeyExpr};
/// Allows importing all of the KeTree traits at once.
pub mod traits;
pub use traits::*;

/// An implementation of a KeTree with shared-ownership of nodes, using [`token_cell`] to allow safe access to the tree's data.
///
/// This implementation allows sharing references to members of the KeTree.
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
