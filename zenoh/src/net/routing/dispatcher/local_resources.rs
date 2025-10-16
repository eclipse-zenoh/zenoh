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
    sync::Arc,
};

use zenoh_protocol::network::interest::InterestId;

use crate::net::routing::router::Resource;

pub(crate) trait ResourceState
where
    Self: Sized + Eq + Clone + Copy,
{
    fn merge(
        self_val: Option<Self>,
        self_res: &Arc<Resource>,
        other_val: &Self,
        other_res: &Arc<Resource>,
    ) -> Self;

    fn merge_many<'a>(
        self_res: &Arc<Resource>,
        iter: impl Iterator<Item = (&'a Arc<Resource>, Self)>,
    ) -> Option<Self> {
        let mut out = None;
        for (res, val) in iter {
            out = Some(Self::merge(out, self_res, &val, res));
        }
        out
    }
}

struct ResourceData<Id: Copy, State: ResourceState> {
    id: Id,
    aggregated_to: HashSet<Arc<Resource>>,
    interest_ids: HashSet<InterestId>,
    state: State,
}

struct AggregatedResourceData<Id: Copy, State: ResourceState> {
    id: Id,
    aggregates: HashSet<Arc<Resource>>,
    interest_ids: HashSet<InterestId>,
    state: Option<State>,
}

impl<Id: Copy, State: ResourceState> AggregatedResourceData<Id, State> {
    fn recompute_state(
        &self,
        self_res: &Arc<Resource>,
        subs: &HashMap<Arc<Resource>, ResourceData<Id, State>>,
    ) -> Option<State> {
        let iter = self
            .aggregates
            .iter()
            .map(|r| (r, subs.get(r).unwrap().state));
        State::merge_many(self_res, iter)
    }
}

pub(crate) struct LocalResourceRemoveResult<Id: Copy, State: ResourceState> {
    pub(crate) id: Id,
    pub(crate) resource: Arc<Resource>,
    pub(crate) update: Option<State>,
}

pub(crate) struct LocalResourceInsertResult<Id: Copy, State: ResourceState> {
    pub(crate) id: Id,
    pub(crate) resource: Arc<Resource>,
    pub(crate) state: State,
}

pub(crate) struct LocalResources<Id: Copy, State: ResourceState> {
    simple_resources: HashMap<Arc<Resource>, ResourceData<Id, State>>,
    aggregated_resources: HashMap<Arc<Resource>, AggregatedResourceData<Id, State>>,
}

impl<Id: Copy, State: ResourceState> LocalResources<Id, State> {
    pub(crate) fn new() -> Self {
        Self {
            simple_resources: HashMap::new(),
            aggregated_resources: HashMap::new(),
        }
    }

    pub(crate) fn contains_simple_resource(&self, key: &Arc<Resource>) -> bool {
        self.simple_resources.contains_key(key)
    }

    pub(crate) fn simple_resources(&self) -> impl Iterator<Item = &Arc<Resource>> {
        self.simple_resources.keys()
    }

    // Returns Id of newly inserted resource and the list of resources that were enabled/changed state by this operation
    pub(crate) fn insert_simple_resource<F>(
        &mut self,
        key: Arc<Resource>,
        state: State,
        f_id: F,
        interests: HashSet<InterestId>,
    ) -> (Id, Vec<LocalResourceInsertResult<Id, State>>)
    where
        F: FnOnce() -> Id,
    {
        let mut updated_resources = Vec::new();
        match self.simple_resources.entry(key.clone()) {
            std::collections::hash_map::Entry::Occupied(mut occupied_entry) => {
                {
                    let s_res_data = occupied_entry.get_mut();
                    s_res_data.interest_ids.extend(interests);
                    if !s_res_data.interest_ids.is_empty() && s_res_data.state != state {
                        updated_resources.push(LocalResourceInsertResult {
                            id: s_res_data.id,
                            resource: key.clone(),
                            state,
                        });
                    }
                    s_res_data.state = state;
                };
                let s_res_data = self.simple_resources.get(&key).unwrap(); // reborrow as shared ref

                for a_res in &s_res_data.aggregated_to {
                    if let Some(a_res_data) = self.aggregated_resources.get_mut(a_res) {
                        let new_state = a_res_data.recompute_state(a_res, &self.simple_resources);
                        if new_state != a_res_data.state {
                            a_res_data.state = new_state;
                            updated_resources.push(LocalResourceInsertResult {
                                id: a_res_data.id,
                                resource: a_res.clone(),
                                state: new_state.unwrap(), // aggregated resource contains at least one simple - so it is guaranteed to have an initialized state
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
                        let new_state = State::merge(a_res_data.state, a_res, &state, &key);
                        if Some(new_state) != a_res_data.state {
                            a_res_data.state = Some(new_state);
                            updated_resources.push(LocalResourceInsertResult {
                                id: a_res_data.id,
                                resource: a_res.clone(),
                                state: new_state,
                            });
                        }
                        a_res_data.aggregates.insert(key.clone());
                        aggregated_to.insert(a_res.clone());
                    }
                }
                let inserted_res = vacant_entry.insert(ResourceData {
                    id,
                    aggregated_to,
                    interest_ids: interests,
                    state,
                });
                if !inserted_res.interest_ids.is_empty() {
                    updated_resources.push(LocalResourceInsertResult {
                        id,
                        resource: key,
                        state,
                    });
                }
                (id, updated_resources)
            }
        }
    }

    pub(crate) fn insert_aggregated_resource<F>(
        &mut self,
        key: Arc<Resource>,
        f_id: F,
        interests: HashSet<InterestId>,
    ) -> (Id, Option<State>)
    where
        F: FnOnce() -> Id,
    {
        match self.aggregated_resources.entry(key.clone()) {
            std::collections::hash_map::Entry::Occupied(mut occupied_entry) => {
                occupied_entry.get_mut().interest_ids.extend(interests);
                (occupied_entry.get().id, occupied_entry.get().state)
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
                    interest_ids: interests,
                    state: None,
                });
                inserted_res.state = inserted_res.recompute_state(&key, &self.simple_resources);
                (inserted_res.id, inserted_res.state)
            }
        }
    }

    // Returns resources that were removed/changed state due to simple resource removal.
    pub(crate) fn remove(
        &mut self,
        key: &Arc<Resource>,
    ) -> Vec<LocalResourceRemoveResult<Id, State>> {
        let mut updated_resources = Vec::new();
        if let Some(s_res_data) = self.simple_resources.remove(key) {
            if !s_res_data.interest_ids.is_empty() {
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
                    let new_state = a_res_data.recompute_state(a_res, &self.simple_resources);
                    if new_state != a_res_data.state {
                        a_res_data.state = new_state;
                        updated_resources.push(LocalResourceRemoveResult {
                            id: a_res_data.id,
                            resource: a_res.clone(),
                            update: new_state,
                        })
                    }
                }
            }
        }
        updated_resources
    }

    pub(crate) fn remove_simple_resource_interest(
        &mut self,
        key: &Option<Arc<Resource>>,
        interest: InterestId,
    ) {
        let mut resources_to_remove = Vec::new();
        for res in self.simple_resources.keys() {
            if key.as_ref().map(|k| k.matches(res)).unwrap_or(true) {
                resources_to_remove.push(res.clone());
            }
        }

        for res in resources_to_remove {
            if let std::collections::hash_map::Entry::Occupied(mut occupied_entry) =
                self.simple_resources.entry(res)
            {
                if occupied_entry.get_mut().interest_ids.remove(&interest)
                    && occupied_entry.get().interest_ids.is_empty()
                    && occupied_entry.get().aggregated_to.is_empty()
                {
                    occupied_entry.remove();
                }
            }
        }
    }

    pub(crate) fn remove_aggregated_resource_interest(
        &mut self,
        key: &Arc<Resource>,
        interest: InterestId,
    ) -> bool {
        match self.aggregated_resources.entry(key.clone()) {
            std::collections::hash_map::Entry::Occupied(mut occupied_entry) => {
                if occupied_entry.get_mut().interest_ids.remove(&interest) {
                    if occupied_entry.get_mut().interest_ids.is_empty() {
                        // the aggregate can be removed if there is no other interest for it
                        let aggregates = occupied_entry.remove().aggregates;
                        for s_res in aggregates {
                            if let std::collections::hash_map::Entry::Occupied(mut e) =
                                self.simple_resources.entry(s_res)
                            {
                                e.get_mut().aggregated_to.remove(key);
                                if e.get().interest_ids.is_empty()
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
