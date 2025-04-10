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

use core::{borrow::Borrow, mem};
use std::{collections::hash_set, fmt, hash::Hash};

#[allow(clippy::box_collection)]
pub enum SingleOrBoxHashSet<T> {
    Empty,
    Single(T),
    Set(Box<ahash::HashSet<T>>),
}

impl<T> Default for SingleOrBoxHashSet<T>
where
    T: Eq + Hash,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<T> fmt::Debug for SingleOrBoxHashSet<T>
where
    T: fmt::Debug + Eq + Hash,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_set().entries(self.iter()).finish()
    }
}

impl<T> SingleOrBoxHashSet<T>
where
    T: Eq + Hash,
{
    #[inline]
    pub fn new() -> Self {
        SingleOrBoxHashSet::Empty
    }
    #[inline]
    pub fn insert(&mut self, v: T) -> bool {
        match self {
            SingleOrBoxHashSet::Empty => {
                *self = SingleOrBoxHashSet::Single(v);
                true
            }
            SingleOrBoxHashSet::Single(single) => {
                if *single == v {
                    *self = SingleOrBoxHashSet::Single(v);
                    false
                } else {
                    let mut swap = SingleOrBoxHashSet::Set(Box::default());
                    std::mem::swap(self, &mut swap);
                    if let SingleOrBoxHashSet::Set(set) = self {
                        if let SingleOrBoxHashSet::Single(single) = swap {
                            set.insert(single);
                            set.insert(v);
                            return true;
                        }
                    }
                    unreachable!()
                }
            }
            SingleOrBoxHashSet::Set(set) => {
                if set.is_empty() {
                    *self = SingleOrBoxHashSet::Single(v);
                    true
                } else {
                    set.insert(v)
                }
            }
        }
    }
    pub fn contains<Q>(&self, v: &Q) -> bool
    where
        T: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        match self {
            SingleOrBoxHashSet::Empty => false,
            SingleOrBoxHashSet::Single(single) => Borrow::<Q>::borrow(single) == v,
            SingleOrBoxHashSet::Set(set) => set.contains(v),
        }
    }
    pub fn is_disjoint(&self, other: &SingleOrBoxHashSet<T>) -> bool {
        if self.len() <= other.len() {
            self.iter().all(|v| !other.contains(v))
        } else {
            other.iter().all(|v| !self.contains(v))
        }
    }

    pub fn is_subset(&self, other: &SingleOrBoxHashSet<T>) -> bool {
        if self.len() <= other.len() {
            self.iter().all(|v| other.contains(v))
        } else {
            false
        }
    }

    #[inline]
    pub fn is_superset(&self, other: &SingleOrBoxHashSet<T>) -> bool {
        other.is_subset(self)
    }

    #[inline]
    pub fn get<Q>(&self, v: &Q) -> Option<&T>
    where
        T: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        match self {
            SingleOrBoxHashSet::Empty => None,
            SingleOrBoxHashSet::Single(single) => {
                (Borrow::<Q>::borrow(single) == v).then_some(single)
            }
            SingleOrBoxHashSet::Set(set) => set.get(v),
        }
    }
    #[inline]
    pub fn is_empty(&self) -> bool {
        matches!(self, SingleOrBoxHashSet::Empty)
    }
    #[inline]
    pub fn len(&self) -> usize {
        match self {
            SingleOrBoxHashSet::Empty => 0,
            SingleOrBoxHashSet::Single(_) => 1,
            SingleOrBoxHashSet::Set(set) => set.len(),
        }
    }
    #[inline]
    pub fn iter(&self) -> Iter<'_, T> {
        match self {
            SingleOrBoxHashSet::Empty => Iter::Empty,
            SingleOrBoxHashSet::Single(single) => Iter::Single(single),
            SingleOrBoxHashSet::Set(set) => Iter::SetIter(set.iter()),
        }
    }
    pub fn replace(&mut self, v: T) -> Option<T> {
        match self {
            SingleOrBoxHashSet::Empty => {
                *self = SingleOrBoxHashSet::Single(v);
                None
            }
            SingleOrBoxHashSet::Single(single) => {
                if *single == v {
                    let mut swap = SingleOrBoxHashSet::Single(v);
                    mem::swap(self, &mut swap);
                    if let SingleOrBoxHashSet::Single(single) = swap {
                        Some(single)
                    } else {
                        None
                    }
                } else {
                    self.insert(v);
                    None
                }
            }
            SingleOrBoxHashSet::Set(set) => set.replace(v),
        }
    }
    #[inline]
    pub fn remove<Q>(&mut self, v: &Q) -> bool
    where
        T: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        match self {
            SingleOrBoxHashSet::Empty => false,
            SingleOrBoxHashSet::Single(single) => {
                if Borrow::<Q>::borrow(single) == v {
                    *self = SingleOrBoxHashSet::Empty;
                    true
                } else {
                    false
                }
            }
            SingleOrBoxHashSet::Set(set) => {
                let res = set.remove(v);
                if set.len() == 1 {
                    let rem = set.drain().next().unwrap();
                    *self = SingleOrBoxHashSet::Single(rem);
                }
                res
            }
        }
    }
    #[inline]
    pub fn drain(&mut self) -> impl Iterator<Item = T> + '_ {
        match self {
            SingleOrBoxHashSet::Set(set) => Drain::Set(set.drain()),
            _ => Drain::Single(self),
        }
    }
    #[inline]
    pub fn clear(&mut self) {
        *self = SingleOrBoxHashSet::Empty;
    }
}

impl<T> PartialEq for SingleOrBoxHashSet<T>
where
    T: Eq + Hash,
{
    fn eq(&self, other: &SingleOrBoxHashSet<T>) -> bool {
        if self.len() != other.len() {
            return false;
        }

        self.iter().all(|key| other.contains(key))
    }
}

impl<T> Eq for SingleOrBoxHashSet<T> where T: Eq + Hash {}

impl<T> FromIterator<T> for SingleOrBoxHashSet<T>
where
    T: Eq + Hash,
{
    #[inline]
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> SingleOrBoxHashSet<T> {
        let mut set = SingleOrBoxHashSet::default();
        set.extend(iter);
        set
    }
}

impl<T> Extend<T> for SingleOrBoxHashSet<T>
where
    T: Eq + Hash,
{
    #[inline]
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        for i in iter.into_iter() {
            self.insert(i);
        }
    }
}

impl<'a, T> Extend<&'a T> for SingleOrBoxHashSet<T>
where
    T: 'a + Eq + Hash + Copy,
{
    #[inline]
    fn extend<I: IntoIterator<Item = &'a T>>(&mut self, iter: I) {
        self.extend(iter.into_iter().cloned());
    }
}

pub enum Iter<'a, T> {
    Empty,
    Single(&'a T),
    SetIter(hash_set::Iter<'a, T>),
}

impl<'a, T> Iterator for Iter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Iter::Empty => None,
            Iter::Single(_) => {
                let mut swap = Iter::Empty;
                mem::swap(self, &mut swap);
                if let Iter::Single(single) = swap {
                    Some(single)
                } else {
                    None
                }
            }
            Iter::SetIter(iter) => iter.next(),
        }
    }
}

enum Drain<'a, T> {
    Single(&'a mut SingleOrBoxHashSet<T>),
    Set(hash_set::Drain<'a, T>),
}

impl<T> Iterator for Drain<'_, T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Drain::Single(single) => {
                let mut swap = SingleOrBoxHashSet::Empty;
                mem::swap(*single, &mut swap);
                match swap {
                    SingleOrBoxHashSet::Single(single) => Some(single),
                    _ => None,
                }
            }
            Drain::Set(drain) => drain.next(),
        }
    }
}

impl<T> Drop for Drain<'_, T> {
    fn drop(&mut self) {
        if let Drain::Single(single) = self {
            **single = SingleOrBoxHashSet::Empty;
        }
    }
}

pub enum IntoIter<T> {
    Empty,
    Single(T),
    Set(hash_set::IntoIter<T>),
}

impl<T> Iterator for IntoIter<T> {
    type Item = T;
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            IntoIter::Empty | IntoIter::Single(_) => {
                let mut swap = IntoIter::Empty;
                mem::swap(self, &mut swap);
                if let IntoIter::Single(single) = swap {
                    Some(single)
                } else {
                    None
                }
            }
            IntoIter::Set(set) => set.next(),
        }
    }
}

impl<'a, T> IntoIterator for &'a SingleOrBoxHashSet<T>
where
    T: Eq + Hash,
{
    type Item = &'a T;
    type IntoIter = Iter<'a, T>;

    #[inline]
    fn into_iter(self) -> Iter<'a, T> {
        self.iter()
    }
}

impl<T> IntoIterator for SingleOrBoxHashSet<T> {
    type Item = T;
    type IntoIter = IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        match self {
            SingleOrBoxHashSet::Empty => IntoIter::Empty,
            SingleOrBoxHashSet::Single(first) => IntoIter::Single(first),
            SingleOrBoxHashSet::Set(set) => IntoIter::Set(set.into_iter()),
        }
    }
}
