#[derive(Clone, Eq)]
enum SingleOrVecInner<T> {
    Single(T),
    Vec(Vec<T>),
}
impl<T> Default for SingleOrVecInner<T> {
    fn default() -> Self {
        SingleOrVecInner::Vec(Vec::new())
    }
}
impl<T> AsRef<[T]> for SingleOrVecInner<T> {
    fn as_ref(&self) -> &[T] {
        match self {
            SingleOrVecInner::Single(t) => std::slice::from_ref(t),
            SingleOrVecInner::Vec(t) => t,
        }
    }
}
impl<T> AsMut<[T]> for SingleOrVecInner<T> {
    fn as_mut(&mut self) -> &mut [T] {
        match self {
            SingleOrVecInner::Single(t) => std::slice::from_mut(t),
            SingleOrVecInner::Vec(t) => t,
        }
    }
}
impl<T: std::fmt::Debug> std::fmt::Debug for SingleOrVecInner<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.as_ref())
    }
}
impl<T: std::cmp::PartialEq> std::cmp::PartialEq for SingleOrVecInner<T> {
    fn eq(&self, other: &Self) -> bool {
        self.as_ref() == other.as_ref()
    }
}
impl<T> SingleOrVecInner<T> {
    fn push(&mut self, value: T) {
        match self {
            SingleOrVecInner::Vec(vec) if vec.capacity() == 0 => *self = Self::Single(value),
            SingleOrVecInner::Single(first) => unsafe {
                let first = std::ptr::read(first);
                std::ptr::write(self, Self::Vec(vec![first, value]))
            },
            SingleOrVecInner::Vec(vec) => vec.push(value),
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct SingleOrVec<T>(SingleOrVecInner<T>);
impl<T> SingleOrVec<T> {
    pub fn push(&mut self, value: T) {
        self.0.push(value)
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
}
impl<T> Default for SingleOrVec<T> {
    fn default() -> Self {
        Self(Default::default())
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
impl<T: std::fmt::Debug> std::fmt::Debug for SingleOrVec<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
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
impl<T> std::iter::Extend<T> for SingleOrVec<T> {
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        for value in iter {
            self.push(value)
        }
    }
}

pub struct IntoIter<T> {
    pub drain: std::vec::IntoIter<T>,
    pub last: Option<T>,
}
impl<T> Iterator for IntoIter<T> {
    type Item = T;
    fn next(&mut self) -> Option<Self::Item> {
        self.drain.next().or_else(|| self.last.take())
    }
}
