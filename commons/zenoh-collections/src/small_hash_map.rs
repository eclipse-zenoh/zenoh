use std::{collections::hash_map, hash::Hash, mem, slice};

/// A hashmap with integer keys, with optimized storage when the set of keys is small.
///
/// Small integer key set can indeed be stored in a vector, allowing direct access instead of
/// hashmap heavy mechanics, improving performance **a lot**. If a key with a too high value for
/// direct access is inserted, then storage fallback to a regular hashmap.
/// The whole API is fully compatible with `HashMap` one.
#[derive(Debug)]
pub enum SmallHashMap<K: Copy + Into<usize> + TryFrom<usize>, V, const SMALL_SIZE: usize> {
    // Because maps can have holes, the value is optional in the vector. The key is also stored,
    // in order to provide a compatible iteration API
    Vec(Vec<(K, Option<V>)>),
    Map(ahash::HashMap<K, V>),
}

impl<K: Copy + Into<usize> + TryFrom<usize>, V, const SMALL_SIZE: usize>
    SmallHashMap<K, V, SMALL_SIZE>
{
    #[inline]
    pub fn new() -> Self {
        Self::Vec(Vec::new())
    }

    #[inline]
    pub fn iter(&self) -> Iter<K, V> {
        match self {
            Self::Vec(vec) => Iter::Vec(vec.iter()),
            Self::Map(map) => Iter::Map(map.iter()),
        }
    }

    #[inline]
    pub fn values(&self) -> Values<K, V> {
        match self {
            Self::Vec(vec) => Values::Vec(vec.iter()),
            Self::Map(map) => Values::Map(map.values()),
        }
    }

    #[inline]
    pub fn values_mut(&mut self) -> ValuesMut<K, V> {
        match self {
            Self::Vec(vec) => ValuesMut::Vec(vec.iter_mut()),
            Self::Map(map) => ValuesMut::Map(map.values_mut()),
        }
    }
}

impl<K: Copy + Into<usize> + TryFrom<usize> + Eq + Hash, V, const SMALL_SIZE: usize>
    SmallHashMap<K, V, SMALL_SIZE>
{
    #[inline]
    pub fn get(&self, k: &K) -> Option<&V> {
        match self {
            Self::Vec(vec) => vec.get((*k).into())?.1.as_ref(),
            Self::Map(map) => map.get(k),
        }
    }

    #[inline]
    pub fn get_mut(&mut self, k: &K) -> Option<&mut V> {
        match self {
            Self::Vec(vec) => vec.get_mut((*k).into())?.1.as_mut(),
            Self::Map(map) => map.get_mut(k),
        }
    }

    #[inline]
    pub fn contains_key(&self, k: &K) -> bool {
        match self {
            Self::Vec(vec) => vec.get((*k).into()).is_some(),
            Self::Map(map) => map.contains_key(k),
        }
    }

    /// Resize the map to be able to insert the given key.
    ///
    /// If its back by a vector, then it means increasing vector size to be able to use direct
    /// access with the key.
    /// If the key is too big to fit in the small vector, then the vector is collected into
    /// and replaced by a hashmap.
    fn resize(&mut self, k: K) {
        let idx = k.into();
        if let Self::Vec(vec) = self {
            if idx < SMALL_SIZE && vec.len() < idx {
                for i in vec.len()..=idx {
                    vec.push((i.try_into().unwrap_or_else(|_| unreachable!()), None));
                }
            } else {
                let map = mem::take(vec)
                    .into_iter()
                    .filter_map(|(k, v)| Some((k, v?)))
                    .collect();
                *self = Self::Map(map);
            }
        }
    }

    #[inline]
    pub fn insert(&mut self, k: K, value: V) -> Option<V> {
        self.resize(k);
        match self {
            Self::Vec(vec) => vec[k.into()].1.replace(value),
            Self::Map(map) => map.insert(k, value),
        }
    }

    #[inline]
    pub fn remove(&mut self, k: &K) -> Option<V> {
        match self {
            Self::Vec(vec) => vec.get_mut((*k).into())?.1.take(),
            Self::Map(map) => map.remove(k),
        }
    }

    #[inline]
    pub fn clear(&mut self) {
        match self {
            Self::Vec(vec) => vec.clear(),
            Self::Map(map) => map.clear(),
        }
    }

    pub fn entry(&mut self, k: K) -> Entry<K, V> {
        self.resize(k);
        match self {
            Self::Vec(vec) => Entry::Vec(&mut vec[k.into()].1),
            Self::Map(map) => Entry::Map(map.entry(k)),
        }
    }
}

impl<K: Copy + Into<usize> + TryFrom<usize>, V, const SMALL_SIZE: usize> Default
    for SmallHashMap<K, V, SMALL_SIZE>
{
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

pub enum Entry<'a, K, V> {
    Vec(&'a mut Option<V>),
    Map(hash_map::Entry<'a, K, V>),
}

impl<'a, K, V> Entry<'a, K, V> {
    pub fn or_insert_with<F: FnOnce() -> V>(self, default: F) -> &'a mut V {
        match self {
            Entry::Vec(entry) => entry.get_or_insert_with(default),
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

impl<'a, K: Copy + Into<usize> + TryFrom<usize>, V, const SMALL_SIZE: usize> IntoIterator
    for &'a SmallHashMap<K, V, SMALL_SIZE>
{
    type Item = (&'a K, &'a V);
    type IntoIter = Iter<'a, K, V>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
