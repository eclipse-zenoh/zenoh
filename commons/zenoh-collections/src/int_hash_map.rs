//
// Copyright (c) 2025 ZettaScale Technology
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
use std::{collections::hash_map, hash::Hash, mem, slice};

/// Decides to fall back to a hashmap if the load factor is below 75%.
///
/// 3/4 is a common load factor threshold for hashmap, even if Rust implementation use 7/8.
/// Also, doesn't fall back if the integer set is too small to matter
fn fallback_to_hashmap(key: usize, len: usize) -> bool {
    key >= 16 && 4 * len < 3 * key
}

/// A hashmap with integer keys, with optimized storage when the key set is increasingly growing
/// and dense enough.
///
/// With such a key set, values can indeed be stored directly in a vector, allowing direct access
/// instead of hashmap heavy mechanics, improving performance **a lot**. If the load factor fall
/// too low, then the storage falls back to a regular hashmap.
/// The whole API is fully compatible with `HashMap` one.
#[derive(Debug)]
pub enum IntHashMap<K, V> {
    // Because maps can have holes, the value is optional in the vector. The key is also stored,
    // in order to provide a compatible iteration API
    Vec {
        vec: Vec<Option<(K, V)>>,
        items: usize,
    },
    Map(ahash::HashMap<K, V>),
}

impl<K, V> IntHashMap<K, V> {
    #[inline]
    pub const fn new() -> Self {
        Self::Vec {
            vec: Vec::new(),
            items: 0,
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        match self {
            Self::Vec { items, .. } => *items,
            Self::Map(map) => map.len(),
        }
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[inline]
    pub fn iter(&self) -> Iter<'_, K, V> {
        match self {
            Self::Vec { vec, .. } => Iter::Vec(vec.iter()),
            Self::Map(map) => Iter::Map(map.iter()),
        }
    }

    #[inline]
    pub fn values(&self) -> Values<'_, K, V> {
        match self {
            Self::Vec { vec, .. } => Values::Vec(vec.iter()),
            Self::Map(map) => Values::Map(map.values()),
        }
    }

    #[inline]
    pub fn values_mut(&mut self) -> ValuesMut<'_, K, V> {
        match self {
            Self::Vec { vec, .. } => ValuesMut::Vec(vec.iter_mut()),
            Self::Map(map) => ValuesMut::Map(map.values_mut()),
        }
    }
}

impl<K: Copy + Into<usize> + Eq + Hash, V> IntHashMap<K, V> {
    #[inline]
    pub fn get(&self, k: &K) -> Option<&V> {
        match self {
            Self::Vec { vec, .. } => Some(&vec.get((*k).into())?.as_ref()?.1),
            Self::Map(map) => map.get(k),
        }
    }

    #[inline]
    pub fn get_mut(&mut self, k: &K) -> Option<&mut V> {
        match self {
            Self::Vec { vec, .. } => Some(&mut vec.get_mut((*k).into())?.as_mut()?.1),
            Self::Map(map) => map.get_mut(k),
        }
    }

    #[inline]
    pub fn contains_key(&self, k: &K) -> bool {
        match self {
            Self::Vec { vec, .. } => vec.get((*k).into()).is_some_and(|kv| kv.is_some()),
            Self::Map(map) => map.contains_key(k),
        }
    }

    /// Resize the map to be able to insert the given key.
    ///
    /// If its back by a vector, then it means increasing vector size to be able to use direct
    /// access with the key.
    /// If the load factor becomes too low, then the vector is collected into and replaced by a
    /// hashmap.
    fn resize(&mut self, k: K) {
        if let Self::Vec { vec, items } = self {
            if fallback_to_hashmap(k.into(), *items) {
                let map = mem::take(vec).into_iter().flatten().collect();
                *self = Self::Map(map);
            } else if k.into() >= vec.len() {
                vec.resize_with(k.into() + 1, || None);
            }
        }
    }

    #[inline]
    pub fn insert(&mut self, k: K, value: V) -> Option<V> {
        self.resize(k);
        match self {
            Self::Vec { vec, items } => {
                let kv = &mut vec[k.into()];
                if kv.is_none() {
                    *items += 1;
                }
                Some(kv.replace((k, value))?.1)
            }
            Self::Map(map) => map.insert(k, value),
        }
    }

    #[inline]
    pub fn remove(&mut self, k: &K) -> Option<V> {
        match self {
            Self::Vec { vec, items } => {
                let kv = &mut vec.get_mut((*k).into())?;
                if kv.is_some() {
                    *items -= 1;
                }
                Some(kv.take()?.1)
            }
            Self::Map(map) => map.remove(k),
        }
    }

    #[inline]
    pub fn clear(&mut self) {
        match self {
            Self::Vec { vec, .. } => vec.clear(),
            Self::Map(map) => map.clear(),
        }
    }

    pub fn entry(&mut self, key: K) -> Entry<'_, K, V> {
        self.resize(key);
        match self {
            Self::Vec { vec, items } => Entry::Vec {
                key,
                entry: &mut vec[key.into()],
                items,
            },
            Self::Map(map) => Entry::Map(map.entry(key)),
        }
    }
}

impl<K, V> Default for IntHashMap<K, V> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

pub enum Entry<'a, K, V> {
    Vec {
        key: K,
        entry: &'a mut Option<(K, V)>,
        items: &'a mut usize,
    },
    Map(hash_map::Entry<'a, K, V>),
}

impl<'a, K, V> Entry<'a, K, V> {
    pub fn or_insert_with<F: FnOnce() -> V>(self, default: F) -> &'a mut V {
        match self {
            Entry::Vec { key, entry, items } => {
                if entry.is_none() {
                    *items += 1
                };
                &mut entry.get_or_insert_with(|| (key, default())).1
            }
            Entry::Map(entry) => entry.or_insert_with(default),
        }
    }
}

pub enum Iter<'a, K, V> {
    Vec(slice::Iter<'a, Option<(K, V)>>),
    Map(hash_map::Iter<'a, K, V>),
}

impl<'a, K, V> Iterator for Iter<'a, K, V> {
    type Item = (&'a K, &'a V);

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Vec(iter) => iter.by_ref().flatten().map(|(k, v)| (k, v)).next(),
            Self::Map(iter) => iter.next(),
        }
    }
}

pub enum Values<'a, K, V> {
    Vec(slice::Iter<'a, Option<(K, V)>>),
    Map(hash_map::Values<'a, K, V>),
}
impl<'a, K, V> Iterator for Values<'a, K, V> {
    type Item = &'a V;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Vec(iter) => iter.by_ref().flatten().map(|(_, v)| v).next(),
            Self::Map(iter) => iter.next(),
        }
    }
}

pub enum ValuesMut<'a, K, V> {
    Vec(slice::IterMut<'a, Option<(K, V)>>),
    Map(hash_map::ValuesMut<'a, K, V>),
}
impl<'a, K, V> Iterator for ValuesMut<'a, K, V> {
    type Item = &'a mut V;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Vec(iter) => iter.by_ref().flatten().map(|(_, v)| v).next(),
            Self::Map(iter) => iter.next(),
        }
    }
}

impl<'a, K, V> IntoIterator for &'a IntHashMap<K, V> {
    type Item = (&'a K, &'a V);
    type IntoIter = Iter<'a, K, V>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

#[derive(Debug)]
pub struct IntHashSet<T>(IntHashMap<T, ()>);

impl<T> IntHashSet<T> {
    #[inline]
    pub const fn new() -> IntHashSet<T> {
        Self(IntHashMap::new())
    }
}

impl<T: Copy + Into<usize> + Eq + Hash> IntHashSet<T> {
    #[inline]
    pub fn contains(&self, value: &T) -> bool {
        self.0.contains_key(value)
    }

    #[inline]
    pub fn insert(&mut self, value: T) -> bool {
        self.0.insert(value, ()).is_none()
    }
}

impl<T> Default for IntHashSet<T> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}
