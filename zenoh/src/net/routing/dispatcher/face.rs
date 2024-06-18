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
    any::Any,
    collections::HashMap,
    fmt,
    sync::{Arc, Weak},
};

use tokio_util::sync::CancellationToken;
use zenoh_protocol::{
    core::{ExprId, WhatAmI, ZenohIdProto},
    network::{
        interest::{InterestId, InterestMode, InterestOptions},
        Mapping, Push, Request, RequestId, Response, ResponseFinal,
    },
    zenoh::RequestBody,
};
use zenoh_sync::get_mut_unchecked;
use zenoh_task::TaskController;
use zenoh_transport::multicast::TransportMulticast;
#[cfg(feature = "stats")]
use zenoh_transport::stats::TransportStats;

use super::{
    super::router::*,
    interests::{declare_final, declare_interest, undeclare_interest, CurrentInterest},
    resource::*,
    tables::{self, TablesLock},
};
use crate::{
    api::key_expr::KeyExpr,
    net::{
        primitives::{McastMux, Mux, Primitives},
        routing::interceptor::{InterceptorTrait, InterceptorsChain},
    },
};

pub(crate) struct InterestState {
    pub(crate) options: InterestOptions,
    pub(crate) res: Option<Arc<Resource>>,
    pub(crate) finalized: bool,
}

pub struct FaceState {
    pub(crate) id: usize,
    pub(crate) zid: ZenohIdProto,
    pub(crate) whatami: WhatAmI,
    #[cfg(feature = "stats")]
    pub(crate) stats: Option<Arc<TransportStats>>,
    pub(crate) primitives: Arc<dyn crate::net::primitives::EPrimitives + Send + Sync>,
    pub(crate) local_interests: HashMap<InterestId, InterestState>,
    pub(crate) remote_key_interests: HashMap<InterestId, Option<Arc<Resource>>>,
    pub(crate) pending_current_interests:
        HashMap<InterestId, (Arc<CurrentInterest>, CancellationToken)>,
    pub(crate) local_mappings: HashMap<ExprId, Arc<Resource>>,
    pub(crate) remote_mappings: HashMap<ExprId, Arc<Resource>>,
    pub(crate) next_qid: RequestId,
    pub(crate) pending_queries: HashMap<RequestId, (Arc<Query>, CancellationToken)>,
    pub(crate) mcast_group: Option<TransportMulticast>,
    pub(crate) in_interceptors: Option<Arc<InterceptorsChain>>,
    pub(crate) hat: Box<dyn Any + Send + Sync>,
    pub(crate) task_controller: TaskController,
}

impl FaceState {
    #[allow(clippy::too_many_arguments)] // @TODO fix warning
    pub(crate) fn new(
        id: usize,
        zid: ZenohIdProto,
        whatami: WhatAmI,
        #[cfg(feature = "stats")] stats: Option<Arc<TransportStats>>,
        primitives: Arc<dyn crate::net::primitives::EPrimitives + Send + Sync>,
        mcast_group: Option<TransportMulticast>,
        in_interceptors: Option<Arc<InterceptorsChain>>,
        hat: Box<dyn Any + Send + Sync>,
    ) -> Arc<FaceState> {
        Arc::new(FaceState {
            id,
            zid,
            whatami,
            #[cfg(feature = "stats")]
            stats,
            primitives,
            local_interests: HashMap::new(),
            remote_key_interests: HashMap::new(),
            pending_current_interests: HashMap::new(),
            local_mappings: HashMap::new(),
            remote_mappings: HashMap::new(),
            next_qid: 0,
            pending_queries: HashMap::new(),
            mcast_group,
            in_interceptors,
            hat,
            task_controller: TaskController::default(),
        })
    }

    #[inline]
    pub(crate) fn get_mapping(
        &self,
        prefixid: &ExprId,
        mapping: Mapping,
    ) -> Option<&std::sync::Arc<Resource>> {
        match mapping {
            Mapping::Sender => self.remote_mappings.get(prefixid),
            Mapping::Receiver => self.local_mappings.get(prefixid),
        }
    }

    #[inline]
    pub(crate) fn get_sent_mapping(
        &self,
        prefixid: &ExprId,
        mapping: Mapping,
    ) -> Option<&std::sync::Arc<Resource>> {
        match mapping {
            Mapping::Sender => self.local_mappings.get(prefixid),
            Mapping::Receiver => self.remote_mappings.get(prefixid),
        }
    }

    pub(crate) fn get_next_local_id(&self) -> ExprId {
        let mut id = 1;
        while self.local_mappings.contains_key(&id) || self.remote_mappings.contains_key(&id) {
            id += 1;
        }
        id
    }

    pub(crate) fn update_interceptors_caches(&self, res: &mut Arc<Resource>) {
        if let Ok(expr) = KeyExpr::try_from(res.expr()) {
            if let Some(interceptor) = self.in_interceptors.as_ref() {
                let cache = interceptor.compute_keyexpr_cache(&expr);
                get_mut_unchecked(
                    get_mut_unchecked(res)
                        .session_ctxs
                        .get_mut(&self.id)
                        .unwrap(),
                )
                .in_interceptor_cache = cache;
            }
            if let Some(mux) = self.primitives.as_any().downcast_ref::<Mux>() {
                let cache = mux.interceptor.compute_keyexpr_cache(&expr);
                get_mut_unchecked(
                    get_mut_unchecked(res)
                        .session_ctxs
                        .get_mut(&self.id)
                        .unwrap(),
                )
                .e_interceptor_cache = cache;
            }
            if let Some(mux) = self.primitives.as_any().downcast_ref::<McastMux>() {
                let cache = mux.interceptor.compute_keyexpr_cache(&expr);
                get_mut_unchecked(
                    get_mut_unchecked(res)
                        .session_ctxs
                        .get_mut(&self.id)
                        .unwrap(),
                )
                .e_interceptor_cache = cache;
            }
        }
    }
}

impl fmt::Display for FaceState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Face{{{}, {}}}", self.id, self.zid)
    }
}

#[derive(Clone, Debug)]
pub struct WeakFace {
    pub(crate) tables: Weak<TablesLock>,
    pub(crate) state: Weak<FaceState>,
}

impl WeakFace {
    pub fn upgrade(&self) -> Option<Face> {
        Some(Face {
            tables: self.tables.upgrade()?,
            state: self.state.upgrade()?,
        })
    }
}

#[derive(Clone)]
pub struct Face {
    pub(crate) tables: Arc<TablesLock>,
    pub(crate) state: Arc<FaceState>,
}

impl Face {
    pub fn downgrade(&self) -> WeakFace {
        WeakFace {
            tables: Arc::downgrade(&self.tables),
            state: Arc::downgrade(&self.state),
        }
    }
}

impl Primitives for Face {
    fn send_interest(&self, msg: zenoh_protocol::network::Interest) {
        let ctrl_lock = zlock!(self.tables.ctrl_lock);
        if msg.mode != InterestMode::Final {
            let mut declares = vec![];
            declare_interest(
                ctrl_lock.as_ref(),
                &self.tables,
                &mut self.state.clone(),
                msg.id,
                msg.wire_expr.as_ref(),
                msg.mode,
                msg.options,
                &mut |p, m| declares.push((p.clone(), m)),
            );
            drop(ctrl_lock);
            for (p, m) in declares {
                p.send_declare(m);
            }
        } else {
            undeclare_interest(
                ctrl_lock.as_ref(),
                &self.tables,
                &mut self.state.clone(),
                msg.id,
            );
        }
    }

    fn send_declare(&self, msg: zenoh_protocol::network::Declare) {
        let ctrl_lock = zlock!(self.tables.ctrl_lock);
        match msg.body {
            zenoh_protocol::network::DeclareBody::DeclareKeyExpr(m) => {
                register_expr(&self.tables, &mut self.state.clone(), m.id, &m.wire_expr);
            }
            zenoh_protocol::network::DeclareBody::UndeclareKeyExpr(m) => {
                unregister_expr(&self.tables, &mut self.state.clone(), m.id);
            }
            zenoh_protocol::network::DeclareBody::DeclareSubscriber(m) => {
                let mut declares = vec![];
                declare_subscription(
                    ctrl_lock.as_ref(),
                    &self.tables,
                    &mut self.state.clone(),
                    m.id,
                    &m.wire_expr,
                    &m.ext_info,
                    msg.ext_nodeid.node_id,
                    &mut |p, m| declares.push((p.clone(), m)),
                );
                drop(ctrl_lock);
                for (p, m) in declares {
                    p.send_declare(m);
                }
            }
            zenoh_protocol::network::DeclareBody::UndeclareSubscriber(m) => {
                let mut declares = vec![];
                undeclare_subscription(
                    ctrl_lock.as_ref(),
                    &self.tables,
                    &mut self.state.clone(),
                    m.id,
                    &m.ext_wire_expr.wire_expr,
                    msg.ext_nodeid.node_id,
                    &mut |p, m| declares.push((p.clone(), m)),
                );
                drop(ctrl_lock);
                for (p, m) in declares {
                    p.send_declare(m);
                }
            }
            zenoh_protocol::network::DeclareBody::DeclareQueryable(m) => {
                let mut declares = vec![];
                declare_queryable(
                    ctrl_lock.as_ref(),
                    &self.tables,
                    &mut self.state.clone(),
                    m.id,
                    &m.wire_expr,
                    &m.ext_info,
                    msg.ext_nodeid.node_id,
                    &mut |p, m| declares.push((p.clone(), m)),
                );
                drop(ctrl_lock);
                for (p, m) in declares {
                    p.send_declare(m);
                }
            }
            zenoh_protocol::network::DeclareBody::UndeclareQueryable(m) => {
                let mut declares = vec![];
                undeclare_queryable(
                    ctrl_lock.as_ref(),
                    &self.tables,
                    &mut self.state.clone(),
                    m.id,
                    &m.ext_wire_expr.wire_expr,
                    msg.ext_nodeid.node_id,
                    &mut |p, m| declares.push((p.clone(), m)),
                );
                drop(ctrl_lock);
                for (p, m) in declares {
                    p.send_declare(m);
                }
            }
            zenoh_protocol::network::DeclareBody::DeclareToken(m) => {
                let mut declares = vec![];
                declare_token(
                    ctrl_lock.as_ref(),
                    &self.tables,
                    &mut self.state.clone(),
                    m.id,
                    &m.wire_expr,
                    msg.ext_nodeid.node_id,
                    msg.interest_id,
                    &mut |p, m| declares.push((p.clone(), m)),
                );
                drop(ctrl_lock);
                for (p, m) in declares {
                    p.send_declare(m);
                }
            }
            zenoh_protocol::network::DeclareBody::UndeclareToken(m) => {
                let mut declares = vec![];
                undeclare_token(
                    ctrl_lock.as_ref(),
                    &self.tables,
                    &mut self.state.clone(),
                    m.id,
                    &m.ext_wire_expr,
                    msg.ext_nodeid.node_id,
                    &mut |p, m| declares.push((p.clone(), m)),
                );
                drop(ctrl_lock);
                for (p, m) in declares {
                    p.send_declare(m);
                }
            }
            zenoh_protocol::network::DeclareBody::DeclareFinal(_) => {
                if let Some(id) = msg.interest_id {
                    get_mut_unchecked(&mut self.state.clone())
                        .local_interests
                        .entry(id)
                        .and_modify(|interest| interest.finalized = true);

                    let mut declares = vec![];
                    declare_final(&mut self.state.clone(), id, &mut |p, m| {
                        declares.push((p.clone(), m))
                    });

                    // recompute routes
                    // TODO: disable  routes and recompute them in parallel to avoid holding
                    // tables write lock for a long time.
                    let mut wtables = zwrite!(self.tables.tables);
                    let mut root_res = wtables.root_res.clone();
                    update_data_routes_from(&mut wtables, &mut root_res);
                    update_query_routes_from(&mut wtables, &mut root_res);

                    drop(wtables);
                    drop(ctrl_lock);
                    for (p, m) in declares {
                        p.send_declare(m);
                    }
                }
            }
        }
    }

    #[inline]
    fn send_push(&self, msg: Push) {
        full_reentrant_route_data(
            &self.tables,
            &self.state,
            &msg.wire_expr,
            msg.ext_qos,
            msg.ext_tstamp,
            msg.payload,
            msg.ext_nodeid.node_id,
        );
    }

    fn send_request(&self, msg: Request) {
        match msg.payload {
            RequestBody::Query(_) => {
                route_query(
                    &self.tables,
                    &self.state,
                    &msg.wire_expr,
                    msg.id,
                    msg.ext_qos,
                    msg.ext_tstamp,
                    msg.ext_target,
                    msg.ext_budget,
                    msg.ext_timeout,
                    msg.payload,
                    msg.ext_nodeid.node_id,
                );
            }
        }
    }

    fn send_response(&self, msg: Response) {
        route_send_response(
            &self.tables,
            &mut self.state.clone(),
            msg.rid,
            msg.ext_qos,
            msg.ext_tstamp,
            msg.ext_respid,
            msg.wire_expr,
            msg.payload,
        );
    }

    fn send_response_final(&self, msg: ResponseFinal) {
        route_send_response_final(&self.tables, &mut self.state.clone(), msg.rid);
    }

    fn send_close(&self) {
        tables::close_face(&self.tables, &Arc::downgrade(&self.state));
    }
}

impl fmt::Display for Face {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.state.fmt(f)
    }
}
