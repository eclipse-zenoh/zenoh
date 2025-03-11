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
use std::collections::hash_map::DefaultHasher;

use keyed_set::{KeyExtractor, KeyedSet};

use crate::keyexpr_tree::*;

#[cfg_attr(not(feature = "std"), allow(deprecated))]
pub struct KeyedSetProvider<Hash: Hasher + Default + 'static = DefaultHasher>(
    core::marker::PhantomData<Hash>,
);
impl<T: 'static, Hash: Hasher + Default + 'static> IChildrenProvider<T> for KeyedSetProvider<Hash> {
    type Assoc = KeyedSet<T, ChunkExtractor>;
}
#[derive(Debug, Default, Clone, Copy)]
pub struct ChunkExtractor;
impl<'a, T: HasChunk> KeyExtractor<'a, T> for ChunkExtractor {
    type Key = &'a keyexpr;
    fn extract(&self, from: &'a T) -> Self::Key {
        from.chunk()
    }
}
impl<'a, 'b, T: HasChunk> IEntry<'a, 'b, T>
    for keyed_set::Entry<'a, T, ChunkExtractor, &'b keyexpr>
{
    fn get_or_insert_with<F: FnOnce(&'b keyexpr) -> T>(self, f: F) -> &'a mut T {
        Self::get_or_insert_with(self, f)
    }
}

impl<T: HasChunk + AsNode<T> + AsNodeMut<T>> IChildren<T> for KeyedSet<T, ChunkExtractor> {
    type Node = T;
    fn child_at(&self, chunk: &keyexpr) -> Option<&T> {
        self.get(&chunk)
    }
    fn child_at_mut(&mut self, chunk: &keyexpr) -> Option<&mut T> {
        // Unicity is guaranteed by &mut self
        unsafe { self.get_mut_unguarded(&chunk) }
    }
    fn remove(&mut self, chunk: &keyexpr) -> Option<T> {
        self.remove(&chunk)
    }
    fn len(&self) -> usize {
        self.len()
    }
    fn is_empty(&self) -> bool {
        self.is_empty()
    }
    type Entry<'a, 'b>
        = keyed_set::Entry<'a, T, ChunkExtractor, &'b keyexpr>
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
        self.entry(chunk)
    }

    type Iter<'a>
        = keyed_set::Iter<'a, T>
    where
        Self: 'a;
    fn children<'a>(&'a self) -> Self::Iter<'a>
    where
        Self: 'a,
    {
        self.iter()
    }

    type IterMut<'a>
        = keyed_set::IterMut<'a, T>
    where
        Self: 'a;

    fn children_mut<'a>(&'a mut self) -> Self::IterMut<'a>
    where
        Self: 'a,
    {
        self.iter_mut()
    }

    fn filter_out<F: FnMut(&mut T) -> bool>(&mut self, predicate: &mut F) {
        self.drain_where(predicate);
    }

    type Intersection<'a>
        = super::FilterMap<keyed_set::Iter<'a, T>, super::Intersection<'a>>
    where
        Self: 'a,
        Self::Node: 'a;
    fn intersection<'a>(&'a self, key: &'a keyexpr) -> Self::Intersection<'a> {
        super::FilterMap::new(self.iter(), super::Intersection(key))
    }
    type IntersectionMut<'a>
        = super::FilterMap<keyed_set::IterMut<'a, T>, super::Intersection<'a>>
    where
        Self: 'a,
        Self::Node: 'a;
    fn intersection_mut<'a>(&'a mut self, key: &'a keyexpr) -> Self::IntersectionMut<'a> {
        super::FilterMap::new(self.iter_mut(), super::Intersection(key))
    }
    type Inclusion<'a>
        = super::FilterMap<keyed_set::Iter<'a, T>, super::Inclusion<'a>>
    where
        Self: 'a,
        Self::Node: 'a;
    fn inclusion<'a>(&'a self, key: &'a keyexpr) -> Self::Inclusion<'a> {
        super::FilterMap::new(self.iter(), super::Inclusion(key))
    }
    type InclusionMut<'a>
        = super::FilterMap<keyed_set::IterMut<'a, T>, super::Inclusion<'a>>
    where
        Self: 'a,
        Self::Node: 'a;
    fn inclusion_mut<'a>(&'a mut self, key: &'a keyexpr) -> Self::InclusionMut<'a> {
        super::FilterMap::new(self.iter_mut(), super::Inclusion(key))
    }
}
