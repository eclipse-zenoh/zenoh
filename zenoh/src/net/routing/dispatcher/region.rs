use std::{
    fmt::{Debug, Display},
    ops::{Index, IndexMut},
};

use zenoh_config::WhatAmI;
pub(crate) use zenoh_transport::Bound;

/// Region identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum Region {
    /// Main region.
    North,
    /// Subregion of local sessions.
    Local,
    /// Subregion of remotes with no user-defined subregion.
    Undefined { mode: WhatAmI }, // REVIEW(regions): call this "unbound" even though it's effectively south-bound?
    /// User-defined subregions.
    Subregion { id: usize, mode: WhatAmI },
}

impl Region {
    pub(crate) fn bound(&self) -> Bound {
        match self {
            Region::North => Bound::North,
            Region::Local | Region::Undefined { .. } | Region::Subregion { .. } => Bound::South,
        }
    }
}

impl Display for Region {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Region::North => f.write_str("N"),
            Region::Local => f.write_str("L"),
            Region::Undefined { mode } => write!(f, "U/{}", mode.short()),
            Region::Subregion { id, mode } => write!(f, "S{}/{}", id, mode.short()),
        }
    }
}

// TODO(regions): optimization
#[derive(Debug, Default)]
pub(crate) struct RegionMap<D>(hashbrown::HashMap<Region, D>);

impl<D> RegionMap<D> {
    #[allow(dead_code)] // FIXME(regions)
    pub(crate) fn get_many_mut<const N: usize>(&mut self, ks: [&Region; N]) -> [Option<&mut D>; N] {
        self.0.get_many_mut(ks)
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = (&Region, &D)> {
        self.0.iter()
    }

    pub(crate) fn iter_mut(&mut self) -> impl Iterator<Item = (&Region, &mut D)> {
        self.0.iter_mut()
    }

    pub(crate) fn north(&self) -> &D {
        let mut iter = self.iter().filter(|(b, _)| b.bound().is_north());
        let (_, north) = iter.next().unwrap();
        assert!(iter.next().is_none());
        north
    }

    #[allow(dead_code)] // FIXME(regions)
    pub(crate) fn north_mut(&mut self) -> &mut D {
        let mut iter = self.iter_mut().filter(|(b, _)| b.bound().is_north());
        let (_, north) = iter.next().unwrap();
        assert!(iter.next().is_none());
        north
    }

    pub(crate) fn partition_north_mut(&mut self) -> (&mut D, RegionMap<&mut D>) {
        self.partition_mut(&Region::North)
    }

    pub(crate) fn partition_mut(&mut self, region: &Region) -> (&mut D, RegionMap<&mut D>) {
        let (mut main, others) = self
            .0
            .iter_mut()
            .map(|(b, d)| (*b, d))
            .partition::<hashbrown::HashMap<_, _>, _>(|(r, _)| r == region);

        let Some(north) = main.remove(region) else {
            unreachable!()
        };

        (north, RegionMap(others))
    }

    pub(crate) fn regions(&self) -> impl Iterator<Item = &Region> + '_ {
        self.0.keys()
    }

    pub(crate) fn values(&self) -> impl Iterator<Item = &D> + '_ {
        self.0.values()
    }

    pub(crate) fn values_mut(&mut self) -> impl Iterator<Item = &mut D> + '_ {
        self.0.values_mut()
    }

    pub(crate) fn map_ref<F, E>(&self, f: F) -> RegionMap<E>
    where
        F: Fn(&D) -> E,
    {
        RegionMap(self.iter().map(|(b, d)| (*b, f(d))).collect())
    }

    pub(crate) fn map<F, E>(self, f: F) -> RegionMap<E>
    where
        F: Fn(D) -> E,
    {
        RegionMap(self.into_iter().map(|(b, d)| (b, f(d))).collect())
    }

    pub(crate) fn into_iter(self) -> impl Iterator<Item = (Region, D)> {
        self.0.into_iter()
    }
}

impl<D> FromIterator<(Region, D)> for RegionMap<D> {
    fn from_iter<T: IntoIterator<Item = (Region, D)>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl<D> Index<&Region> for RegionMap<D> {
    type Output = D;

    fn index(&self, region: &Region) -> &Self::Output {
        self.0.index(region)
    }
}

// REVIEW(regions): do we need two `Index` impls?
impl<D> Index<Region> for RegionMap<D> {
    type Output = D;

    fn index(&self, region: Region) -> &Self::Output {
        self.index(&region)
    }
}

impl<D> IndexMut<&Region> for RegionMap<D> {
    fn index_mut(&mut self, region: &Region) -> &mut Self::Output {
        self.0.get_mut(region).unwrap()
    }
}

// REVIEW(regions): do we need two `IndexMut` impls?
impl<D> IndexMut<Region> for RegionMap<D> {
    fn index_mut(&mut self, region: Region) -> &mut Self::Output {
        self.index_mut(&region)
    }
}
