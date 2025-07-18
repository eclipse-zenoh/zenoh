use std::{collections::hash_map, hash::Hash, mem, slice};

#[derive(Debug)]
pub enum SmallHashMap<K: Copy + Into<usize> + TryFrom<usize>, V, const SMALL_SIZE: usize> {
    Vec {
        vec: Vec<(K, Option<V>)>,
        // use usize for len/first increase the size of SmallHashMap, while u16 is fine here
        len: u16,
        first: Option<u16>,
    },
    Map(ahash::HashMap<K, V>),
}

impl<K: Copy + Into<usize> + TryFrom<usize>, V, const SMALL_SIZE: usize>
    SmallHashMap<K, V, SMALL_SIZE>
{
    #[inline]
    pub fn new() -> Self {
        Self::Vec {
            vec: Vec::new(),
            len: 0,
            first: None,
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        match self {
            Self::Vec { len, .. } => *len as _,
            Self::Map(m) => m.len(),
        }
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[inline]
    pub fn iter(&self) -> Iter<K, V> {
        match self {
            Self::Vec {
                vec,
                first: Some(first),
                ..
            } => Iter::Vec(unsafe { vec.get_unchecked(*first as usize..) }.iter()),
            Self::Vec { .. } => Iter::Vec([].iter()),
            Self::Map(map) => Iter::Map(map.iter()),
        }
    }

    #[inline]
    pub fn values(&self) -> Values<K, V> {
        match self {
            Self::Vec {
                vec,
                first: Some(first),
                ..
            } => Values::Vec(unsafe { vec.get_unchecked(*first as usize..) }.iter()),
            Self::Vec { .. } => Values::Vec([].iter()),
            Self::Map(map) => Values::Map(map.values()),
        }
    }

    #[inline]
    pub fn values_mut(&mut self) -> ValuesMut<K, V> {
        match self {
            Self::Vec {
                vec,
                first: Some(first),
                ..
            } => ValuesMut::Vec(unsafe { vec.get_unchecked_mut(*first as usize..) }.iter_mut()),
            Self::Vec { .. } => ValuesMut::Vec([].iter_mut()),
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
            Self::Map(map) => map.contains_key(&k),
        }
    }

    fn resize(&mut self, k: K) {
        let idx = k.into();
        match self {
            Self::Vec { vec, .. } if idx < SMALL_SIZE && vec.len() < idx => {
                for i in vec.len()..=idx {
                    vec.push((i.try_into().unwrap_or_else(|_| unreachable!()), None));
                }
            }
            Self::Vec { vec, .. } => {
                let map = mem::take(vec)
                    .into_iter()
                    .filter_map(|(k, v)| Some((k, v?)))
                    .collect();
                *self = Self::Map(map);
            }
            _ => {}
        }
    }

    #[inline]
    pub fn insert(&mut self, k: K, value: V) {
        self.resize(k);
        match self {
            Self::Vec { vec, len, first } => {
                if vec[k.into()].1.replace(value).is_none() {
                    if first.is_none() || matches!(first, Some(f) if *f< k.into() as u16) {
                        *first = Some(k.into() as u16)
                    }
                    *len += 1;
                }
            }
            Self::Map(map) => {
                map.insert(k, value);
            }
        }
    }

    #[inline]
    pub fn remove(&mut self, k: &K) -> Option<V> {
        match self {
            Self::Vec { vec, len, first } => {
                let prev = vec.get_mut((*k).into())?.1.take();
                if prev.is_some() {
                    *len -= 1;
                    if *len == 0 {
                        *first = None;
                    } else if first.unwrap() == (*k).into() as u16 {
                        *first = vec.iter().position(|(_, v)| v.is_some()).map(|f| f as u16);
                    }
                }
                prev
            }
            Self::Map(map) => map.remove(&k),
        }
    }

    #[inline]
    pub fn clear(&mut self) {
        match self {
            Self::Vec { vec, len, first } => {
                vec.clear();
                *len = 0;
                *first = None;
            }
            Self::Map(map) => {
                map.clear();
                *self = Self::new();
            }
        }
    }

    pub fn entry(&mut self, k: K) -> Entry<K, V> {
        self.resize(k);
        match self {
            Self::Vec { vec, len, first } => Entry::Vec {
                idx: k.into(),
                value: &mut vec[k.into()].1,
                len,
                first,
            },
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
    Vec {
        idx: usize,
        value: &'a mut Option<V>,
        len: &'a mut u16,
        first: &'a mut Option<u16>,
    },
    Map(hash_map::Entry<'a, K, V>),
}

impl<'a, K, V> Entry<'a, K, V> {
    pub fn or_insert_with<F: FnOnce() -> V>(self, default: F) -> &'a mut V {
        match self {
            Entry::Vec {
                idx,
                value,
                len,
                first,
            } => {
                if value.is_none() {
                    *len += 1;
                    if first.is_none() || matches!(first, Some(f) if *f< idx as u16) {
                        *first = Some(idx as u16)
                    }
                }
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
            Self::Vec(iter) => iter.next().and_then(|(k, v)| Some((k, v.as_ref()?))),
            Self::Map(iter) => iter.next().map(|(k, v)| (k, v)),
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        match self {
            Self::Vec(iter) => (0, Some(iter.len())),
            Self::Map(iter) => iter.size_hint(),
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
            Self::Vec(iter) => iter.next()?.1.as_ref(),
            Self::Map(iter) => iter.next(),
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        match self {
            Self::Vec(iter) => (0, Some(iter.len())),
            Self::Map(iter) => iter.size_hint(),
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
            Self::Vec(iter) => iter.next()?.1.as_mut(),
            Self::Map(iter) => iter.next(),
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        match self {
            Self::Vec(iter) => (0, Some(iter.len())),
            Self::Map(iter) => iter.size_hint(),
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
