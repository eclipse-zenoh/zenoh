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
use uhlc::HLC;

use zenoh_protocol::core::{whatami, Reliability, ResKey, SubInfo, SubMode, ZInt};
use zenoh_protocol::io::RBuf;
use zenoh_protocol::proto::DataInfo;

use crate::routing::broker::Tables;
use crate::routing::face::FaceState;
use crate::routing::resource::{Context, Resource};

pub type DataRoute = HashMap<usize, (Arc<FaceState>, ZInt, String)>;

pub async fn declare_subscription(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    prefixid: ZInt,
    suffix: &str,
    sub_info: &SubInfo,
) {
    match tables.get_mapping(&face, &prefixid).cloned() {
        Some(mut prefix) => unsafe {
            // Register subscription
            let mut res = Resource::make_resource(&mut prefix, suffix);
            Resource::match_resource(&tables.root_res, &mut res);
            {
                let res = Arc::get_mut_unchecked(&mut res);
                log::debug!("Register subscription {} for face {}", res.name(), face.id);
                match res.contexts.get_mut(&face.id) {
                    Some(mut ctx) => match &ctx.subs {
                        Some(info) => {
                            if SubMode::Pull == info.mode {
                                Arc::get_mut_unchecked(&mut ctx).subs = Some(sub_info.clone());
                            }
                        }
                        None => {
                            Arc::get_mut_unchecked(&mut ctx).subs = Some(sub_info.clone());
                        }
                    },
                    None => {
                        res.contexts.insert(
                            face.id,
                            Arc::new(Context {
                                face: face.clone(),
                                local_rid: None,
                                remote_rid: None,
                                subs: Some(sub_info.clone()),
                                qabl: false,
                                last_values: HashMap::new(),
                            }),
                        );
                    }
                }
            }

            // Propagate subscription
            let mut propa_sub_info = sub_info.clone();
            propa_sub_info.mode = SubMode::Push;
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
                                        .subscriber(&(rid, wildsuffix).into(), &propa_sub_info)
                                        .await;
                                } else if let Some(rid) = ctx.remote_rid {
                                    someface
                                        .primitives
                                        .subscriber(&(rid, wildsuffix).into(), &propa_sub_info)
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
                                        .subscriber(&(rid, wildsuffix).into(), &propa_sub_info)
                                        .await;
                                }
                            } else {
                                let rid = someface.get_next_local_id();
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
                                    .subscriber(&(rid, wildsuffix).into(), &propa_sub_info)
                                    .await;
                            }
                        }
                        None => {
                            someface
                                .primitives
                                .subscriber(&wildsuffix.into(), &propa_sub_info)
                                .await;
                        }
                    }
                }
            }
            Tables::build_matches_direct_tables(&mut res);
            Arc::get_mut_unchecked(face).subs.push(res);
        },
        None => log::error!("Declare subscription for unknown rid {}!", prefixid),
    }
}

pub async fn undeclare_subscription(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    prefixid: ZInt,
    suffix: &str,
) {
    match tables.get_mapping(&face, &prefixid) {
        Some(prefix) => match Resource::get_resource(prefix, suffix) {
            Some(mut res) => unsafe {
                log::debug!(
                    "Unregister subscription {} for face {}",
                    res.name(),
                    face.id
                );
                if let Some(mut ctx) = Arc::get_mut_unchecked(&mut res).contexts.get_mut(&face.id) {
                    Arc::get_mut_unchecked(&mut ctx).subs = None;
                }
                Arc::get_mut_unchecked(face)
                    .subs
                    .retain(|x| !Arc::ptr_eq(&x, &res));
                Resource::clean(&mut res)
            },
            None => log::error!("Undeclare unknown subscription!"),
        },
        None => log::error!("Undeclare subscription with unknown prefix!"),
    }
}

pub async fn route_data_to_map(
    tables: &mut Tables,
    face: &Arc<FaceState>,
    rid: ZInt,
    suffix: &str,
    _reliability: Reliability,
    info: &Option<DataInfo>,
    payload: &RBuf,
) -> Option<DataRoute> {
    match tables.get_mapping(&face, &rid) {
        Some(prefix) => unsafe {
            match Resource::get_resource(prefix, suffix) {
                Some(res) => {
                    let resname = res.name();
                    for mres in Resource::get_matches_from(
                        &[&prefix.name(), suffix].concat(),
                        &tables.root_res,
                    ) {
                        let mut mres = mres.upgrade().unwrap();
                        let mres = Arc::get_mut_unchecked(&mut mres);
                        for mut context in mres.contexts.values_mut() {
                            if let Some(subinfo) = &context.subs {
                                if SubMode::Pull == subinfo.mode {
                                    Arc::get_mut_unchecked(&mut context)
                                        .last_values
                                        .insert(resname.clone(), (info.clone(), payload.clone()));
                                }
                            }
                        }
                    }

                    Some(res.route.clone())
                }
                None => {
                    let mut faces = HashMap::new();
                    let resname = [&prefix.name(), suffix].concat();
                    for mres in Resource::get_matches_from(&resname, &tables.root_res) {
                        let mut mres = mres.upgrade().unwrap();
                        let mres = Arc::get_mut_unchecked(&mut mres);
                        for (sid, mut context) in &mut mres.contexts {
                            if let Some(subinfo) = &context.subs {
                                match subinfo.mode {
                                    SubMode::Pull => {
                                        Arc::get_mut_unchecked(&mut context).last_values.insert(
                                            resname.clone(),
                                            (info.clone(), payload.clone()),
                                        );
                                    }
                                    SubMode::Push => {
                                        faces.entry(*sid).or_insert_with(|| {
                                            let (rid, suffix) =
                                                Resource::get_best_key(prefix, suffix, *sid);
                                            (context.face.clone(), rid, suffix)
                                        });
                                    }
                                }
                            }
                        }
                    }
                    Some(faces)
                }
            }
        },
        None => {
            log::error!("Route data with unknown rid {}!", rid);
            None
        }
    }
}

pub async fn route_data(
    tables: &mut Tables,
    face: &Arc<FaceState>,
    rid: u64,
    suffix: &str,
    reliability: Reliability,
    info: Option<DataInfo>,
    payload: RBuf,
) {
    if let Some(outfaces) =
        route_data_to_map(tables, face, rid, suffix, reliability, &info, &payload).await
    {
        // if an HLC was configured (via Config.add_timestamp),
        // check DataInfo and add a timestamp if there isn't
        let data_info = match &tables.hlc {
            Some(hlc) => match treat_timestamp(hlc, info).await {
                Ok(info) => info,
                Err(e) => {
                    log::error!(
                        "Error treating timestamp for received Data ({}): drop it!",
                        e
                    );
                    return;
                }
            },
            None => info,
        };

        for (_id, (outface, rid, suffix)) in outfaces {
            if !Arc::ptr_eq(face, &outface) {
                let primitives = {
                    if (face.whatami != whatami::PEER && face.whatami != whatami::ROUTER)
                        || (outface.whatami != whatami::PEER && outface.whatami != whatami::ROUTER)
                    {
                        Some(outface.primitives.clone())
                    } else {
                        None
                    }
                };
                if let Some(primitives) = primitives {
                    primitives
                        .data(
                            &(rid, suffix).into(),
                            reliability,
                            data_info.clone(),
                            payload.clone(),
                        )
                        .await
                }
            }
        }
    }
}

async fn treat_timestamp(hlc: &HLC, info: Option<DataInfo>) -> Result<Option<DataInfo>, String> {
    if let Some(mut data_info) = info {
        if let Some(ref ts) = data_info.timestamp {
            // Timestamp is present; update HLC with it (possibly raising error if delta exceed)
            hlc.update_with_timestamp(ts).await?;
            Ok(Some(data_info))
        } else {
            // Timestamp not present; add one
            data_info.timestamp = Some(hlc.new_timestamp().await);
            log::trace!("Adding timestamp to DataInfo: {:?}", data_info.timestamp);
            Ok(Some(data_info))
        }
    } else {
        // No DataInfo; add one with a Timestamp
        Ok(Some(new_datainfo(hlc.new_timestamp().await)))
    }
}

fn new_datainfo(ts: uhlc::Timestamp) -> DataInfo {
    DataInfo {
        source_id: None,
        source_sn: None,
        first_router_id: None,
        first_router_sn: None,
        timestamp: Some(ts),
        kind: None,
        encoding: None,
    }
}

pub async fn pull_data(
    tables: &mut Tables,
    face: &Arc<FaceState>,
    _is_final: bool,
    rid: ZInt,
    suffix: &str,
    _pull_id: ZInt,
    _max_samples: &Option<ZInt>,
) {
    match tables.get_mapping(&face, &rid) {
        Some(prefix) => match Resource::get_resource(prefix, suffix) {
            Some(mut res) => unsafe {
                let res = Arc::get_mut_unchecked(&mut res);
                match res.contexts.get_mut(&face.id) {
                    Some(mut ctx) => match &ctx.subs {
                        Some(subinfo) => {
                            for (name, (info, data)) in &ctx.last_values {
                                let reskey: ResKey =
                                    Resource::get_best_key(&tables.root_res, name, face.id).into();
                                face.primitives
                                    .data(&reskey, subinfo.reliability, info.clone(), data.clone())
                                    .await;
                            }
                            Arc::get_mut_unchecked(&mut ctx).last_values.clear();
                        }
                        None => {
                            log::error!(
                                "Pull data for unknown subscription {} (no info)!",
                                [&prefix.name(), suffix].concat()
                            );
                        }
                    },
                    None => {
                        log::error!(
                            "Pull data for unknown subscription {} (no context)!",
                            [&prefix.name(), suffix].concat()
                        );
                    }
                }
            },
            None => {
                log::error!(
                    "Pull data for unknown subscription {} (no resource)!",
                    [&prefix.name(), suffix].concat()
                );
            }
        },
        None => {
            log::error!("Pull data with unknown rid {}!", rid);
        }
    };
}
