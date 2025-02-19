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

use core::{borrow::Borrow, mem};
use std::{
    collections::{hash_map, HashMap},
    fmt,
    hash::Hash,
};

#[allow(clippy::box_collection)]
pub enum CompactMap<K, V> {
    Empty,
    Single(K, V),
    Map(Box<HashMap<K, V>>),
}

impl<K, V> Default for CompactMap<K, V>
where
    K: Eq + Hash,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<K, V> fmt::Debug for CompactMap<K, V>
where
    K: fmt::Debug + Eq + Hash,
    V: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_set().entries(self.iter()).finish()
    }
}

impl<K, V> CompactMap<K, V>
where
    K: Eq + Hash,
{
    pub fn new() -> Self {
        CompactMap::Empty
    }

    pub fn insert(&mut self, k: K, v: V) -> Option<V> {
        match self {
            CompactMap::Empty => {
                let CompactMap::Single(_, v0) = mem::replace(self, CompactMap::Empty) else {
                    unreachable!()
                };

                Some(v0)
            }
            CompactMap::Single(k0, v0) => {
                if *k0 == k {
                    Some(mem::replace(v0, v))
                } else {
                    let mut swap = CompactMap::Map(Box::default());
                    std::mem::swap(self, &mut swap);
                    if let CompactMap::Map(set) = self {
                        if let CompactMap::Single(k0, v0) = swap {
                            set.insert(k0, v0);
                            set.insert(k, v);
                            return None;
                        }
                    }
                    unreachable!()
                }
            }
            CompactMap::Map(map) => {
                if map.is_empty() {
                    *self = CompactMap::Single(k, v);
                    None
                } else {
                    map.insert(k, v)
                }
            }
        }
    }

    pub fn contains_key<Q>(&self, k: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        match self {
            CompactMap::Empty => false,
            CompactMap::Single(k0, _) => Borrow::<Q>::borrow(k0) == k,
            CompactMap::Map(map) => map.contains_key(k),
        }
    }

    pub fn get<Q>(&self, k: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        match self {
            CompactMap::Empty => None,
            CompactMap::Single(k0, v0) => (Borrow::<Q>::borrow(k0) == k).then_some(v0),
            CompactMap::Map(set) => set.get(k),
        }
    }

    pub fn get_mut<Q>(&mut self, k: &Q) -> Option<&mut V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        match self {
            CompactMap::Empty => None,
            CompactMap::Single(k0, v0) => (Borrow::<Q>::borrow(k0) == k).then_some(v0),
            CompactMap::Map(set) => set.get_mut(k),
        }
    }

    pub fn is_empty(&self) -> bool {
        matches!(self, CompactMap::Empty)
    }

    pub fn len(&self) -> usize {
        match self {
            CompactMap::Empty => 0,
            CompactMap::Single(..) => 1,
            CompactMap::Map(set) => set.len(),
        }
    }

    pub fn iter(&self) -> Iter<'_, K, V> {
        match self {
            CompactMap::Empty => Iter::Empty,
            CompactMap::Single(k0, v0) => Iter::Single(k0, v0),
            CompactMap::Map(set) => Iter::SetIter(set.iter()),
        }
    }

    pub fn entry(&mut self, k: K) -> Entry<'_, K, V> {
        match self {
            CompactMap::Empty => Entry::Empty {
                key: Some(k),
                parent: self,
            },
            CompactMap::Single(..) => Entry::Single {
                key: Some(k),
                parent: self,
            },
            CompactMap::Map(map) => Entry::Map(map.entry(k)),
        }
    }

    pub fn remove<Q>(&mut self, k: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        match self {
            CompactMap::Empty => None,
            CompactMap::Single(k0, _) => {
                if Borrow::<Q>::borrow(k0) == k {
                    let CompactMap::Single(_, v0) = mem::replace(self, CompactMap::Empty) else {
                        unreachable!()
                    };
                    Some(v0)
                } else {
                    None
                }
            }
            CompactMap::Map(map) => {
                let res = map.remove(k);
                if map.len() == 1 {
                    let (k0, v0) = map.drain().next().unwrap();
                    *self = CompactMap::Single(k0, v0);
                }
                res
            }
        }
    }

    pub fn values(&self) -> impl Iterator<Item = &V> + '_ {
        self.iter().map(|(_, v)| v)
    }

    fn as_single_mut(&mut self) -> (&mut K, &mut V) {
        if let CompactMap::Single(k, v) = self {
            (k, v)
        } else {
            unreachable!()
        }
    }

    fn as_single(&mut self) -> (&K, &V) {
        if let CompactMap::Single(k, v) = self {
            (k, v)
        } else {
            unreachable!()
        }
    }

    fn into_single(self) -> (K, V) {
        if let CompactMap::Single(k, v) = self {
            (k, v)
        } else {
            unreachable!()
        }
    }

    fn as_map_mut(&mut self) -> &mut HashMap<K, V> {
        if let CompactMap::Map(map) = self {
            map
        } else {
            unreachable!()
        }
    }

    fn swap_with_map(&mut self) -> &mut HashMap<K, V> {
        match self {
            CompactMap::Empty => {
                *self = CompactMap::Map(Box::default());
                self.as_map_mut()
            }
            CompactMap::Single(..) => {
                let (k0, v0) = mem::replace(self, CompactMap::Map(Box::default())).into_single();
                let new = self.as_map_mut();
                new.insert(k0, v0);
                new
            }
            CompactMap::Map(map) => map,
        }
    }
}

impl<K, V> FromIterator<(K, V)> for CompactMap<K, V>
where
    K: Eq + Hash,
{
    #[inline]
    fn from_iter<I: IntoIterator<Item = (K, V)>>(iter: I) -> CompactMap<K, V> {
        let mut set = CompactMap::default();
        set.extend(iter);
        set
    }
}

impl<K, V> Extend<(K, V)> for CompactMap<K, V>
where
    K: Eq + Hash,
{
    #[inline]
    fn extend<I: IntoIterator<Item = (K, V)>>(&mut self, iter: I) {
        for (k, v) in iter.into_iter() {
            self.insert(k, v);
        }
    }
}

impl<'a, K, V> Extend<&'a (K, V)> for CompactMap<K, V>
where
    K: 'a + Eq + Hash + Copy,
    V: 'a + Clone,
{
    #[inline]
    fn extend<I: IntoIterator<Item = &'a (K, V)>>(&mut self, iter: I) {
        self.extend(iter.into_iter().cloned());
    }
}

pub enum Iter<'a, K, V> {
    Empty,
    Single(&'a K, &'a V),
    SetIter(hash_map::Iter<'a, K, V>),
}

impl<'a, K, V> Iterator for Iter<'a, K, V> {
    type Item = (&'a K, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Iter::Empty => None,
            Iter::Single(..) => {
                let mut swap = Iter::Empty;
                mem::swap(self, &mut swap);
                if let Iter::Single(k0, v0) = swap {
                    Some((k0, v0))
                } else {
                    None
                }
            }
            Iter::SetIter(iter) => iter.next(),
        }
    }
}

pub enum IntoIter<K, V> {
    Empty,
    Single(K, V),
    Map(hash_map::IntoIter<K, V>),
}

impl<K, V> Iterator for IntoIter<K, V> {
    type Item = (K, V);
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            IntoIter::Empty | IntoIter::Single(..) => {
                let mut swap = IntoIter::Empty;
                mem::swap(self, &mut swap);
                if let IntoIter::Single(k0, v0) = swap {
                    Some((k0, v0))
                } else {
                    None
                }
            }
            IntoIter::Map(set) => set.next(),
        }
    }
}

impl<'a, K, V> IntoIterator for &'a CompactMap<K, V>
where
    K: Eq + Hash,
{
    type Item = (&'a K, &'a V);
    type IntoIter = Iter<'a, K, V>;

    #[inline]
    fn into_iter(self) -> Iter<'a, K, V> {
        self.iter()
    }
}

impl<K, V> IntoIterator for CompactMap<K, V> {
    type Item = (K, V);
    type IntoIter = IntoIter<K, V>;

    fn into_iter(self) -> Self::IntoIter {
        match self {
            CompactMap::Empty => IntoIter::Empty,
            CompactMap::Single(k0, v0) => IntoIter::Single(k0, v0),
            CompactMap::Map(set) => IntoIter::Map(set.into_iter()),
        }
    }
}

pub enum Entry<'a, K, V> {
    Single {
        key: Option<K>,
        parent: &'a mut CompactMap<K, V>,
    },
    Empty {
        key: Option<K>,
        parent: &'a mut CompactMap<K, V>,
    },
    Map(hash_map::Entry<'a, K, V>),
}

impl<'a, K, V> Entry<'a, K, V>
where
    K: Eq + Hash,
{
    pub fn or_insert_with<F: FnOnce() -> V>(self, f: F) -> &'a mut V {
        match self {
            Entry::Map(entry) => entry.or_insert_with(f),
            Entry::Single { mut key, parent } => {
                let k0 = parent.as_single().0;
                let k = key.take().unwrap();
                if k == *k0 {
                    parent.as_single_mut().1
                } else {
                    parent.swap_with_map().entry(k).or_insert_with(f)
                }
            }
            Entry::Empty { mut key, parent } => {
                *parent = CompactMap::Single(key.take().unwrap(), f());
                parent.as_single_mut().1
            }
        }
    }
}
