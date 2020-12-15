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

use zenoh_protocol::core::{
    whatami, CongestionControl, Reliability, ResKey, SubInfo, SubMode, ZInt,
};
use zenoh_protocol::io::RBuf;
use zenoh_protocol::proto::DataInfo;

use crate::routing::broker::Tables;
use crate::routing::face::FaceState;
use crate::routing::resource::{Context, Resource};

pub(crate) fn propagate_subscription(
    whatami: whatami::Type,
    src_face: &Arc<FaceState>,
    dst_face: &Arc<FaceState>,
) -> bool {
    src_face.id != dst_face.id
        && match whatami {
            whatami::ROUTER => {
                (src_face.whatami != whatami::PEER || dst_face.whatami != whatami::PEER)
                    && (src_face.whatami != whatami::ROUTER || dst_face.whatami != whatami::ROUTER)
            }
            _ => (src_face.whatami == whatami::CLIENT || dst_face.whatami == whatami::CLIENT),
        }
}

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
            Resource::match_resource(&tables, &mut res);
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
            let whatami = tables.whatami;
            for someface in &mut tables.faces.values_mut() {
                if propagate_subscription(whatami, face, someface) {
                    let reskey = Resource::decl_key(&res, someface).await;
                    someface
                        .primitives
                        .subscriber(&reskey, &propa_sub_info, None)
                        .await;
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

#[inline]
fn propagate_data(
    whatami: whatami::Type,
    src_face: &Arc<FaceState>,
    dst_face: &Arc<FaceState>,
) -> bool {
    src_face.id != dst_face.id
        && match whatami {
            whatami::ROUTER => {
                (src_face.whatami != whatami::PEER || dst_face.whatami != whatami::PEER)
                    && (src_face.whatami != whatami::ROUTER || dst_face.whatami != whatami::ROUTER)
            }
            _ => (src_face.whatami == whatami::CLIENT || dst_face.whatami == whatami::CLIENT),
        }
}

pub async fn route_data(
    tables: &mut Tables,
    face: &Arc<FaceState>,
    rid: u64,
    suffix: &str,
    congestion_control: CongestionControl,
    info: Option<DataInfo>,
    payload: RBuf,
) {
    match tables.get_mapping(&face, &rid) {
        Some(prefix) => unsafe {
            // if an HLC was configured (via Config.add_timestamp),
            // check DataInfo and add a timestamp if there isn't
            let info = match &tables.hlc {
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

            match Resource::get_resource(prefix, suffix) {
                Some(res) => {
                    for mres in &res.matches {
                        let mut mres = mres.upgrade().unwrap();
                        let mres = Arc::get_mut_unchecked(&mut mres);
                        for mut context in mres.contexts.values_mut() {
                            if let Some(subinfo) = &context.subs {
                                if SubMode::Pull == subinfo.mode {
                                    Arc::get_mut_unchecked(&mut context).last_values.insert(
                                        [&prefix.name(), suffix].concat(),
                                        (info.clone(), payload.clone()),
                                    );
                                }
                            }
                        }
                    }
                    for (outface, rid, suffix) in res.route.values() {
                        if propagate_data(tables.whatami, face, outface) {
                            outface
                                .primitives
                                .data(
                                    &(*rid, suffix.clone()).into(),
                                    payload.clone(),
                                    Reliability::Reliable, // TODO: Need to check the active subscriptions to determine the right reliability value
                                    congestion_control,
                                    info.clone(),
                                    None,
                                )
                                .await
                        }
                    }
                }
                None => {
                    let mut faces = HashMap::new();
                    let resname = [&prefix.name(), suffix].concat();
                    for mres in Resource::get_matches(&tables, &resname) {
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
                    for (outface, rid, suffix) in faces.into_values() {
                        if propagate_data(tables.whatami, face, &outface) {
                            outface
                                .primitives
                                .data(
                                    &(rid, suffix).into(),
                                    payload.clone(),
                                    Reliability::Reliable, // TODO: Need to check the active subscriptions to determine the right reliability value
                                    congestion_control,
                                    info.clone(),
                                    None,
                                )
                                .await
                        }
                    }
                }
            }
        },
        None => {
            log::error!("Route data with unknown rid {}!", rid);
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
                                    .data(
                                        &reskey,
                                        data.clone(),
                                        subinfo.reliability,
                                        CongestionControl::Drop, // TODO: Default value for the time being
                                        info.clone(),
                                        None,
                                    )
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
