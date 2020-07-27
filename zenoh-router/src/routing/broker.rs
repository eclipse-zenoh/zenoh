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
use async_std::sync::RwLock;
use async_std::sync::{Arc, Weak};
use async_trait::async_trait;
use std::collections::HashMap;
use uhlc::HLC;

use zenoh_protocol::core::{whatami, Reliability, ResKey, SubInfo, SubMode, WhatAmI, ZInt};
use zenoh_protocol::proto::{DeMux, Mux, Primitives};
use zenoh_protocol::session::{Session, SessionEventHandler, SessionHandler};

use zenoh_util::core::ZResult;

use crate::routing::face::{Face, FaceState};

pub use crate::routing::pubsub::*;
pub use crate::routing::queries::*;
pub use crate::routing::resource::*;

/// # Examples
/// ```
///   use async_std::sync::Arc;
///   use uhlc::HLC;
///   use zenoh_protocol::core::{PeerId, whatami::PEER};
///   use zenoh_protocol::io::RBuf;
///   use zenoh_protocol::session::{SessionManager, SessionManagerConfig};
///   use zenoh_router::routing::broker::Broker;
///
///   async{
///     // implement Primitives trait
///     use zenoh_protocol::proto::Mux;
///     use zenoh_protocol::session::DummyHandler;
///     let dummy_primitives = Arc::new(Mux::new(Arc::new(DummyHandler::new())));
///
///     // Instanciate broker
///     let broker = Arc::new(Broker::new(HLC::default()));
///
///     // Instanciate SessionManager and plug it to the broker
///     let config = SessionManagerConfig {
///         version: 0,
///         whatami: PEER,
///         id: PeerId{id: vec![1, 2]},
///         handler: broker.clone()
///     };
///     let manager = SessionManager::new(config, None);
///
///     // Declare new primitives
///     let primitives = broker.new_primitives(dummy_primitives).await;
///
///     // Use primitives
///     primitives.data(&"/demo".to_string().into(), true, &None, RBuf::from(vec![1, 2])).await;
///
///     // Close primitives
///     primitives.close().await;
///   };
///
/// ```
pub struct Broker {
    pub tables: Arc<RwLock<Tables>>,
}

impl Broker {
    pub fn new(hlc: HLC) -> Broker {
        Broker {
            tables: Tables::new(hlc),
        }
    }

    pub async fn new_primitives(
        &self,
        primitives: Arc<dyn Primitives + Send + Sync>,
    ) -> Arc<dyn Primitives + Send + Sync> {
        Arc::new(Face {
            tables: self.tables.clone(),
            state: Tables::open_face(&self.tables, whatami::CLIENT, primitives)
                .await
                .upgrade()
                .unwrap(),
        })
    }
}

#[async_trait]
impl SessionHandler for Broker {
    async fn new_session(
        &self,
        session: Session,
    ) -> ZResult<Arc<dyn SessionEventHandler + Send + Sync>> {
        let whatami = session.get_whatami()?;
        let handler = Arc::new(DeMux::new(Face {
            tables: self.tables.clone(),
            state: Tables::open_face(&self.tables, whatami, Arc::new(Mux::new(Arc::new(session))))
                .await
                .upgrade()
                .unwrap(),
        }));
        Ok(handler)
    }
}

pub struct Tables {
    face_counter: usize,
    pub(crate) root_res: Arc<Resource>,
    pub(crate) faces: HashMap<usize, Arc<FaceState>>,
    pub(crate) hlc: HLC,
}

impl Tables {
    pub fn new(hlc: HLC) -> Arc<RwLock<Tables>> {
        Arc::new(RwLock::new(Tables {
            face_counter: 0,
            root_res: Resource::root(),
            faces: HashMap::new(),
            hlc,
        }))
    }

    #[doc(hidden)]
    pub fn _get_root(&self) -> &Arc<Resource> {
        &self.root_res
    }

    pub async fn print(tables: &Arc<RwLock<Tables>>) -> String {
        Resource::print_tree(&tables.read().await.root_res)
    }

    #[allow(clippy::trivially_copy_pass_by_ref)]
    pub(crate) fn get_mapping<'a>(
        &'a self,
        face: &'a FaceState,
        rid: &ZInt,
    ) -> Option<&'a Arc<Resource>> {
        match rid {
            0 => Some(&self.root_res),
            rid => face.get_mapping(rid),
        }
    }

    pub async fn open_face(
        tables: &Arc<RwLock<Tables>>,
        whatami: WhatAmI,
        primitives: Arc<dyn Primitives + Send + Sync>,
    ) -> Weak<FaceState> {
        unsafe {
            let mut t = tables.write().await;
            let fid = t.face_counter;
            log::debug!("New face {}", fid);
            t.face_counter += 1;
            let mut newface = t
                .faces
                .entry(fid)
                .or_insert_with(|| FaceState::new(fid, whatami, primitives.clone()))
                .clone();

            // @TODO temporarily propagate to everybody (clients)
            // if whatami != whatami::CLIENT {
            if true {
                let mut local_id: ZInt = 0;
                for (id, face) in t.faces.iter() {
                    if *id != fid {
                        for sub in face.subs.iter() {
                            let (nonwild_prefix, wildsuffix) = Resource::nonwild_prefix(sub);
                            match nonwild_prefix {
                                Some(mut nonwild_prefix) => {
                                    local_id += 1;
                                    Arc::get_mut_unchecked(&mut nonwild_prefix).contexts.insert(
                                        fid,
                                        Arc::new(Context {
                                            face: newface.clone(),
                                            local_rid: Some(local_id),
                                            remote_rid: None,
                                            subs: None,
                                            qabl: false,
                                            last_values: HashMap::new(),
                                        }),
                                    );
                                    Arc::get_mut_unchecked(&mut newface)
                                        .local_mappings
                                        .insert(local_id, nonwild_prefix.clone());

                                    let sub_info = SubInfo {
                                        reliability: Reliability::Reliable,
                                        mode: SubMode::Push,
                                        period: None,
                                    };
                                    primitives
                                        .resource(local_id, &ResKey::RName(nonwild_prefix.name()))
                                        .await;
                                    primitives
                                        .subscriber(
                                            &ResKey::RIdWithSuffix(local_id, wildsuffix),
                                            &sub_info,
                                        )
                                        .await;
                                }
                                None => {
                                    let sub_info = SubInfo {
                                        reliability: Reliability::Reliable,
                                        mode: SubMode::Push,
                                        period: None,
                                    };
                                    primitives
                                        .subscriber(&ResKey::RName(wildsuffix), &sub_info)
                                        .await;
                                }
                            }
                        }

                        for qabl in face.qabl.iter() {
                            let (nonwild_prefix, wildsuffix) = Resource::nonwild_prefix(qabl);
                            match nonwild_prefix {
                                Some(mut nonwild_prefix) => {
                                    local_id += 1;
                                    Arc::get_mut_unchecked(&mut nonwild_prefix).contexts.insert(
                                        fid,
                                        Arc::new(Context {
                                            face: newface.clone(),
                                            local_rid: Some(local_id),
                                            remote_rid: None,
                                            subs: None,
                                            qabl: false,
                                            last_values: HashMap::new(),
                                        }),
                                    );
                                    Arc::get_mut_unchecked(&mut newface)
                                        .local_mappings
                                        .insert(local_id, nonwild_prefix.clone());

                                    primitives
                                        .resource(local_id, &ResKey::RName(nonwild_prefix.name()))
                                        .await;
                                    primitives
                                        .queryable(&ResKey::RIdWithSuffix(local_id, wildsuffix))
                                        .await;
                                }
                                None => {
                                    primitives.queryable(&ResKey::RName(wildsuffix)).await;
                                }
                            }
                        }
                    }
                }
            }
            Arc::downgrade(&newface)
        }
    }

    pub async fn close_face(tables: &Arc<RwLock<Tables>>, face: &Weak<FaceState>) {
        let mut t = tables.write().await;
        match face.upgrade() {
            Some(mut face) => unsafe {
                log::debug!("Close face {}", face.id);
                finalize_pending_queries(&mut t, &mut face).await;

                let face = Arc::get_mut_unchecked(&mut face);
                for mut res in face.remote_mappings.values_mut() {
                    Arc::get_mut_unchecked(res).contexts.remove(&face.id);
                    Resource::clean(&mut res);
                }
                face.remote_mappings.clear();
                for mut res in face.local_mappings.values_mut() {
                    Arc::get_mut_unchecked(res).contexts.remove(&face.id);
                    Resource::clean(&mut res);
                }
                face.local_mappings.clear();
                while let Some(mut res) = face.subs.pop() {
                    Arc::get_mut_unchecked(&mut res).contexts.remove(&face.id);
                    Resource::clean(&mut res);
                }
                while let Some(mut res) = face.qabl.pop() {
                    Arc::get_mut_unchecked(&mut res).contexts.remove(&face.id);
                    Resource::clean(&mut res);
                }
                t.faces.remove(&face.id);
            },
            None => log::error!("Face already closed!"),
        }
    }

    unsafe fn build_direct_tables(res: &mut Arc<Resource>) {
        let mut dests = HashMap::new();
        for match_ in &res.matches {
            for (fid, context) in &match_.upgrade().unwrap().contexts {
                if let Some(subinfo) = &context.subs {
                    if SubMode::Push == subinfo.mode {
                        let (rid, suffix) = Resource::get_best_key(res, "", *fid);
                        dests.insert(*fid, (context.face.clone(), rid, suffix));
                    }
                }
            }
        }
        Arc::get_mut_unchecked(res).route = dests;
    }

    pub(crate) unsafe fn build_matches_direct_tables(res: &mut Arc<Resource>) {
        Tables::build_direct_tables(res);

        let resclone = res.clone();
        for match_ in &mut Arc::get_mut_unchecked(res).matches {
            if !Arc::ptr_eq(&match_.upgrade().unwrap(), &resclone) {
                Tables::build_direct_tables(&mut match_.upgrade().unwrap());
            }
        }
    }

    pub async fn get_matches(tables: &Arc<RwLock<Tables>>, rname: &str) -> Vec<Weak<Resource>> {
        let t = tables.read().await;
        Resource::get_matches_from(rname, &t.root_res)
    }
}
