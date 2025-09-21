//
// Copyright (c) 2024 ZettaScale Technology
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
use std::sync::Arc;

use zenoh_protocol::{
    core::{WhatAmI, ZenohIdProto},
    network::{
        declare,
        interest::{self, InterestId, InterestMode, InterestOptions},
        Declare, DeclareBody, DeclareFinal, Interest,
    },
};
use zenoh_runtime::ZRuntime;
use zenoh_sync::get_mut_unchecked;

use super::Hat;
use crate::net::routing::{
    dispatcher::{
        face::FaceState,
        gateway::{BoundMap, GatewayPendingCurrentInterest},
        interests::{finalize_pending_interest, CurrentInterest, RemoteInterest},
        resource::Resource,
    },
    hat::{BaseContext, CurrentFutureTrait, HatBaseTrait, HatInterestTrait, HatTrait, SendDeclare},
    router::NodeId,
    RoutingContext,
};

impl HatInterestTrait for Hat {
    #[tracing::instrument(level = "trace", skip_all, fields(wai = %self.whatami().short(), bnd = %self.bound))]
    fn propagate_declarations(
        &mut self,
        mut ctx: BaseContext,
        msg: &Interest,
        res: Option<&mut Arc<Resource>>,
        upstream_hat: &mut dyn HatTrait,
    ) {
        match ctx.src_face.whatami {
            WhatAmI::Router => {
                self.handle_sourced_interest(ctx.reborrow(), msg, res.as_deref().cloned().as_mut());
            }
            WhatAmI::Peer | WhatAmI::Client => {
                self.handle_simple_interest(ctx.reborrow(), msg, res.as_deref().cloned().as_mut());
            }
        }

        let Some(src_zid) = self.node_id_to_zid(ctx.src_face, msg.ext_nodeid.node_id) else {
            tracing::error!(
                src_interest_id = msg.id,
                "Could not determine interest finalization source zid"
            );
            return;
        };

        if !upstream_hat.propagate_interest(ctx.reborrow(), msg, res, &src_zid)
            && msg.mode.current()
        {
            tracing::trace!(
                id = msg.id,
                src = %ctx.src_face,
                "Finalizing current interest; it was not propagated upstream"
            );
            self.finalize_current_interest(ctx, msg.id, &src_zid);
        }
    }

    #[tracing::instrument(level = "trace", skip_all, fields(wai = %self.whatami().short(), bnd = %self.bound))]
    fn propagate_interest(
        &mut self,
        mut ctx: BaseContext,
        msg: &Interest,
        res: Option<&mut Arc<Resource>>,
        zid: &ZenohIdProto,
    ) -> bool {
        let mut msg = msg.clone();

        if msg.options.aggregate() && ctx.src_face.whatami == WhatAmI::Peer {
            tracing::warn!(
                "Received Interest with aggregate=true from peer {}. Not supported!",
                ctx.src_face.zid
            );
            msg.options -= InterestOptions::AGGREGATE;
        }

        if self.owns(ctx.src_face) {
            // relay
            let src_node_id =
                self.map_routing_context(ctx.tables, ctx.src_face, msg.ext_nodeid.node_id);
            self.send_sourced_interest_to_gateway(ctx, msg.id, src_node_id, &msg, res);
        } else {
            // exit relay
            let interest_id = self.gateway_next_interest_id;
            self.gateway_next_interest_id += 1;

            let is_sent = self.send_sourced_interest_to_gateway(
                ctx.reborrow(),
                interest_id,
                self.net().idx.index() as NodeId,
                &msg,
                res,
            );

            if is_sent && msg.mode.current() {
                self.register_pending_current_interest(ctx, &msg, interest_id, zid);
                return true;
            }
        }

        false
    }

    #[tracing::instrument(level = "trace", skip_all, fields(wai = %self.whatami().short(), bnd = %self.bound))]
    fn unregister_current_interest(
        &mut self,
        ctx: BaseContext,
        id: InterestId,
        mut downstream_hats: BoundMap<&mut dyn HatTrait>,
    ) {
        if let Some(interest) = get_mut_unchecked(&mut ctx.src_face.clone())
            .pending_current_interests
            .remove(&id)
        {
            finalize_pending_interest(interest, ctx.send_declare);
        }

        if let Some(pending_interest) = self.gateway_pending_current_interests.remove(&id) {
            pending_interest.cancellation_token.cancel();
            let hat = &mut downstream_hats[pending_interest.interest.src_face.bound];
            hat.finalize_current_interest(ctx, id, &pending_interest.src_zid);
        };
    }

    #[tracing::instrument(level = "trace", skip_all, fields(wai = %self.whatami().short(), bnd = %self.bound))]
    fn finalize_current_interest(
        &mut self,
        mut ctx: BaseContext,
        id: InterestId,
        zid: &ZenohIdProto,
    ) {
        if self.router_remote_interests.contains_key(&(*zid, id)) {
            let Some(dst_node_id) = self.net().get_idx(zid).map(|idx| idx.index() as NodeId) else {
                return;
            };

            self.send_declare_point_to_point(
                ctx.reborrow(),
                dst_node_id,
                |send_declare, next_hop| {
                    send_declare(
                        &next_hop.primitives,
                        RoutingContext::new(Declare {
                            interest_id: Some(id),
                            ext_qos: declare::ext::QoSType::DECLARE,
                            ext_tstamp: None,
                            ext_nodeid: declare::ext::NodeIdType {
                                node_id: dst_node_id,
                            },
                            body: DeclareBody::DeclareFinal(DeclareFinal),
                        }),
                    );
                },
            );
        } else if let Some(src_face) = self.face(ctx.tables, zid) {
            (ctx.send_declare)(
                &src_face.primitives,
                RoutingContext::new(Declare {
                    interest_id: Some(id),
                    ext_qos: declare::ext::QoSType::DECLARE,
                    ext_tstamp: None,
                    ext_nodeid: declare::ext::NodeIdType::DEFAULT,
                    body: DeclareBody::DeclareFinal(DeclareFinal),
                }),
            );
        } else {
            todo!()
        }
    }

    #[tracing::instrument(level = "trace", skip_all, fields(wai = %self.whatami().short(), bnd = %self.bound))]
    fn unregister_interest(
        &mut self,
        mut ctx: BaseContext,
        msg: &Interest,
        upstream_hat: &mut dyn HatTrait,
    ) {
        match ctx.src_face.whatami {
            WhatAmI::Router => {
                if let Some(src_zid) = self.node_id_to_zid(ctx.src_face, msg.ext_nodeid.node_id) {
                    if let Some(interest) = self.router_remote_interests.remove(&(src_zid, msg.id))
                    {
                        if self.inbound_interest_ref_count(ctx.reborrow(), &interest) == 0 {
                            upstream_hat.finalize_interest(ctx, msg, interest);
                        }
                    }
                } else {
                    tracing::error!(
                        src_interest_id = msg.id,
                        "Could not determine interest finalization source zid"
                    );
                };
            }
            WhatAmI::Peer | WhatAmI::Client => {
                if let Some(interest) = self
                    .face_hat_mut(ctx.src_face)
                    .remote_interests
                    .remove(&msg.id)
                {
                    if self.inbound_interest_ref_count(ctx.reborrow(), &interest) == 0 {
                        upstream_hat.finalize_interest(ctx, msg, interest);
                    }
                }
            }
        }
    }

    #[tracing::instrument(level = "trace", skip_all, fields(wai = %self.whatami().short(), bnd = %self.bound))]
    fn finalize_interest(
        &mut self,
        mut ctx: BaseContext,
        msg: &Interest,
        inbound_interest: RemoteInterest,
    ) {
        if self.owns(ctx.src_face) {
            // relay
            let src_node_id =
                self.map_routing_context(ctx.tables, ctx.src_face, msg.ext_nodeid.node_id);
            self.send_sourced_interest_finalization_to_gateway(
                ctx,
                msg.id,
                src_node_id,
                msg,
                inbound_interest.res.clone().as_mut(),
            );
        } else {
            // exit relay
            for (id, outbound_interest) in self.gateway_local_interests.iter() {
                if inbound_interest.options == outbound_interest.options
                    && inbound_interest.res == outbound_interest.res
                {
                    self.send_sourced_interest_finalization_to_gateway(
                        ctx.reborrow(),
                        *id,
                        self.net().idx.index() as NodeId,
                        msg,
                        outbound_interest.res.clone().as_mut(),
                    );
                }
            }
        }
    }
}

impl Hat {
    /// Sends the interest on the next-hop on the route to the router subregion gateway.
    fn send_sourced_interest_to_gateway(
        &self,
        ctx: BaseContext,
        interest_id: InterestId,
        src_node_id: NodeId,
        msg: &Interest,
        res: Option<&mut Arc<Resource>>,
    ) -> bool {
        let Some(gwy_node_id) = self.subregion_gateway() else {
            tracing::debug!(
                bnd = ?self.bound,
                "No gateway found in router subregion. \
                Will not forward sourced interest message"
            );
            return false;
        };

        self.send_point_to_point(ctx, gwy_node_id, |next_hop| {
            next_hop.primitives.send_interest(RoutingContext::with_expr(
                &mut Interest {
                    id: interest_id,
                    mode: if msg.mode == InterestMode::Future {
                        InterestMode::CurrentFuture
                    } else {
                        msg.mode
                    },
                    options: msg.options,
                    wire_expr: res.as_ref().map(|res| {
                        let push = self.push_declaration_profile(next_hop);
                        Resource::decl_key(res, &mut next_hop.clone(), push)
                    }),
                    ext_qos: msg.ext_qos,
                    ext_tstamp: msg.ext_tstamp,
                    ext_nodeid: interest::ext::NodeIdType {
                        node_id: src_node_id,
                    },
                },
                res.as_ref()
                    .map(|res| res.expr().to_string())
                    .unwrap_or_default(),
            ));
        });

        true
    }

    /// Sends the interest finalization on the next-hop on the route to the router subregion gateway.
    fn send_sourced_interest_finalization_to_gateway(
        &self,
        ctx: BaseContext,
        interest_id: InterestId,
        src_node_id: NodeId,
        msg: &Interest,
        res: Option<&mut Arc<Resource>>,
    ) {
        let Some(gwy_node_id) = self.subregion_gateway() else {
            tracing::debug!(
                bnd = ?self.bound,
                "No gateway found in router subregion. \
                Will not forward sourced interest finalization message"
            );
            return;
        };

        self.send_point_to_point(ctx, gwy_node_id, |next_hop| {
            next_hop.primitives.send_interest(RoutingContext::with_expr(
                &mut Interest {
                    id: interest_id,
                    mode: InterestMode::Final,
                    // NOTE: InterestMode::Final options are undefined in the current protocol specification,
                    // they are initialized here for internal use by local egress interceptors.
                    options: msg.options,
                    wire_expr: res.as_ref().map(|res| {
                        let push = self.push_declaration_profile(next_hop);
                        Resource::decl_key(res, &mut next_hop.clone(), push)
                    }),
                    ext_qos: msg.ext_qos,
                    ext_tstamp: msg.ext_tstamp,
                    ext_nodeid: interest::ext::NodeIdType {
                        node_id: src_node_id,
                    },
                },
                res.as_ref()
                    .map(|res| res.expr().to_string())
                    .unwrap_or_default(),
            ));
        });
    }

    fn register_pending_current_interest(
        &mut self,
        ctx: BaseContext,
        msg: &Interest,
        interest_id: InterestId,
        src_zid: &ZenohIdProto,
    ) {
        let cancellation_token = self.task_controller.get_cancellation_token();
        let rejection_token = self.task_controller.get_cancellation_token();

        // NOTE(regions): finalize declaration if:
        //   1. timeout expires (task)
        //   2. interest is rejected by an interceptor (by reject_interest)
        //   3. gateway finalized declaration (by task)
        {
            let cancellation_token = cancellation_token.clone();
            let rejection_token = rejection_token.clone();
            let interests_timeout = ctx.tables.interests_timeout;
            let src_face = Arc::downgrade(ctx.src_face);
            let tables_lock = ctx.tables_lock.clone();
            let bound = self.bound;

            let cleanup = move |print_warning| {
                let Some(mut src_face) = src_face.upgrade() else {
                    return;
                };
                let ctrl_lock = zlock!(tables_lock.ctrl_lock);
                let mut wtables = zwrite!(tables_lock.tables);
                let this = wtables.hats[bound]
                    .as_any_mut()
                    .downcast_mut::<Self>()
                    .unwrap();
                let Some(current_interest) =
                    this.gateway_pending_current_interests.remove(&interest_id)
                else {
                    return;
                };

                drop(ctrl_lock);

                if print_warning {
                    tracing::warn!(
                        interest_id,
                        ?interests_timeout,
                        "Gateway failed to finalize declarations for current interest before timeout",
                    );
                }

                tracing::debug!(
                    ?current_interest.interest.src_face,
                    ?current_interest.interest.src_interest_id,
                    ?current_interest.src_zid,
                    "Finalizing declarations to source face of current interest",
                );
                let tables = &mut *wtables;
                tables.hats[src_face.bound].finalize_current_interest(
                    BaseContext {
                        tables_lock: &tables_lock,
                        tables: &mut tables.data,
                        src_face: &mut src_face,
                        send_declare: &mut |p, m| m.with_mut(|m| p.send_declare(m)),
                    },
                    current_interest.interest.src_interest_id,
                    &current_interest.src_zid,
                );
            };

            self.task_controller
                .spawn_with_rt(ZRuntime::Net, async move {
                    tokio::select! {
                        _ = cancellation_token.cancelled() => {}
                        _ = tokio::time::sleep(interests_timeout) => { cleanup(true); }
                        _ = rejection_token.cancelled() => { cleanup(false); }
                    }
                });
        }

        // state: sent dst zid (gateway) current interest with id
        self.gateway_pending_current_interests.insert(
            interest_id,
            GatewayPendingCurrentInterest {
                src_zid: *src_zid,
                interest: CurrentInterest {
                    mode: msg.mode,
                    src_face: ctx.src_face.clone(),
                    src_interest_id: msg.id,
                },
                cancellation_token,
                rejection_token,
            },
        );
    }

    /// Handles interest from peers and clients.
    fn handle_simple_interest(
        &self,
        ctx: BaseContext,
        msg: &Interest,
        res: Option<&mut Arc<Resource>>,
    ) {
        if msg.options.subscribers() {
            self.declare_sub_interest(
                ctx.tables,
                ctx.src_face,
                msg.id,
                res.as_deref().cloned().as_mut(),
                msg.mode,
                msg.options.aggregate(),
                ctx.send_declare,
            )
        }
        if msg.options.queryables() {
            self.declare_qabl_interest(
                ctx.tables,
                ctx.src_face,
                msg.id,
                res.as_deref().cloned().as_mut(),
                msg.mode,
                msg.options.aggregate(),
                ctx.send_declare,
            )
        }
        if msg.options.tokens() {
            self.declare_token_interest(
                ctx.tables,
                ctx.src_face,
                msg.id,
                res.as_deref().cloned().as_mut(),
                msg.mode,
                msg.options.aggregate(),
                ctx.send_declare,
            )
        }

        if msg.mode.future() {
            self.face_hat_mut(ctx.src_face).remote_interests.insert(
                msg.id,
                RemoteInterest {
                    res: res.as_deref().cloned(),
                    options: msg.options,
                    mode: msg.mode,
                },
            );
        }
    }

    /// Handles interest from routers.
    fn handle_sourced_interest(
        &mut self,
        ctx: BaseContext,
        msg: &Interest,
        res: Option<&mut Arc<Resource>>,
    ) {
        let src_node_id = self.net().idx.index() as NodeId;

        // FIXME(regions): send to the proper next-hop face in the tree centered at the source

        if msg.options.subscribers() {
            self.declare_sub_interest_with_source(
                ctx.tables,
                ctx.src_face,
                msg.id,
                res.as_deref().cloned().as_mut(),
                msg.mode,
                msg.options.aggregate(),
                src_node_id,
                ctx.send_declare,
            )
        }
        if msg.options.queryables() {
            self.declare_qabl_interest_with_source(
                ctx.tables,
                ctx.src_face,
                msg.id,
                res.as_deref().cloned().as_mut(),
                msg.mode,
                msg.options.aggregate(),
                src_node_id,
                ctx.send_declare,
            )
        }
        if msg.options.tokens() {
            self.declare_token_interest_with_source(
                ctx.tables,
                ctx.src_face,
                msg.id,
                res.as_deref().cloned().as_mut(),
                msg.mode,
                msg.options.aggregate(),
                src_node_id,
                ctx.send_declare,
            )
        }

        if msg.mode.future() {
            let Some(src_zid) = self.get_router(ctx.src_face, msg.ext_nodeid.node_id) else {
                return;
            };
            // state: src has an interest
            self.router_remote_interests.insert(
                (src_zid, msg.id),
                RemoteInterest {
                    res: res.as_deref().cloned(),
                    options: msg.options,
                    mode: msg.mode,
                },
            );
        }
    }

    fn inbound_interest_ref_count(&self, ctx: BaseContext, interest: &RemoteInterest) -> usize {
        let sourced_interest_count = self
            .router_remote_interests
            .values()
            .filter(|i| *i == interest)
            .count();

        let interest_count = self
            .faces(ctx.tables)
            .values()
            .map(|f| {
                self.face_hat(f)
                    .remote_interests
                    .values()
                    .filter(|i| *i == interest)
                    .count()
            })
            .sum::<usize>();

        sourced_interest_count + interest_count
    }
}
