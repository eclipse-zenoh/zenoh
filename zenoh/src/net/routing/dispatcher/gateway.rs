use std::{
    collections::HashMap,
    fmt::Debug,
    ops::{Index, IndexMut},
};

/// Region identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Bound {
    /// The gateway exit point; there is only one such bound.
    North,
    /// A sub-region within the south-bound region; there is zero or
    /// more such bound.
    South { index: usize },
    /// A special bound for faces that neither belong to the south-bound nor the
    /// north-bound regions; there is zero or more such bound.
    Eastwest { index: usize },
}

impl Bound {
    /// Returns a gateway [`Bound`] for session clients.
    pub(crate) const fn session() -> Self {
        Self::North
    }

    pub(crate) const fn south(index: usize) -> Self {
        Self::South { index }
    }

    pub(crate) const fn eastwest(index: usize) -> Self {
        Bound::Eastwest { index }
    }

    pub(crate) fn is_south(&self) -> bool {
        matches!(self, Bound::South { .. })
    }

    pub(crate) fn is_eastwest(&self) -> bool {
        matches!(self, Bound::Eastwest { .. })
    }

    pub(crate) fn is_north(&self) -> bool {
        matches!(self, Bound::North)
    }
}

// TODO(regions): optimization
#[derive(Debug)]
pub(crate) struct BoundMap<D>(HashMap<Bound, D>);

impl<D> BoundMap<D> {
    pub(crate) fn iter(&self) -> impl Iterator<Item = (&Bound, &D)> {
        self.0.iter()
    }

    pub(crate) fn iter_mut(&mut self) -> impl Iterator<Item = (&Bound, &mut D)> {
        self.0.iter_mut()
    }

    pub(crate) fn north(&self) -> &D {
        let mut iter = self.iter().filter(|(b, _)| b.is_north());
        let (_, north) = iter.next().unwrap();
        assert!(iter.next().is_none());
        north
    }

    pub(crate) fn non_north_iter(&self) -> impl Iterator<Item = (&Bound, &D)> {
        self.iter().filter(|(b, _)| !b.is_north())
    }

    pub(crate) fn non_north_iter_mut(&mut self) -> impl Iterator<Item = (&Bound, &mut D)> {
        self.iter_mut().filter(|(b, _)| !b.is_north())
    }

    pub(crate) fn bounds(&self) -> impl Iterator<Item = &Bound> + '_ {
        self.0.keys()
    }

    pub(crate) fn values(&self) -> impl Iterator<Item = &D> + '_ {
        self.0.values()
    }

    pub(crate) fn values_mut(&mut self) -> impl Iterator<Item = &mut D> + '_ {
        self.0.values_mut()
    }

    pub(crate) fn map<F, E>(&self, f: F) -> BoundMap<E>
    where
        F: Fn(&D) -> E,
    {
        BoundMap(self.iter().map(|(b, d)| (*b, f(d))).collect())
    }
}

impl<D> FromIterator<(Bound, D)> for BoundMap<D> {
    fn from_iter<T: IntoIterator<Item = (Bound, D)>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl<D> Index<&Bound> for BoundMap<D> {
    type Output = D;

    fn index(&self, bound: &Bound) -> &Self::Output {
        self.0.index(bound)
    }
}

// REVIEW(regions): do we need two `Index` impls?
impl<D> Index<Bound> for BoundMap<D> {
    type Output = D;

    fn index(&self, bound: Bound) -> &Self::Output {
        self.index(&bound)
    }
}

impl<D> IndexMut<&Bound> for BoundMap<D> {
    fn index_mut(&mut self, bound: &Bound) -> &mut Self::Output {
        self.0.get_mut(bound).unwrap()
    }
}

// REVIEW(regions): do we need two `IndexMut` impls?
impl<D> IndexMut<Bound> for BoundMap<D> {
    fn index_mut(&mut self, bound: Bound) -> &mut Self::Output {
        self.index_mut(&bound)
    }
}
