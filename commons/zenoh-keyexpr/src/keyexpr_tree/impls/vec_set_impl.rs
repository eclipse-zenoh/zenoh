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

use zenoh_result::unlikely;

use crate::keyexpr_tree::*;

pub struct VecSetProvider;
impl<T: 'static> IChildrenProvider<T> for VecSetProvider {
    type Assoc = Vec<T>;
}

impl<'a, 'b, T: HasChunk> IEntry<'a, 'b, T> for Entry<'a, 'b, T> {
    fn get_or_insert_with<F: FnOnce(&'b keyexpr) -> T>(self, f: F) -> &'a mut T {
        match self {
            Entry::Vacant(vec, key) => {
                vec.push(f(key));
                vec.last_mut().unwrap()
            }
            Entry::Occupied(v) => v,
        }
    }
}

pub enum Entry<'a, 'b, T> {
    Vacant(&'a mut Vec<T>, &'b keyexpr),
    Occupied(&'a mut T),
}
impl<T: HasChunk + AsNode<T> + AsNodeMut<T> + 'static> IChildren<T> for Vec<T> {
    type Node = T;
    fn child_at(&self, chunk: &keyexpr) -> Option<&T> {
        self.iter().find(|t| unlikely(t.chunk() == chunk))
    }
    fn child_at_mut(&mut self, chunk: &keyexpr) -> Option<&mut T> {
        self.iter_mut().find(|t| unlikely(t.chunk() == chunk))
    }
    fn remove(&mut self, chunk: &keyexpr) -> Option<Self::Node> {
        for (i, t) in self.iter().enumerate() {
            if unlikely(t.chunk() == chunk) {
                return Some(self.swap_remove(i));
            }
        }
        None
    }
    fn len(&self) -> usize {
        self.len()
    }
    fn is_empty(&self) -> bool {
        self.is_empty()
    }
    type Entry<'a, 'b>
        = Entry<'a, 'b, T>
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
        let this = unsafe { &mut *(self as *mut Self) };
        match self.child_at_mut(chunk) {
            Some(entry) => Entry::Occupied(entry),
            None => Entry::Vacant(this, chunk),
        }
    }

    type Iter<'a>
        = core::slice::Iter<'a, T>
    where
        Self: 'a;
    fn children<'a>(&'a self) -> Self::Iter<'a>
    where
        Self: 'a,
    {
        self.iter()
    }

    type IterMut<'a>
        = core::slice::IterMut<'a, T>
    where
        Self: 'a;

    fn children_mut<'a>(&'a mut self) -> Self::IterMut<'a>
    where
        Self: 'a,
    {
        self.iter_mut()
    }

    fn filter_out<F: FnMut(&mut T) -> bool>(&mut self, predicate: &mut F) {
        for i in (0..self.len()).rev() {
            if predicate(&mut self[i]) {
                self.swap_remove(i);
            }
        }
    }

    type Intersection<'a>
        = super::FilterMap<core::slice::Iter<'a, T>, super::Intersection<'a>>
    where
        Self: 'a,
        Self::Node: 'a;
    fn intersection<'a>(&'a self, key: &'a keyexpr) -> Self::Intersection<'a> {
        super::FilterMap::new(self.iter(), super::Intersection(key))
    }
    type IntersectionMut<'a>
        = super::FilterMap<core::slice::IterMut<'a, T>, super::Intersection<'a>>
    where
        Self: 'a,
        Self::Node: 'a;
    fn intersection_mut<'a>(&'a mut self, key: &'a keyexpr) -> Self::IntersectionMut<'a> {
        super::FilterMap::new(self.iter_mut(), super::Intersection(key))
    }
    type Inclusion<'a>
        = super::FilterMap<core::slice::Iter<'a, T>, super::Inclusion<'a>>
    where
        Self: 'a,
        Self::Node: 'a;
    fn inclusion<'a>(&'a self, key: &'a keyexpr) -> Self::Inclusion<'a> {
        super::FilterMap::new(self.iter(), super::Inclusion(key))
    }
    type InclusionMut<'a>
        = super::FilterMap<core::slice::IterMut<'a, T>, super::Inclusion<'a>>
    where
        Self: 'a,
        Self::Node: 'a;
    fn inclusion_mut<'a>(&'a mut self, key: &'a keyexpr) -> Self::InclusionMut<'a> {
        super::FilterMap::new(self.iter_mut(), super::Inclusion(key))
    }
}
