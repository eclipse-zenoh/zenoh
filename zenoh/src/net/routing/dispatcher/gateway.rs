use std::ops::{Index, IndexMut};

// TODO(fuzzypixelz): doc
#[derive(Debug, Clone, Copy, Default)]
pub(crate) enum BoundKind {
    South,
    #[default]
    Eastwest,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Bound {
    pub(crate) index: usize,
    pub(crate) kind: BoundKind,
}

impl Bound {
    pub(crate) const fn south0() -> Self {
        Bound {
            index: 0,
            kind: BoundKind::South,
        }
    }

    pub(crate) const fn eastwest0() -> Self {
        Bound {
            index: 0,
            kind: BoundKind::Eastwest,
        }
    }
}

pub(crate) struct BoundMap<D>([Vec<D>; 2]);

impl<D> BoundMap<D> {
    pub(crate) fn iter(&self) -> impl Iterator<Item = (Bound, &D)> {
        let bound_kind_iter = |kind| {
            self.0[kind as usize]
                .iter()
                .enumerate()
                .map(move |(index, d)| (Bound { index, kind }, d))
        };

        bound_kind_iter(BoundKind::Eastwest).chain(bound_kind_iter(BoundKind::South))
    }

    pub(crate) fn get(&self, bound: Bound) -> Option<&D> {
        self.0[bound.kind as usize].get(bound.index)
    }

    pub(crate) fn get_mut(&mut self, bound: Bound) -> Option<&mut D> {
        self.0[bound.kind as usize].get_mut(bound.index)
    }

    pub(crate) fn map<F, E>(&self, mut f: F) -> BoundMap<E>
    where
        F: FnMut(Bound) -> E,
    {
        BoundMap::from_iter(self.iter().map(|(b, _)| (b, f(b))))
    }
}

impl<D> FromIterator<(Bound, D)> for BoundMap<D> {
    fn from_iter<T: IntoIterator<Item = (Bound, D)>>(iter: T) -> Self {
        let mut bound_map = BoundMap([Vec::new(), Vec::new()]);

        for (bound, d) in iter {
            bound_map.0[bound.kind as usize].insert(bound.index, d)
        }

        bound_map
    }
}

impl<D> Index<Bound> for BoundMap<D> {
    type Output = D;

    fn index(&self, bound: Bound) -> &Self::Output {
        self.get(bound).unwrap()
    }
}

impl<D> IndexMut<Bound> for BoundMap<D> {
    fn index_mut(&mut self, bound: Bound) -> &mut Self::Output {
        self.get_mut(bound).unwrap()
    }
}
