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
pub enum IntHashMap<K: Copy + Into<usize> + TryFrom<usize>, V> {
    // Because maps can have holes, the value is optional in the vector. The key is also stored,
    // in order to provide a compatible iteration API
    Vec {
        vec: Vec<(K, Option<V>)>,
        len: usize,
    },
    Map(ahash::HashMap<K, V>),
}

impl<K: Copy + Into<usize> + TryFrom<usize>, V> IntHashMap<K, V> {
    #[inline]
    pub fn new() -> Self {
        Self::Vec {
            vec: Vec::new(),
            len: 0,
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        match self {
            Self::Vec { len, .. } => *len,
            Self::Map(map) => map.len(),
        }
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[inline]
    pub fn iter(&self) -> Iter<K, V> {
        match self {
            Self::Vec { vec, .. } => Iter::Vec(vec.iter()),
            Self::Map(map) => Iter::Map(map.iter()),
        }
    }

    #[inline]
    pub fn values(&self) -> Values<K, V> {
        match self {
            Self::Vec { vec, .. } => Values::Vec(vec.iter()),
            Self::Map(map) => Values::Map(map.values()),
        }
    }

    #[inline]
    pub fn values_mut(&mut self) -> ValuesMut<K, V> {
        match self {
            Self::Vec { vec, .. } => ValuesMut::Vec(vec.iter_mut()),
            Self::Map(map) => ValuesMut::Map(map.values_mut()),
        }
    }
}

impl<K: Copy + Into<usize> + TryFrom<usize> + Eq + Hash, V> IntHashMap<K, V> {
    #[inline]
    pub fn get(&self, k: &K) -> Option<&V> {
        match self {
            Self::Vec { vec, .. } => vec.get((*k).into())?.1.as_ref(),
            Self::Map(map) => map.get(k),
        }
    }

    #[inline]
    pub fn get_mut(&mut self, k: &K) -> Option<&mut V> {
        match self {
            Self::Vec { vec, .. } => vec.get_mut((*k).into())?.1.as_mut(),
            Self::Map(map) => map.get_mut(k),
        }
    }

    #[inline]
    pub fn contains_key(&self, k: &K) -> bool {
        match self {
            Self::Vec { vec, .. } => vec.get((*k).into()).is_some(),
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
        if let Self::Vec { vec, len } = self {
            if fallback_to_hashmap(k.into(), *len) {
                let map = mem::take(vec)
                    .into_iter()
                    .filter_map(|(k, v)| Some((k, v?)))
                    .collect();
                *self = Self::Map(map);
            } else {
                for i in vec.len()..=k.into() {
                    vec.push((i.try_into().unwrap_or_else(|_| unreachable!()), None));
                }
            }
        }
    }

    #[inline]
    pub fn insert(&mut self, k: K, value: V) -> Option<V> {
        self.resize(k);
        match self {
            Self::Vec { vec, len } => {
                let v = &mut vec[k.into()].1;
                if v.is_none() {
                    *len += 1;
                }
                v.replace(value)
            }
            Self::Map(map) => map.insert(k, value),
        }
    }

    #[inline]
    pub fn remove(&mut self, k: &K) -> Option<V> {
        match self {
            Self::Vec { vec, len } => {
                let value = &mut vec.get_mut((*k).into())?.1;
                if value.is_some() {
                    *len -= 1;
                }
                value.take()
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

    pub fn entry(&mut self, k: K) -> Entry<K, V> {
        self.resize(k);
        match self {
            Self::Vec { vec, len } => Entry::Vec {
                value: &mut vec[k.into()].1,
                len,
            },
            Self::Map(map) => Entry::Map(map.entry(k)),
        }
    }
}

impl<K: Copy + Into<usize> + TryFrom<usize>, V> Default for IntHashMap<K, V> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

pub enum Entry<'a, K, V> {
    Vec {
        value: &'a mut Option<V>,
        len: &'a mut usize,
    },
    Map(hash_map::Entry<'a, K, V>),
}

impl<'a, K, V> Entry<'a, K, V> {
    pub fn or_insert_with<F: FnOnce() -> V>(self, default: F) -> &'a mut V {
        match self {
            Entry::Vec { value, len } => {
                if value.is_none() {
                    *len += 1
                };
                value.get_or_insert_with(default)
            }
            Entry::Map(entry) => entry.or_insert_with(default),
        }
    }
}

pub enum Iter<'a, K, V> {
    Vec(slice::Iter<'a, (K, Option<V>)>),
    Map(hash_map::Iter<'a, K, V>),
}

impl<'a, K: Copy + TryFrom<usize>, V> Iterator for Iter<'a, K, V> {
    type Item = (&'a K, &'a V);

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Vec(iter) => iter
                .by_ref()
                .filter_map(|(k, v)| Some((k, v.as_ref()?)))
                .next(),
            Self::Map(iter) => iter.next(),
        }
    }
}

pub enum Values<'a, K, V> {
    Vec(slice::Iter<'a, (K, Option<V>)>),
    Map(hash_map::Values<'a, K, V>),
}
impl<'a, K, V> Iterator for Values<'a, K, V> {
    type Item = &'a V;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Vec(iter) => iter.by_ref().filter_map(|(_, v)| v.as_ref()).next(),
            Self::Map(iter) => iter.next(),
        }
    }
}

pub enum ValuesMut<'a, K, V> {
    Vec(slice::IterMut<'a, (K, Option<V>)>),
    Map(hash_map::ValuesMut<'a, K, V>),
}
impl<'a, K, V> Iterator for ValuesMut<'a, K, V> {
    type Item = &'a mut V;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Vec(iter) => iter.by_ref().filter_map(|(_, v)| v.as_mut()).next(),
            Self::Map(iter) => iter.next(),
        }
    }
}

impl<'a, K: Copy + Into<usize> + TryFrom<usize>, V> IntoIterator for &'a IntHashMap<K, V> {
    type Item = (&'a K, &'a V);
    type IntoIter = Iter<'a, K, V>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
