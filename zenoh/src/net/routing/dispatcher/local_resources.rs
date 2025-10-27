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

use std::{
    collections::{HashMap, HashSet},
    hash::Hash,
    ops::Deref,
    sync::Arc,
};

use zenoh_protocol::network::interest::InterestId;

use crate::net::routing::router::Resource;

pub(crate) trait LocalResourceTrait: Hash + Clone + Eq {
    fn matches(&self, other: &Self) -> bool;
}

pub(crate) trait LocalResourceInfoTrait<Res: LocalResourceTrait>
where
    Self: Sized + Eq + Clone + Copy,
{
    fn aggregate(self_val: Option<Self>, self_res: &Res, other_val: &Self, other_res: &Res)
        -> Self;

    fn aggregate_many<'a>(
        self_res: &'a Res,
        iter: impl Iterator<Item = (&'a Res, Self)>,
    ) -> Option<Self> {
        let mut out = None;
        for (res, val) in iter {
            out = Some(Self::aggregate(out, self_res, &val, res));
        }
        out
    }
}

struct ResourceData<Id: Copy, Res: LocalResourceTrait, Info: LocalResourceInfoTrait<Res>> {
    id: Id,
    aggregated_to: HashSet<Res>,
    simple_interest_ids: HashSet<InterestId>,
    info: Info,
}

struct AggregatedResourceData<Id: Copy, Res: LocalResourceTrait, Info: LocalResourceInfoTrait<Res>>
{
    id: Id,
    aggregates: HashSet<Res>,
    aggregated_interest_ids: HashSet<InterestId>,
    info: Option<Info>,
}

impl<Id: Copy, Res: LocalResourceTrait, Info: LocalResourceInfoTrait<Res>>
    AggregatedResourceData<Id, Res, Info>
{
    fn recompute_info(
        &self,
        self_res: &Res,
        subs: &HashMap<Res, ResourceData<Id, Res, Info>>,
    ) -> Option<Info> {
        let iter = self
            .aggregates
            .iter()
            .map(|r| (r, subs.get(r).unwrap().info));
        Info::aggregate_many(self_res, iter)
    }
}

pub(crate) struct LocalResourceRemoveResult<
    Id: Copy,
    Res: LocalResourceTrait,
    Info: LocalResourceInfoTrait<Res>,
> {
    pub(crate) id: Id,
    pub(crate) resource: Res,
    pub(crate) update: Option<Info>,
}

pub(crate) struct LocalResourceInsertResult<
    Id: Copy,
    Res: LocalResourceTrait,
    Info: LocalResourceInfoTrait<Res>,
> {
    pub(crate) id: Id,
    pub(crate) resource: Res,
    pub(crate) info: Info,
}

pub(crate) struct LocalResources<
    Id: Copy,
    Res: LocalResourceTrait,
    Info: LocalResourceInfoTrait<Res>,
> {
    simple_resources: HashMap<Res, ResourceData<Id, Res, Info>>,
    aggregated_resources: HashMap<Res, AggregatedResourceData<Id, Res, Info>>,
}

impl<Id: Copy, Res: LocalResourceTrait, Info: LocalResourceInfoTrait<Res>>
    LocalResources<Id, Res, Info>
{
    pub(crate) fn new() -> Self {
        Self {
            simple_resources: HashMap::new(),
            aggregated_resources: HashMap::new(),
        }
    }

    pub(crate) fn contains_simple_resource(&self, key: &Res) -> bool {
        self.simple_resources.contains_key(key)
    }

    pub(crate) fn simple_resources(&self) -> impl Iterator<Item = &Res> {
        self.simple_resources.keys()
    }

    // Returns Id of newly inserted resource and the list of resources that were enabled/changed info by this operation
    pub(crate) fn insert_simple_resource<F>(
        &mut self,
        key: Res,
        info: Info,
        f_id: F,
        simple_interests: HashSet<InterestId>,
    ) -> (Id, Vec<LocalResourceInsertResult<Id, Res, Info>>)
    where
        F: FnOnce() -> Id,
    {
        let mut updated_resources = Vec::new();
        match self.simple_resources.entry(key.clone()) {
            std::collections::hash_map::Entry::Occupied(mut occupied_entry) => {
                {
                    let s_res_data = occupied_entry.get_mut();
                    s_res_data.simple_interest_ids.extend(simple_interests);
                    if !s_res_data.simple_interest_ids.is_empty() && s_res_data.info != info {
                        updated_resources.push(LocalResourceInsertResult {
                            id: s_res_data.id,
                            resource: key.clone(),
                            info,
                        });
                    }
                    s_res_data.info = info;
                };
                let s_res_data = self.simple_resources.get(&key).unwrap(); // reborrow as shared ref

                for a_res in &s_res_data.aggregated_to {
                    if let Some(a_res_data) = self.aggregated_resources.get_mut(a_res) {
                        let new_info = a_res_data.recompute_info(a_res, &self.simple_resources);
                        if new_info != a_res_data.info {
                            a_res_data.info = new_info;
                            updated_resources.push(LocalResourceInsertResult {
                                id: a_res_data.id,
                                resource: a_res.clone(),
                                info: new_info.unwrap(), // aggregated resource contains at least one simple - so it is guaranteed to have an initialized info
                            });
                        }
                    }
                }
                (s_res_data.id, updated_resources)
            }
            std::collections::hash_map::Entry::Vacant(vacant_entry) => {
                let id = self
                    .aggregated_resources
                    .get(&key)
                    .map_or_else(f_id, |r| r.id);

                let mut aggregated_to = HashSet::new();
                for (a_res, a_res_data) in &mut self.aggregated_resources {
                    if key.matches(a_res) {
                        let new_info = Info::aggregate(a_res_data.info, a_res, &info, &key);
                        if Some(new_info) != a_res_data.info {
                            a_res_data.info = Some(new_info);
                            updated_resources.push(LocalResourceInsertResult {
                                id: a_res_data.id,
                                resource: a_res.clone(),
                                info: new_info,
                            });
                        }
                        a_res_data.aggregates.insert(key.clone());
                        aggregated_to.insert(a_res.clone());
                    }
                }
                let inserted_res = vacant_entry.insert(ResourceData {
                    id,
                    aggregated_to,
                    simple_interest_ids: simple_interests,
                    info,
                });
                if !inserted_res.simple_interest_ids.is_empty() {
                    updated_resources.push(LocalResourceInsertResult {
                        id,
                        resource: key,
                        info,
                    });
                }
                (id, updated_resources)
            }
        }
    }

    pub(crate) fn insert_aggregated_resource<F>(
        &mut self,
        key: Res,
        f_id: F,
        aggregated_interests: HashSet<InterestId>,
    ) -> (Id, Option<Info>)
    where
        F: FnOnce() -> Id,
    {
        match self.aggregated_resources.entry(key.clone()) {
            std::collections::hash_map::Entry::Occupied(mut occupied_entry) => {
                occupied_entry
                    .get_mut()
                    .aggregated_interest_ids
                    .extend(aggregated_interests);
                (occupied_entry.get().id, occupied_entry.get().info)
            }
            std::collections::hash_map::Entry::Vacant(vacant_entry) => {
                let mut aggregates = HashSet::new();
                for (s_res, s_res_data) in &mut self.simple_resources {
                    if s_res.matches(&key) {
                        s_res_data.aggregated_to.insert(key.clone());
                        aggregates.insert(s_res.clone());
                    }
                }
                let inserted_res = vacant_entry.insert(AggregatedResourceData {
                    id: self.simple_resources.get(&key).map_or_else(f_id, |r| r.id),
                    aggregates,
                    aggregated_interest_ids: aggregated_interests,
                    info: None,
                });
                inserted_res.info = inserted_res.recompute_info(&key, &self.simple_resources);
                (inserted_res.id, inserted_res.info)
            }
        }
    }

    // Returns resources that were removed/changed info due to simple resource removal.
    pub(crate) fn remove_simple_resource(
        &mut self,
        key: &Res,
    ) -> Vec<LocalResourceRemoveResult<Id, Res, Info>> {
        let mut updated_resources = Vec::new();
        if let Some(s_res_data) = self.simple_resources.remove(key) {
            if !s_res_data.simple_interest_ids.is_empty() {
                // there was an interest for this specific resource
                updated_resources.push(LocalResourceRemoveResult {
                    id: s_res_data.id,
                    resource: key.clone(),
                    update: None,
                });
            }
            if !s_res_data.aggregated_to.is_empty() {
                for a_res in &s_res_data.aggregated_to {
                    let a_res_data = self.aggregated_resources.get_mut(a_res).unwrap();
                    a_res_data.aggregates.remove(key);
                    let new_info = a_res_data.recompute_info(a_res, &self.simple_resources);
                    if new_info != a_res_data.info {
                        a_res_data.info = new_info;
                        updated_resources.push(LocalResourceRemoveResult {
                            id: a_res_data.id,
                            resource: a_res.clone(),
                            update: new_info,
                        })
                    }
                }
            }
        }
        updated_resources
    }

    pub(crate) fn remove_simple_resource_interest(&mut self, simple_interest: InterestId) {
        self.simple_resources.retain(|_, res_data| {
            !(res_data.simple_interest_ids.remove(&simple_interest)
                && res_data.simple_interest_ids.is_empty()
                && res_data.aggregated_to.is_empty())
        });
    }

    pub(crate) fn remove_aggregated_resource_interest(
        &mut self,
        key: &Res,
        aggregated_interest: InterestId,
    ) -> bool {
        match self.aggregated_resources.entry(key.clone()) {
            std::collections::hash_map::Entry::Occupied(mut occupied_entry) => {
                if occupied_entry
                    .get_mut()
                    .aggregated_interest_ids
                    .remove(&aggregated_interest)
                {
                    if occupied_entry.get_mut().aggregated_interest_ids.is_empty() {
                        // the aggregate can be removed if there is no other interest for it
                        let aggregates = occupied_entry.remove().aggregates;
                        for s_res in aggregates {
                            if let std::collections::hash_map::Entry::Occupied(mut e) =
                                self.simple_resources.entry(s_res)
                            {
                                e.get_mut().aggregated_to.remove(key);
                                if e.get().simple_interest_ids.is_empty()
                                    && e.get().aggregated_to.is_empty()
                                {
                                    // remove simple resource if there is no interest for it, nor it is aggregated into any other one
                                    e.remove();
                                }
                            }
                        }
                    }
                    true
                } else {
                    false
                }
            }
            std::collections::hash_map::Entry::Vacant(_) => false,
        }
    }

    pub(crate) fn clear(&mut self) {
        self.simple_resources.clear();
        self.aggregated_resources.clear();
    }
}

impl LocalResourceTrait for Arc<Resource> {
    fn matches(&self, other: &Self) -> bool {
        self.deref().matches(other)
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, sync::atomic::AtomicUsize};

    use zenoh_keyexpr::OwnedKeyExpr;

    use super::*;

    impl LocalResourceTrait for OwnedKeyExpr {
        fn matches(&self, other: &Self) -> bool {
            self.intersects(other)
        }
    }

    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    struct TestInfo {
        count: usize,
    }

    impl LocalResourceInfoTrait<OwnedKeyExpr> for TestInfo {
        fn aggregate(
            self_val: Option<Self>,
            _self_res: &OwnedKeyExpr,
            other_val: &Self,
            _other_res: &OwnedKeyExpr,
        ) -> Self {
            match self_val {
                Some(self_val) => TestInfo {
                    count: self_val.count + other_val.count,
                },
                None => *other_val,
            }
        }
    }

    type LocalTestResources = LocalResources<usize, OwnedKeyExpr, TestInfo>;

    fn ke(s: &str) -> OwnedKeyExpr {
        s.try_into().unwrap()
    }

    #[test]
    fn test_simple() {
        let mut local_res = LocalTestResources::new();
        let info0 = TestInfo { count: 0 };
        let info1 = TestInfo { count: 1 };
        let counter = AtomicUsize::new(0);
        let out = local_res.insert_simple_resource(
            "test/simple/1".try_into().unwrap(),
            info0,
            || counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst),
            HashSet::from([1u32]),
        );

        assert_eq!(out.0, 0);
        assert_eq!(out.1.len(), 1);
        assert_eq!(out.1[0].id, 0);
        assert_eq!(out.1[0].info, info0);
        assert_eq!(out.1[0].resource, ke("test/simple/1"));

        let _ = local_res.insert_simple_resource(
            ke("test/simple/2"),
            info0,
            || counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst),
            HashSet::from([2u32]),
        );

        let out = local_res.insert_simple_resource(
            ke("test/simple/2"),
            info0,
            || counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst),
            HashSet::from([2u32]),
        );
        assert_eq!(out.0, 1);
        assert_eq!(out.1.len(), 0);

        let out = local_res.insert_simple_resource(
            ke("test/simple/2"),
            info1,
            || counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst),
            HashSet::from([2u32]),
        );
        assert_eq!(out.0, 1);
        assert_eq!(out.1.len(), 1);
        assert_eq!(out.1[0].id, 1);
        assert_eq!(out.1[0].info, info1);
        assert_eq!(out.1[0].resource, ke("test/simple/2"));

        let _ = local_res.insert_simple_resource(
            ke("test/simple/*"),
            info1,
            || counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst),
            HashSet::from([1u32, 2u32]),
        );

        assert!(local_res.contains_simple_resource(&ke("test/simple/1")));
        assert!(local_res.contains_simple_resource(&ke("test/simple/2")));
        assert!(local_res.contains_simple_resource(&ke("test/simple/*")));

        let out = local_res.remove_simple_resource(&ke("test/simple/2"));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].id, 1);
        assert_eq!(out[0].update, None);
        assert_eq!(out[0].resource, ke("test/simple/2"));

        assert!(local_res.contains_simple_resource(&ke("test/simple/1")));
        assert!(!local_res.contains_simple_resource(&ke("test/simple/2")));
        assert!(local_res.contains_simple_resource(&ke("test/simple/*")));

        local_res.remove_simple_resource_interest(1);

        assert!(!local_res.contains_simple_resource(&ke("test/simple/1")));
        assert!(local_res.contains_simple_resource(&ke("test/simple/*")));

        local_res.remove_simple_resource_interest(2);

        assert!(!local_res.contains_simple_resource(&ke("test/simple/*")));
    }

    #[test]
    fn test_aggregate() {
        fn hm(
            v: Vec<LocalResourceInsertResult<usize, OwnedKeyExpr, TestInfo>>,
        ) -> HashMap<usize, (OwnedKeyExpr, TestInfo)> {
            v.into_iter()
                .map(|r| (r.id, (r.resource, r.info)))
                .collect::<HashMap<_, _>>()
        }

        let mut local_res = LocalTestResources::new();
        let info1 = TestInfo { count: 1 };
        let counter = AtomicUsize::new(0);
        local_res.insert_simple_resource(
            ke("test/aggregate/1"),
            info1,
            || counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst),
            HashSet::from([1u32]),
        );
        let _ = local_res.insert_simple_resource(
            ke("test/wrong/2"),
            info1,
            || counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst),
            HashSet::from([10u32]),
        );
        let out = local_res.insert_aggregated_resource(
            ke("test/aggregate/*"),
            || counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst),
            HashSet::from([2u32]),
        );
        assert_eq!(out.0, 2);
        assert_eq!(out.1, Some(TestInfo { count: 1 }));
        let out = local_res.insert_simple_resource(
            ke("test/aggregate/*"),
            info1,
            || counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst),
            HashSet::new(),
        );
        assert_eq!(out.0, 2);
        assert_eq!(out.1.len(), 1);
        assert_eq!(out.1[0].id, 2);
        assert_eq!(out.1[0].info, TestInfo { count: 2 });
        assert_eq!(out.1[0].resource, ke("test/aggregate/*"));

        let out = local_res.insert_simple_resource(
            ke("test/aggregate/2"),
            info1,
            || counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst),
            HashSet::from([3u32]),
        );
        assert_eq!(out.0, 3);
        let hm = hm(out.1);
        assert_eq!(hm.len(), 2);
        assert_eq!(
            hm.get(&3).unwrap(),
            &(ke("test/aggregate/2"), TestInfo { count: 1 })
        );
        assert_eq!(
            hm.get(&2).unwrap(),
            &(ke("test/aggregate/*"), TestInfo { count: 3 })
        );

        let out = local_res.insert_simple_resource(
            "test/aggregate/**".try_into().unwrap(),
            info1,
            || counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst),
            HashSet::new(),
        );
        assert_eq!(out.0, 4);
        assert_eq!(out.1.len(), 1);
        assert_eq!(out.1[0].id, 2);
        assert_eq!(out.1[0].info, TestInfo { count: 4 });
        assert_eq!(out.1[0].resource, ke("test/aggregate/*"));

        assert!(local_res.contains_simple_resource(&ke("test/aggregate/*")));
        let out = local_res.remove_simple_resource(&ke("test/aggregate/*"));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].id, 2);
        assert_eq!(out[0].update, Some(TestInfo { count: 3 }));
        assert_eq!(out[0].resource, ke("test/aggregate/*"));
        assert!(!local_res.contains_simple_resource(&ke("test/aggregate/*")));

        local_res.remove_simple_resource_interest(1u32);
        assert!(local_res.contains_simple_resource(&ke("test/aggregate/1")));
        assert!(local_res.contains_simple_resource(&ke("test/aggregate/**")));

        local_res.remove_aggregated_resource_interest(&ke("test/aggregate/*"), 2);

        assert!(!local_res.contains_simple_resource(&ke("test/aggregate/1")));
        assert!(!local_res.contains_simple_resource(&ke("test/aggregate/**")));
        assert!(local_res.contains_simple_resource(&ke("test/aggregate/2")));
    }
}
