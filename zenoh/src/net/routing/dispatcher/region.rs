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

use zenoh_config::WhatAmI;
use zenoh_protocol::core::Region;

fn region_index(region: &Region) -> usize {
    match region {
        Region::North => 0,
        Region::Local => 1,
        Region::South { id, mode } => {
            id * 3
                + 2
                + match mode {
                    WhatAmI::Router => 0,
                    WhatAmI::Peer => 1,
                    WhatAmI::Client => 2,
                }
        }
    }
}

fn from_index(idx: usize) -> Region {
    match idx {
        0 => Region::North,
        1 => Region::Local,
        n => Region::South {
            id: (n - 2) / 3,
            mode: match (n - 2) % 3 {
                0 => WhatAmI::Router,
                1 => WhatAmI::Peer,
                _ => WhatAmI::Client,
            },
        },
    }
}

#[derive(Debug, Default)]
pub(crate) struct RegionMap<D>(Vec<Option<D>>);

impl<D> RegionMap<D> {
    pub(crate) fn get(&self, region: &Region) -> Option<&D> {
        self.0.get(region_index(region)).and_then(|o| o.as_ref())
    }

    pub(crate) fn clear(&mut self) {
        self.0.clear();
    }

    pub(crate) fn get_mut(&mut self, region: &Region) -> Option<&mut D> {
        self.0
            .get_mut(region_index(region))
            .and_then(|o| o.as_mut())
    }

    pub(crate) fn insert(&mut self, region: Region, value: D) -> Option<D> {
        let idx = region_index(&region);
        if self.0.len() < idx + 1 {
            self.0.resize_with(idx + 1, || None);
        }
        self.0[idx].replace(value)
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = (Region, &D)> {
        self.0
            .iter()
            .enumerate()
            .filter_map(|(i, v)| v.as_ref().map(|v| (from_index(i), v)))
    }

    pub(crate) fn iter_mut(&mut self) -> impl Iterator<Item = (Region, &mut D)> {
        self.0
            .iter_mut()
            .enumerate()
            .filter_map(|(i, v)| v.as_mut().map(|v| (from_index(i), v)))
    }

    pub(crate) fn partition_mut(&mut self, region: &Region) -> (&mut D, RegionMap<&mut D>) {
        let mut main = None;
        let mut others = vec![];
        others.resize_with(self.0.len(), || None);

        for (i, v) in self.0.iter_mut().enumerate() {
            if i == region_index(region) {
                main = Some(v.as_mut().unwrap_or_else(|| unreachable!()));
            } else {
                others[i] = v.as_mut();
            }
        }

        let Some(north) = main else { unreachable!() };

        (north, RegionMap(others))
    }

    pub(crate) fn partition(&self, region: &Region) -> (&D, RegionMap<&D>) {
        let mut main = None;
        let mut others = vec![];
        others.resize_with(self.0.len(), || None);

        for (i, v) in self.0.iter().enumerate() {
            if i == region_index(region) {
                main = Some(v.as_ref().unwrap_or_else(|| unreachable!()));
            } else {
                others[i] = v.as_ref();
            }
        }

        let Some(north) = main else { unreachable!() };

        (north, RegionMap(others))
    }

    pub(crate) fn regions(&self) -> impl Iterator<Item = Region> + '_ {
        self.0
            .iter()
            .enumerate()
            .filter_map(|(i, v)| v.is_some().then_some(from_index(i)))
    }

    pub(crate) fn values(&self) -> impl Iterator<Item = &D> + '_ {
        self.0.iter().filter_map(|v| v.as_ref())
    }

    pub(crate) fn values_mut(&mut self) -> impl Iterator<Item = &mut D> + '_ {
        self.0.iter_mut().filter_map(|v| v.as_mut())
    }

    pub(crate) fn map_ref<F, E>(&self, f: F) -> RegionMap<E>
    where
        F: Fn(&D) -> E,
    {
        RegionMap(self.0.iter().map(|d| d.as_ref().map(&f)).collect())
    }

    pub(crate) fn map<F, E>(self, f: F) -> RegionMap<E>
    where
        F: Fn(D) -> E,
    {
        RegionMap(self.0.into_iter().map(|d| d.map(&f)).collect())
    }

    pub(crate) fn into_iter(self) -> impl Iterator<Item = (Region, D)> {
        self.0
            .into_iter()
            .enumerate()
            .filter_map(|(i, v)| v.map(|v| (from_index(i), v)))
    }
}

impl<D> FromIterator<(Region, D)> for RegionMap<D> {
    fn from_iter<T: IntoIterator<Item = (Region, D)>>(iter: T) -> Self {
        let mut res = Self(vec![]);
        for (r, d) in iter.into_iter() {
            res.insert(r, d);
        }
        res
    }
}

impl<D> Index<&Region> for RegionMap<D> {
    type Output = D;

    fn index(&self, region: &Region) -> &Self::Output {
        self.get(region).unwrap()
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
        self.get_mut(region).unwrap()
    }
}

// REVIEW(regions): do we need two `IndexMut` impls?
impl<D> IndexMut<Region> for RegionMap<D> {
    fn index_mut(&mut self, region: Region) -> &mut Self::Output {
        self.index_mut(&region)
    }
}

#[test]
fn region_indexes() {
    let regions = [
        Region::North,
        Region::Local,
        Region::South {
            id: 0,
            mode: WhatAmI::Router,
        },
        Region::South {
            id: 3,
            mode: WhatAmI::Peer,
        },
        Region::South {
            id: 35,
            mode: WhatAmI::Client,
        },
    ];
    for region in regions {
        assert_eq!(region, from_index(region_index(&region)));
    }
}
