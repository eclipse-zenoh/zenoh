//
// Copyright (c) 2017, 2020 ADLINK Technology Inc.
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ADLINK zenoh team, <zenoh@adlink-labs.tech>
//
use async_std::sync::Arc;
use std::collections::HashMap;

use zenoh_protocol::core::{whatami, PeerId, QueryConsolidation, QueryTarget, ResKey, ZInt};
use zenoh_protocol::io::RBuf;
use zenoh_protocol::proto::DataInfo;

use crate::routing::broker::Tables;
use crate::routing::face::FaceState;
use crate::routing::resource::{Context, Resource};

pub(crate) struct Query {
    src_face: Arc<FaceState>,
    src_qid: ZInt,
}

type QueryRoute = HashMap<usize, (Arc<FaceState>, ZInt, String, ZInt)>;

pub(crate) async fn declare_queryable(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    prefixid: ZInt,
    suffix: &str,
) {
    let prefix = {
        match prefixid {
            0 => Some(tables.root_res.clone()),
            prefixid => face.get_mapping(&prefixid).cloned(),
        }
    };
    match prefix {
        Some(mut prefix) => unsafe {
            let mut res = Resource::make_resource(&mut prefix, suffix);
            Resource::match_resource(&tables.root_res, &mut res);
            {
                log::debug!("Register quaryable {} for face {}", res.name(), face.id);
                match Arc::get_mut_unchecked(&mut res).contexts.get_mut(&face.id) {
                    Some(mut ctx) => {
                        Arc::get_mut_unchecked(&mut ctx).qabl = true;
                    }
                    None => {
                        Arc::get_mut_unchecked(&mut res).contexts.insert(
                            face.id,
                            Arc::new(Context {
                                face: face.clone(),
                                local_rid: None,
                                remote_rid: None,
                                subs: None,
                                qabl: true,
                                last_values: HashMap::new(),
                            }),
                        );
                    }
                }
            }

            for (id, someface) in &mut tables.faces {
                if face.id != *id
                    && (face.whatami != whatami::PEER || someface.whatami != whatami::PEER)
                    && (face.whatami != whatami::ROUTER || someface.whatami != whatami::ROUTER)
                {
                    let (nonwild_prefix, wildsuffix) = Resource::nonwild_prefix(&res);
                    match nonwild_prefix {
                        Some(mut nonwild_prefix) => {
                            if let Some(mut ctx) = Arc::get_mut_unchecked(&mut nonwild_prefix)
                                .contexts
                                .get_mut(id)
                            {
                                if let Some(rid) = ctx.local_rid {
                                    someface
                                        .primitives
                                        .queryable(&(rid, wildsuffix).into())
                                        .await;
                                } else if let Some(rid) = ctx.remote_rid {
                                    someface
                                        .primitives
                                        .queryable(&(rid, wildsuffix).into())
                                        .await;
                                } else {
                                    let rid = someface.get_next_local_id();
                                    Arc::get_mut_unchecked(&mut ctx).local_rid = Some(rid);
                                    Arc::get_mut_unchecked(someface)
                                        .local_mappings
                                        .insert(rid, nonwild_prefix.clone());

                                    someface
                                        .primitives
                                        .resource(rid, &nonwild_prefix.name().into())
                                        .await;
                                    someface
                                        .primitives
                                        .queryable(&(rid, wildsuffix).into())
                                        .await;
                                }
                            } else {
                                let rid = face.get_next_local_id();
                                Arc::get_mut_unchecked(&mut nonwild_prefix).contexts.insert(
                                    *id,
                                    Arc::new(Context {
                                        face: someface.clone(),
                                        local_rid: Some(rid),
                                        remote_rid: None,
                                        subs: None,
                                        qabl: false,
                                        last_values: HashMap::new(),
                                    }),
                                );
                                Arc::get_mut_unchecked(someface)
                                    .local_mappings
                                    .insert(rid, nonwild_prefix.clone());

                                someface
                                    .primitives
                                    .resource(rid, &nonwild_prefix.name().into())
                                    .await;
                                someface
                                    .primitives
                                    .queryable(&(rid, wildsuffix).into())
                                    .await;
                            }
                        }
                        None => {
                            someface.primitives.queryable(&wildsuffix.into()).await;
                        }
                    }
                }
            }
            Tables::build_matches_direct_tables(&mut res);
            Arc::get_mut_unchecked(face).qabl.push(res);
        },
        None => log::error!("Declare queryable for unknown rid {}!", prefixid),
    }
}

pub async fn undeclare_queryable(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    prefixid: ZInt,
    suffix: &str,
) {
    match tables.get_mapping(&face, &prefixid) {
        Some(prefix) => match Resource::get_resource(prefix, suffix) {
            Some(mut res) => unsafe {
                log::debug!("Unregister queryable {} for face {}", res.name(), face.id);
                if let Some(mut ctx) = Arc::get_mut_unchecked(&mut res).contexts.get_mut(&face.id) {
                    Arc::get_mut_unchecked(&mut ctx).qabl = false;
                }
                Arc::get_mut_unchecked(face)
                    .subs
                    .retain(|x| !Arc::ptr_eq(&x, &res));
                Resource::clean(&mut res)
            },
            None => log::error!("Undeclare unknown queryable!"),
        },
        None => log::error!("Undeclare queryable with unknown prefix!"),
    }
}

async fn route_query_to_map(
    tables: &mut Tables,
    face: &Arc<FaceState>,
    qid: ZInt,
    rid: ZInt,
    suffix: &str, /*, _predicate: &str, */
                  /*_qid: ZInt, _target: &Option<QueryTarget>, _consolidation: &QueryConsolidation*/
) -> Option<QueryRoute> {
    match tables.get_mapping(&face, &rid) {
        Some(prefix) => {
            log::debug!(
                "Route query {}:{} {}{}",
                face.id,
                qid,
                prefix.name(),
                suffix
            );
            let query = Arc::new(Query {
                src_face: face.clone(),
                src_qid: qid,
            });
            let mut faces = HashMap::new();
            for res in
                Resource::get_matches_from(&[&prefix.name(), suffix].concat(), &tables.root_res)
            {
                unsafe {
                    let mut res = res.upgrade().unwrap();
                    for (sid, context) in &mut Arc::get_mut_unchecked(&mut res).contexts {
                        if context.qabl
                            && !Arc::ptr_eq(&face, &context.face)
                            && ((face.whatami != whatami::PEER && face.whatami != whatami::ROUTER)
                                || (context.face.whatami != whatami::PEER
                                    && context.face.whatami != whatami::ROUTER))
                        {
                            faces.entry(*sid).or_insert_with(|| {
                                let (rid, suffix) = Resource::get_best_key(prefix, suffix, *sid);
                                let face = Arc::get_mut_unchecked(
                                    &mut Arc::get_mut_unchecked(context).face,
                                );
                                face.next_qid += 1;
                                let qid = face.next_qid;
                                face.pending_queries.insert(qid, query.clone());
                                (context.face.clone(), rid, suffix, qid)
                            });
                        }
                    }
                }
            }
            Some(faces)
        }
        None => {
            log::error!("Route query with unknown rid {}!", rid);
            None
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn route_query(
    tables: &mut Tables,
    face: &Arc<FaceState>,
    rid: ZInt,
    suffix: &str,
    predicate: &str,
    qid: ZInt,
    target: QueryTarget,
    consolidation: QueryConsolidation,
) {
    if let Some(outfaces) = route_query_to_map(tables, face, qid, rid, suffix).await {
        let outfaces = outfaces
            .into_iter()
            .filter(|(_, (outface, _, _, _))| {
                face.whatami != whatami::PEER || outface.whatami != whatami::PEER
            })
            .map(|(_, v)| v)
            .collect::<Vec<(Arc<FaceState>, ZInt, String, ZInt)>>();
        match outfaces.len() {
            0 => {
                log::debug!(
                    "Send final reply {}:{} (no matching queryables)",
                    face.id,
                    qid
                );
                face.primitives.clone().reply_final(qid).await
            }
            _ => {
                for (outface, rid, suffix, qid) in outfaces {
                    outface
                        .primitives
                        .query(
                            &(rid, suffix).into(),
                            predicate,
                            qid,
                            target.clone(),
                            consolidation.clone(),
                        )
                        .await;
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn route_reply_data(
    _tables: &mut Tables,
    face: &mut Arc<FaceState>,
    qid: ZInt,
    source_kind: ZInt,
    replier_id: PeerId,
    reskey: ResKey,
    info: Option<DataInfo>,
    payload: RBuf,
) {
    match face.pending_queries.get(&qid) {
        Some(query) => {
            query
                .src_face
                .primitives
                .clone()
                .reply_data(
                    query.src_qid,
                    source_kind,
                    replier_id,
                    reskey,
                    info,
                    payload,
                )
                .await;
        }
        None => log::error!("Route reply for unknown query!"),
    }
}

pub(crate) async fn route_reply_final(_tables: &mut Tables, face: &mut Arc<FaceState>, qid: ZInt) {
    match face.pending_queries.get(&qid) {
        Some(query) => unsafe {
            log::debug!(
                "Received final reply {}:{} from face {}",
                query.src_face.id,
                qid,
                face.id
            );
            if Arc::strong_count(&query) == 1 {
                log::debug!("Propagate final reply {}:{}", query.src_face.id, qid);
                query
                    .src_face
                    .primitives
                    .clone()
                    .reply_final(query.src_qid)
                    .await;
            }
            Arc::get_mut_unchecked(face).pending_queries.remove(&qid);
        },
        None => log::error!("Route reply for unknown query!"),
    }
}

pub(crate) async fn finalize_pending_queries(_tables: &mut Tables, face: &mut Arc<FaceState>) {
    for query in face.pending_queries.values() {
        log::debug!(
            "Finalize reply {}:{} for closing face {}",
            query.src_face.id,
            query.src_qid,
            face.id
        );
        if Arc::strong_count(&query) == 1 {
            log::debug!(
                "Propagate final reply {}:{}",
                query.src_face.id,
                query.src_qid
            );
            query
                .src_face
                .primitives
                .clone()
                .reply_final(query.src_qid)
                .await;
        }
    }
    unsafe {
        Arc::get_mut_unchecked(face).pending_queries.clear();
    }
}
