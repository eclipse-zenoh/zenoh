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

pub use hashmap_impl::HashMapProvider;
pub use keyed_set_impl::KeyedSetProvider;
pub use vec_set_impl::VecSetProvider;

use crate::keyexpr;
mod hashmap_impl;
mod keyed_set_impl;
mod vec_set_impl;

/// The advised way of storing children in KeTrees, based on benchmarks.
pub type DefaultChildrenProvider = KeyedSetProvider;
pub struct FilterMap<I, F> {
    iter: I,
    filter: F,
}

impl<I, F> FilterMap<I, F> {
    fn new(iter: I, filter: F) -> Self {
        Self { iter, filter }
    }
}
pub trait IFilter<I> {
    type O;
    fn filter_map(&self, i: I) -> Option<Self::O>;
}
impl<I: Iterator, F: IFilter<<I as Iterator>::Item>> Iterator for FilterMap<I, F> {
    type Item = F::O;

    fn next(&mut self) -> Option<Self::Item> {
        for next in self.iter.by_ref() {
            if let Some(output) = self.filter.filter_map(next) {
                return Some(output);
            }
        }
        None
    }
}
pub struct Intersection<'a>(pub &'a keyexpr);
impl<K: core::ops::Deref<Target = keyexpr>, V> IFilter<(&K, V)> for Intersection<'_> {
    type O = V;
    fn filter_map(&self, (k, v): (&K, V)) -> Option<Self::O> {
        self.0.intersects(k).then_some(v)
    }
}

impl<'a, T: super::HasChunk> IFilter<&'a T> for Intersection<'_> {
    type O = &'a T;
    fn filter_map(&self, t: &'a T) -> Option<Self::O> {
        self.0.intersects(t.chunk()).then_some(t)
    }
}

impl<'a, T: super::HasChunk> IFilter<&'a mut T> for Intersection<'_> {
    type O = &'a mut T;
    fn filter_map(&self, t: &'a mut T) -> Option<Self::O> {
        self.0.intersects(t.chunk()).then_some(t)
    }
}

pub struct Inclusion<'a>(pub &'a keyexpr);
impl<K: core::ops::Deref<Target = keyexpr>, V> IFilter<(&K, V)> for Inclusion<'_> {
    type O = V;
    fn filter_map(&self, (k, v): (&K, V)) -> Option<Self::O> {
        self.0.includes(k).then_some(v)
    }
}

impl<'a, T: super::HasChunk> IFilter<&'a T> for Inclusion<'_> {
    type O = &'a T;
    fn filter_map(&self, t: &'a T) -> Option<Self::O> {
        self.0.includes(t.chunk()).then_some(t)
    }
}

impl<'a, T: super::HasChunk> IFilter<&'a mut T> for Inclusion<'_> {
    type O = &'a mut T;
    fn filter_map(&self, t: &'a mut T) -> Option<Self::O> {
        self.0.includes(t.chunk()).then_some(t)
    }
}
