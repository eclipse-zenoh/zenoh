use std::{
    fmt,
    ops::{Index, IndexMut},
};

// TODO(fuzzypixelz): doc
#[derive(Debug, Clone, Copy, Default)]
pub(crate) enum BoundKind {
    #[default]
    Eastwest,
    South,
}

#[derive(Clone, Copy)]
pub(crate) struct Bound {
    pub(crate) index: usize,
    pub(crate) kind: BoundKind,
}

impl fmt::Debug for Bound {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}/{}", self.kind, self.index)
    }
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

    pub(crate) fn is_south(&self) -> bool {
        matches!(self.kind, BoundKind::South)
    }

    pub(crate) fn is_eastwest(&self) -> bool {
        matches!(self.kind, BoundKind::Eastwest)
    }
}

pub(crate) struct BoundMap<D> {
    south: Vec<D>,
    eastwest: Vec<D>,
}

impl<D> BoundMap<D> {
    pub(crate) fn iter(&self) -> impl Iterator<Item = (Bound, &D)> {
        self.eastwest
            .iter()
            .enumerate()
            .map(move |(index, d)| {
                (
                    Bound {
                        index,
                        kind: BoundKind::Eastwest,
                    },
                    d,
                )
            })
            .chain(self.south.iter().enumerate().map(move |(index, d)| {
                (
                    Bound {
                        index,
                        kind: BoundKind::South,
                    },
                    d,
                )
            }))
    }

    pub(crate) fn iter_mut(&mut self) -> impl Iterator<Item = (Bound, &mut D)> {
        self.eastwest
            .iter_mut()
            .enumerate()
            .map(move |(index, d)| {
                (
                    Bound {
                        index,
                        kind: BoundKind::Eastwest,
                    },
                    d,
                )
            })
            .chain(self.south.iter_mut().enumerate().map(move |(index, d)| {
                (
                    Bound {
                        index,
                        kind: BoundKind::South,
                    },
                    d,
                )
            }))
    }

    pub(crate) fn get(&self, bound: Bound) -> Option<&D> {
        match bound.kind {
            BoundKind::Eastwest => self.eastwest.get(bound.index),
            BoundKind::South => self.south.get(bound.index),
        }
    }

    pub(crate) fn get_mut(&mut self, bound: Bound) -> Option<&mut D> {
        match bound.kind {
            BoundKind::Eastwest => self.eastwest.get_mut(bound.index),
            BoundKind::South => self.south.get_mut(bound.index),
        }
    }

    pub(crate) fn bounds(&self) -> impl Iterator<Item = Bound> + '_ {
        self.iter().map(|(b, _)| b)
    }

    pub(crate) fn values(&self) -> impl Iterator<Item = &D> + '_ {
        self.iter().map(|(_, d)| d)
    }

    pub(crate) fn values_mut(&mut self) -> impl Iterator<Item = &mut D> + '_ {
        self.iter_mut().map(|(_, d)| d)
    }

    pub(crate) fn map<F, E>(&self, f: F) -> BoundMap<E>
    where
        F: Fn(&D) -> E,
    {
        self.iter().map(|(b, d)| (b, f(d))).collect()
    }
}

impl<D> FromIterator<(Bound, D)> for BoundMap<D> {
    fn from_iter<T: IntoIterator<Item = (Bound, D)>>(iter: T) -> Self {
        let mut bound_map = BoundMap {
            south: Vec::new(),
            eastwest: Vec::new(),
        };

        for (bound, d) in iter {
            match bound.kind {
                BoundKind::Eastwest => bound_map.eastwest.insert(bound.index, d),
                BoundKind::South => bound_map.south.insert(bound.index, d),
            }
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
