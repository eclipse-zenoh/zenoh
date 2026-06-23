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

/// A map from [`Region`] to values of type `D`
#[derive(Debug, Default)]
pub(crate) struct RegionMap<D> {
    /// Regions are mapped to contiguous indices as follows:
    ///
    /// ```text
    ///  Index:  0       1       2       3       4       5       6       ...
    ///          North   Local   S(0,R)  S(0,P)  S(0,C)  S(1,R)  S(1,P)  ...
    ///                           ╰──     id=0     ──╯    ╰──   id=1    ──╯
    ///
    ///  where S(id,mode) = South { id, mode }
    ///        R = Router, P = Peer, C = Client
    ///
    ///  formula: index = 2 + 3 * id + mode_offset
    ///           mode_offset: Router=0, Peer=1, Client=2
    /// ```
    buf: Vec<Option<D>>,
}

impl<D> RegionMap<D> {
    pub(crate) fn get(&self, region: &Region) -> Option<&D> {
        self.buf
            .get(Self::region_to_index(region))
            .and_then(|o| o.as_ref())
    }

    pub(crate) fn clear(&mut self) {
        self.buf.clear();
    }

    pub(crate) fn get_mut(&mut self, region: &Region) -> Option<&mut D> {
        self.buf
            .get_mut(Self::region_to_index(region))
            .and_then(|o| o.as_mut())
    }

    pub(crate) fn insert(&mut self, region: Region, value: D) -> Option<D> {
        let idx = Self::region_to_index(&region);
        if self.buf.len() < idx + 1 {
            self.buf.resize_with(idx + 1, || None);
        }
        self.buf[idx].replace(value)
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = (Region, &D)> {
        self.buf
            .iter()
            .enumerate()
            .filter_map(|(i, v)| v.as_ref().map(|v| (Self::index_to_region(i), v)))
    }

    pub(crate) fn iter_mut(&mut self) -> impl Iterator<Item = (Region, &mut D)> {
        self.buf
            .iter_mut()
            .enumerate()
            .filter_map(|(i, v)| v.as_mut().map(|v| (Self::index_to_region(i), v)))
    }

    pub(crate) fn partition_mut(&mut self, region: &Region) -> Option<(&mut D, RegionMap<&mut D>)> {
        let mut main = None;
        let mut others = vec![];
        others.resize_with(self.buf.len(), || None);

        for (i, v) in self.buf.iter_mut().enumerate() {
            if i == Self::region_to_index(region) {
                main = Some(v.as_mut().unwrap_or_else(|| unreachable!()));
            } else {
                others[i] = v.as_mut();
            }
        }

        Some((main?, RegionMap { buf: others }))
    }

    pub(crate) fn partition(&self, region: &Region) -> Option<(&D, RegionMap<&D>)> {
        let mut main = None;
        let mut others = vec![];
        others.resize_with(self.buf.len(), || None);

        for (i, v) in self.buf.iter().enumerate() {
            if i == Self::region_to_index(region) {
                main = Some(v.as_ref().unwrap_or_else(|| unreachable!()));
            } else {
                others[i] = v.as_ref();
            }
        }

        Some((main?, RegionMap { buf: others }))
    }

    pub(crate) fn regions(&self) -> impl Iterator<Item = Region> + '_ {
        self.buf
            .iter()
            .enumerate()
            .filter_map(|(i, v)| v.is_some().then_some(Self::index_to_region(i)))
    }

    pub(crate) fn values(&self) -> impl Iterator<Item = &D> + '_ {
        self.buf.iter().filter_map(|v| v.as_ref())
    }

    pub(crate) fn values_mut(&mut self) -> impl Iterator<Item = &mut D> + '_ {
        self.buf.iter_mut().filter_map(|v| v.as_mut())
    }

    pub(crate) fn map_ref<F, E>(&self, f: F) -> RegionMap<E>
    where
        F: Fn(&D) -> E,
    {
        RegionMap {
            buf: self.buf.iter().map(|d| d.as_ref().map(&f)).collect(),
        }
    }

    pub(crate) fn map<F, E>(self, f: F) -> RegionMap<E>
    where
        F: Fn(D) -> E,
    {
        RegionMap {
            buf: self.buf.into_iter().map(|d| d.map(&f)).collect(),
        }
    }

    pub(crate) fn into_iter(self) -> impl Iterator<Item = (Region, D)> {
        self.buf
            .into_iter()
            .enumerate()
            .filter_map(|(i, v)| v.map(|v| (Self::index_to_region(i), v)))
    }

    fn region_to_index(region: &Region) -> usize {
        let mode_offset = |mode: &WhatAmI| match mode {
            WhatAmI::Router => 0,
            WhatAmI::Peer => 1,
            WhatAmI::Client => 2,
        };

        match region {
            Region::North => 0,
            Region::Local => 1,
            Region::South { id, mode } => 2 + 3 * id + mode_offset(mode),
        }
    }

    fn index_to_region(idx: usize) -> Region {
        match idx {
            0 => Region::North,
            1 => Region::Local,
            n => Region::South {
                id: (n - 2) / 3,
                mode: match (n - 2) % 3 {
                    0 => WhatAmI::Router,
                    1 => WhatAmI::Peer,
                    2 => WhatAmI::Client,
                    _ => unreachable!(),
                },
            },
        }
    }
}

impl<D> FromIterator<(Region, D)> for RegionMap<D> {
    fn from_iter<T: IntoIterator<Item = (Region, D)>>(iter: T) -> Self {
        let mut res = Self { buf: vec![] };
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
        assert_eq!(
            region,
            RegionMap::<()>::index_to_region(RegionMap::<()>::region_to_index(&region))
        );
    }
}
