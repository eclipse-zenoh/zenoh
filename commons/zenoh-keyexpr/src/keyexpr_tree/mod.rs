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

use crate::{keyexpr, OwnedKeyExpr};
pub mod traits;
pub use traits::*;

pub mod arc_tree;
pub use arc_tree::{DefaultToken, KeArcTree};
pub mod box_tree;
pub use box_tree::KeBoxTree;
pub mod impls;
pub use impls::DefaultChildrenProvider;
mod iters;
pub use iters::*;

#[cfg(test)]
mod test;

pub trait IWildness: 'static {
    fn non_wild() -> Self;
    fn get(&self) -> bool;
    fn set(&mut self, wildness: bool) -> bool;
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NonWild;
impl IWildness for NonWild {
    fn non_wild() -> Self {
        NonWild
    }
    fn get(&self) -> bool {
        false
    }
    fn set(&mut self, wildness: bool) -> bool {
        if wildness {
            panic!("Attempted to set NonWild to wild, which breaks its contract. You likely attempted to insert a wild key into a `NonWild` tree. Use `bool` instead to make wildness determined at runtime.")
        }
        false
    }
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UnknownWildness;
impl IWildness for UnknownWildness {
    fn non_wild() -> Self {
        UnknownWildness
    }
    fn get(&self) -> bool {
        true
    }
    fn set(&mut self, _wildness: bool) -> bool {
        true
    }
}
impl IWildness for bool {
    fn non_wild() -> Self {
        false
    }
    fn get(&self) -> bool {
        *self
    }
    fn set(&mut self, wildness: bool) -> bool {
        core::mem::replace(self, wildness)
    }
}
