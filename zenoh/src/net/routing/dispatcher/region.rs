//
// Copyright (c) 2026 ZettaScale Technology
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
use std::{
    fmt::Debug,
    ops::{Index, IndexMut},
};

use zenoh_protocol::core::Region;

// TODO(regions): optimization
#[derive(Debug, Default)]
pub(crate) struct RegionMap<D>(hashbrown::HashMap<Region, D>);

impl<D> RegionMap<D> {
    pub(crate) fn iter(&self) -> impl Iterator<Item = (&Region, &D)> {
        self.0.iter()
    }

    pub(crate) fn iter_mut(&mut self) -> impl Iterator<Item = (&Region, &mut D)> {
        self.0.iter_mut()
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

    pub(crate) fn partition(&self, region: &Region) -> (&D, RegionMap<&D>) {
        let (mut main, others) = self
            .0
            .iter()
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
