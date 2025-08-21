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

use alloc::vec;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;
use core::{
    cmp::PartialEq,
    fmt, iter,
    ops::{Index, IndexMut, RangeBounds},
    ptr, slice,
};

#[derive(Clone, Eq)]
enum SingleOrVecInner<T> {
    Single(T),
    Vec(Vec<T>),
}

impl<T> SingleOrVecInner<T> {
    const fn empty() -> Self {
        SingleOrVecInner::Vec(Vec::new())
    }

    fn push(&mut self, value: T) {
        match self {
            SingleOrVecInner::Vec(vec) if vec.capacity() == 0 => *self = Self::Single(value),
            SingleOrVecInner::Single(first) => unsafe {
                let first = ptr::read(first);
                ptr::write(self, Self::Vec(vec![first, value]));
            },
            SingleOrVecInner::Vec(vec) => vec.push(value),
        }
    }
}

impl<T> PartialEq for SingleOrVecInner<T>
where
    T: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.as_ref() == other.as_ref()
    }
}

impl<T> Default for SingleOrVecInner<T> {
    fn default() -> Self {
        Self::empty()
    }
}

impl<T> AsRef<[T]> for SingleOrVecInner<T> {
    fn as_ref(&self) -> &[T] {
        match self {
            SingleOrVecInner::Single(t) => slice::from_ref(t),
            SingleOrVecInner::Vec(t) => t,
        }
    }
}

impl<T> AsMut<[T]> for SingleOrVecInner<T> {
    fn as_mut(&mut self) -> &mut [T] {
        match self {
            SingleOrVecInner::Single(t) => slice::from_mut(t),
            SingleOrVecInner::Vec(t) => t,
        }
    }
}

impl<T> fmt::Debug for SingleOrVecInner<T>
where
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.as_ref())
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct SingleOrVec<T>(SingleOrVecInner<T>);

impl<T> SingleOrVec<T> {
    pub const fn empty() -> Self {
        Self(SingleOrVecInner::empty())
    }

    pub fn push(&mut self, value: T) {
        self.0.push(value);
    }

    pub fn truncate(&mut self, len: usize) {
        if let SingleOrVecInner::Vec(v) = &mut self.0 {
            v.truncate(len);
        } else if len == 0 {
            self.0 = SingleOrVecInner::Vec(Vec::new());
        }
    }

    pub fn clear(&mut self) {
        self.truncate(0);
    }

    pub fn len(&self) -> usize {
        match &self.0 {
            SingleOrVecInner::Single(_) => 1,
            SingleOrVecInner::Vec(v) => v.len(),
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        matches!(&self.0, SingleOrVecInner::Vec(v) if v.is_empty())
    }

    fn vectorize(&mut self) -> &mut Vec<T> {
        if let SingleOrVecInner::Single(v) = &self.0 {
            unsafe {
                let v = core::ptr::read(v);
                core::ptr::write(&mut self.0, SingleOrVecInner::Vec(vec![v]))
            };
        }
        let SingleOrVecInner::Vec(v) = &mut self.0 else {
            unsafe { core::hint::unreachable_unchecked() }
        };
        v
    }

    pub fn get(&self, index: usize) -> Option<&T> {
        match &self.0 {
            SingleOrVecInner::Single(v) => (index == 0).then_some(v),
            SingleOrVecInner::Vec(v) => v.get(index),
        }
    }

    pub fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        match &mut self.0 {
            SingleOrVecInner::Single(v) => (index == 0).then_some(v),
            SingleOrVecInner::Vec(v) => v.get_mut(index),
        }
    }

    pub fn last(&self) -> Option<&T> {
        match &self.0 {
            SingleOrVecInner::Single(v) => Some(v),
            SingleOrVecInner::Vec(v) => v.last(),
        }
    }

    pub fn last_mut(&mut self) -> Option<&mut T> {
        match &mut self.0 {
            SingleOrVecInner::Single(v) => Some(v),
            SingleOrVecInner::Vec(v) => v.last_mut(),
        }
    }
    pub fn drain<Range: RangeBounds<usize>>(&mut self, range: Range) -> Drain<'_, T> {
        match &mut self.0 {
            this @ SingleOrVecInner::Single(_) if range.contains(&0) => Drain {
                inner: DrainInner::Single(this),
            },
            SingleOrVecInner::Vec(vec) => Drain {
                inner: DrainInner::Vec(vec.drain(range)),
            },
            _ => Drain {
                inner: DrainInner::Done,
            },
        }
    }
    pub fn insert(&mut self, at: usize, value: T) {
        assert!(at <= self.len());
        self.vectorize().insert(at, value);
    }
}

enum DrainInner<'a, T> {
    Vec(alloc::vec::Drain<'a, T>),
    Single(&'a mut SingleOrVecInner<T>),
    Done,
}

pub struct Drain<'a, T> {
    inner: DrainInner<'a, T>,
}

impl<T> Iterator for Drain<'_, T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        match &mut self.inner {
            DrainInner::Vec(drain) => drain.next(),
            DrainInner::Single(inner) => match unsafe { core::ptr::read(*inner) } {
                SingleOrVecInner::Single(value) => unsafe {
                    core::ptr::write(*inner, SingleOrVecInner::Vec(Vec::new()));
                    Some(value)
                },
                SingleOrVecInner::Vec(_) => None,
            },
            _ => None,
        }
    }
}
impl<T> Drop for Drain<'_, T> {
    fn drop(&mut self) {
        if let DrainInner::Single(_) = self.inner {
            self.next();
        }
    }
}

impl<T> Default for SingleOrVec<T> {
    fn default() -> Self {
        Self(SingleOrVecInner::default())
    }
}

impl<T> AsRef<[T]> for SingleOrVec<T> {
    fn as_ref(&self) -> &[T] {
        self.0.as_ref()
    }
}

impl<T> AsMut<[T]> for SingleOrVec<T> {
    fn as_mut(&mut self) -> &mut [T] {
        self.0.as_mut()
    }
}

impl<T> fmt::Debug for SingleOrVec<T>
where
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl<T> IntoIterator for SingleOrVec<T> {
    type Item = T;
    type IntoIter = IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        match self.0 {
            SingleOrVecInner::Single(first) => IntoIter {
                last: Some(first),
                drain: Vec::new().into_iter(),
            },
            SingleOrVecInner::Vec(v) => {
                let mut it = v.into_iter();
                IntoIter {
                    last: it.next_back(),
                    drain: it,
                }
            }
        }
    }
}

impl<T> iter::Extend<T> for SingleOrVec<T> {
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        for value in iter {
            self.push(value);
        }
    }
}

pub struct IntoIter<T> {
    pub drain: alloc::vec::IntoIter<T>,
    pub last: Option<T>,
}

impl<T> Iterator for IntoIter<T> {
    type Item = T;
    fn next(&mut self) -> Option<Self::Item> {
        self.drain.next().or_else(|| self.last.take())
    }
}

impl<T> Index<usize> for SingleOrVec<T> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        &self.as_ref()[index]
    }
}

impl<T> IndexMut<usize> for SingleOrVec<T> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.as_mut()[index]
    }
}
