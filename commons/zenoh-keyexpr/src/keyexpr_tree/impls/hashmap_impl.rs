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

use core::hash::Hasher;
#[cfg(not(feature = "std"))]
// `SipHasher` is deprecated in favour of a symbol that only exists in `std`
#[allow(deprecated)]
use core::hash::SipHasher as DefaultHasher;
#[cfg(feature = "std")]
use std::collections::{
    hash_map::{DefaultHasher, Entry, Iter, IterMut, Values, ValuesMut},
    HashMap,
};

#[cfg(not(feature = "std"))]
use hashbrown::{
    hash_map::{Entry, Iter, IterMut, Values, ValuesMut},
    HashMap,
};

use crate::keyexpr_tree::*;

#[cfg_attr(not(feature = "std"), allow(deprecated))]
pub struct HashMapProvider<Hash: Hasher + Default + 'static = DefaultHasher>(
    core::marker::PhantomData<Hash>,
);
impl<T: 'static, Hash: Hasher + Default + 'static> IChildrenProvider<T> for HashMapProvider<Hash> {
    type Assoc = HashMap<OwnedKeyExpr, T, core::hash::BuildHasherDefault<Hash>>;
}

#[cfg(not(feature = "std"))]
impl<'a: 'b, 'b, T: HasChunk, S: core::hash::BuildHasher> IEntry<'a, 'b, T>
    for Entry<'a, OwnedKeyExpr, T, S>
{
    fn get_or_insert_with<F: FnOnce(&'b keyexpr) -> T>(self, f: F) -> &'a mut T {
        match self {
            Entry::Vacant(entry) => {
                let value = unsafe { f(core::mem::transmute::<&keyexpr, &keyexpr>(entry.key())) };
                entry.insert(value)
            }
            Entry::Occupied(v) => v.into_mut(),
        }
    }
}
#[cfg(feature = "std")]
impl<'a: 'b, 'b, T: HasChunk> IEntry<'a, 'b, T> for Entry<'a, OwnedKeyExpr, T> {
    fn get_or_insert_with<F: FnOnce(&'b keyexpr) -> T>(self, f: F) -> &'a mut T {
        match self {
            Entry::Vacant(entry) => {
                let value = unsafe { f(core::mem::transmute::<&keyexpr, &keyexpr>(entry.key())) };
                entry.insert(value)
            }
            Entry::Occupied(v) => v.into_mut(),
        }
    }
}

impl<T: HasChunk + AsNode<T> + AsNodeMut<T> + 'static, S: core::hash::BuildHasher> IChildren<T>
    for HashMap<OwnedKeyExpr, T, S>
{
    type Node = T;
    fn child_at(&self, chunk: &keyexpr) -> Option<&T> {
        self.get(chunk)
    }
    fn child_at_mut(&mut self, chunk: &keyexpr) -> Option<&mut T> {
        self.get_mut(chunk)
    }
    fn remove(&mut self, chunk: &keyexpr) -> Option<Self::Node> {
        self.remove(chunk)
    }
    fn len(&self) -> usize {
        self.len()
    }
    fn is_empty(&self) -> bool {
        self.is_empty()
    }

    #[cfg(feature = "std")]
    type Entry<'a, 'b>
        = Entry<'a, OwnedKeyExpr, T>
    where
        Self: 'a,
        'a: 'b,
        T: 'b;
    #[cfg(not(feature = "std"))]
    type Entry<'a, 'b>
        = Entry<'a, OwnedKeyExpr, T, S>
    where
        Self: 'a,
        'a: 'b,
        T: 'b;
    fn entry<'a, 'b>(&'a mut self, chunk: &'b keyexpr) -> Self::Entry<'a, 'b>
    where
        Self: 'a,
        'a: 'b,
        T: 'b,
    {
        self.entry(chunk.into())
    }

    type Iter<'a>
        = Values<'a, OwnedKeyExpr, T>
    where
        Self: 'a;
    fn children<'a>(&'a self) -> Self::Iter<'a>
    where
        Self: 'a,
    {
        self.values()
    }

    type IterMut<'a>
        = ValuesMut<'a, OwnedKeyExpr, T>
    where
        Self: 'a;

    fn children_mut<'a>(&'a mut self) -> Self::IterMut<'a>
    where
        Self: 'a,
    {
        self.values_mut()
    }

    fn filter_out<F: FnMut(&mut T) -> bool>(&mut self, predicate: &mut F) {
        self.retain(|_, v| predicate(v));
    }

    type Intersection<'a>
        = super::FilterMap<Iter<'a, OwnedKeyExpr, T>, super::Intersection<'a>>
    where
        Self: 'a,
        Self::Node: 'a;
    fn intersection<'a>(&'a self, key: &'a keyexpr) -> Self::Intersection<'a> {
        super::FilterMap::new(self.iter(), super::Intersection(key))
    }
    type IntersectionMut<'a>
        = super::FilterMap<IterMut<'a, OwnedKeyExpr, T>, super::Intersection<'a>>
    where
        Self: 'a,
        Self::Node: 'a;
    fn intersection_mut<'a>(&'a mut self, key: &'a keyexpr) -> Self::IntersectionMut<'a> {
        super::FilterMap::new(self.iter_mut(), super::Intersection(key))
    }
    type Inclusion<'a>
        = super::FilterMap<Iter<'a, OwnedKeyExpr, T>, super::Inclusion<'a>>
    where
        Self: 'a,
        Self::Node: 'a;
    fn inclusion<'a>(&'a self, key: &'a keyexpr) -> Self::Inclusion<'a> {
        super::FilterMap::new(self.iter(), super::Inclusion(key))
    }
    type InclusionMut<'a>
        = super::FilterMap<IterMut<'a, OwnedKeyExpr, T>, super::Inclusion<'a>>
    where
        Self: 'a,
        Self::Node: 'a;
    fn inclusion_mut<'a>(&'a mut self, key: &'a keyexpr) -> Self::InclusionMut<'a> {
        super::FilterMap::new(self.iter_mut(), super::Inclusion(key))
    }
}
