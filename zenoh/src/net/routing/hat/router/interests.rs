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

use itertools::Itertools;
use zenoh_config::WhatAmI;
use zenoh_protocol::network::{
    declare,
    interest::{self, InterestId, InterestMode, InterestOptions},
    Declare, DeclareBody, DeclareFinal, Interest,
};
use zenoh_runtime::ZRuntime;

use super::Hat;
use crate::net::routing::{
    dispatcher::{
        gateway::{Bound, BoundMap},
        interests::{CurrentInterest, PendingCurrentInterest, RemoteInterest},
        resource::Resource,
    },
    hat::{BaseContext, CurrentFutureTrait, HatBaseTrait, HatInterestTrait, HatTrait, Remote},
    router::NodeId,
    RoutingContext,
};

impl HatInterestTrait for Hat {
    #[tracing::instrument(level = "trace", skip_all, fields(wai = %self.whatami().short(), bnd = %self.bound))]
    fn route_interest(
        &mut self,
        mut ctx: BaseContext,
        msg: &Interest,
        res: Option<&mut Arc<Resource>>,
        mut south_hats: BoundMap<&mut dyn HatTrait>,
    ) {
        // I have received an interest with mode != FINAL.
        // I should be the north hat.
        // Either I own the src face, in which case msg originates in my region and I need to pass the parcel till the gateway.
        // Or, the face is south-bound, in which case the msg originates in a subregion (for which I am the gateway):
        //   1. If I have a gateway, I should re-propagate the interest to it.
        //   2. If the interest is current, I need to send all current declarations in the (south) owner hat.
        //   3. If the interest is future, I need to register it as a remote interest in the (south) owner hat.

        assert!(self.bound().is_north());

        // REVIEW(regions): mainline zenoh has a failure mode for aggregate interests from peers to routers.
        // See: https://github.com/eclipse-zenoh/zenoh/blob/1bd82eeef7d9b2df0d96dbbaf947ac75c90571aa/zenoh/src/net/routing/hat/router/interests.rs#L53-L59

        let mut msg = msg.clone();

        if msg.options.aggregate() && res.is_none() {
            tracing::warn!(
                "Received Interest with aggregate=true with empty key expression. Not supported!"
            );
            msg.options -= InterestOptions::AGGREGATE;
        }

        if self.owns(ctx.src_face) {
            let src_nid =
                self.map_routing_context(ctx.tables, ctx.src_face, msg.ext_nodeid.node_id);

            if let Some(gwy_node_id) = self.subregion_gateway() {
                self.send_point_to_point(ctx.reborrow(), gwy_node_id, |next_hop| {
                    next_hop.primitives.send_interest(RoutingContext::with_expr(
                        &mut Interest {
                            id: msg.id,
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
                            ext_nodeid: interest::ext::NodeIdType { node_id: src_nid },
                        },
                        res.as_ref()
                            .map(|res| res.expr().to_string())
                            .unwrap_or_default(),
                    ));
                });
            } else {
                tracing::error!(
                    src_nid,
                    id = msg.id,
                    "No gateway found in router region. Cannot route interest message"
                );
            }
        } else {
            let owner_hat = &mut *south_hats[ctx.src_face.local_bound];

            if msg.mode.current() {
                owner_hat.send_declarations(ctx.reborrow(), &msg, res.as_deref().cloned().as_mut());
            }

            if msg.mode.future() {
                owner_hat.register_interest(ctx.reborrow(), &msg, res.as_deref().cloned().as_mut());
            }

            if let Some(gwy_node_id) = self.subregion_gateway() {
                let id = self.next_interest_id;
                self.next_interest_id += 1;

                let src_nid = self.net().idx.index() as NodeId;

                self.send_point_to_point(ctx.reborrow(), gwy_node_id, |next_hop| {
                    next_hop.primitives.send_interest(RoutingContext::with_expr(
                        &mut Interest {
                            id,
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
                            ext_nodeid: interest::ext::NodeIdType { node_id: src_nid },
                        },
                        res.as_ref()
                            .map(|res| res.expr().to_string())
                            .unwrap_or_default(),
                    ));
                });

                if msg.mode.current() {
                    self.register_pending_current_interest(ctx, &msg, owner_hat, id);
                }
            } else {
                tracing::debug!(
                    id = msg.id,
                    src = %ctx.src_face,
                    "No gateway found in router region. Will not propagate interest"
                );

                if msg.mode.current() {
                    tracing::debug!(
                        id = msg.id,
                        src = %ctx.src_face,
                        "Finalizing current interest. It was not propagated upstream"
                    );

                    let Some(dst) = owner_hat.new_remote(ctx.src_face, msg.ext_nodeid.node_id)
                    else {
                        return;
                    };

                    owner_hat.send_final_declaration(ctx, msg.id, &dst);
                }
            }
        }
    }

    #[tracing::instrument(level = "trace", skip_all, fields(wai = %self.whatami().short(), bnd = %self.bound))]
    fn route_interest_final(
        &mut self,
        mut ctx: BaseContext,
        msg: &Interest,
        mut south_hats: BoundMap<&mut dyn HatTrait>,
    ) {
        // I have received an interest with mode FINAL.
        // I should be the north hat.
        // Either I own the src face, in which case msg originates in my region and I need to pass the parcel till the gateway.
        // Or, the face is south-bound, in which case the msg originates in a subregion (for I which am the gateway):
        //   1. I need to unregister it as a remote interest in the owner (south) hat.
        //   2. If I have a gateway, I should re-propagate the FINAL interest to it iff no other subregion has the same remote interest.

        assert!(self.bound().is_north());

        // FIXME(regions): check if any subregion has the same remote interest before propagating the interest final

        // FIXME(regions): compute routing expr for interceptors from the WireExpr (currently empty).
        // This is only relevant for the ingress case, at least in the case of ACL.
        // See: https://github.com/eclipse-zenoh/zenoh/blob/17610bf9e090a70f4347057137bff8b952f4783a/zenoh/src/net/routing/interceptor/access_control.rs#L885-L906

        if self.owns(ctx.src_face) {
            let src_nid =
                self.map_routing_context(ctx.tables, ctx.src_face, msg.ext_nodeid.node_id);

            if let Some(gwy_node_id) = self.subregion_gateway() {
                self.send_point_to_point(ctx, gwy_node_id, |next_hop| {
                    next_hop.primitives.send_interest(RoutingContext::with_expr(
                        &mut Interest {
                            id: msg.id,
                            mode: InterestMode::Final,
                            // NOTE: InterestMode::Final options are undefined in the current protocol specification,
                            // they are initialized here for internal use by local egress interceptors.
                            options: msg.options,
                            wire_expr: None,
                            ext_qos: msg.ext_qos,
                            ext_tstamp: msg.ext_tstamp,
                            ext_nodeid: interest::ext::NodeIdType { node_id: src_nid },
                        },
                        "".to_string(),
                    ));
                });
            } else {
                tracing::error!(
                    bnd = ?self.bound,
                    "No gateway found in router subregion. Will not forward interest finalization"
                );
            }
        } else {
            let owner_hat = &mut *south_hats[ctx.src_face.local_bound];

            let Some(remote_interest) = owner_hat.unregister_interest(ctx.reborrow(), msg) else {
                tracing::error!(id = msg.id, "Unknown remote interest");
                return;
            };

            if let Some(gwy_node_id) = self.subregion_gateway() {
                let src_nid = self.net().idx.index() as NodeId;

                for id in self.router_local_interests.keys().cloned().collect_vec() {
                    if self
                        .router_local_interests
                        .get(&id)
                        .is_some_and(|local_interest| local_interest == &remote_interest)
                    {
                        self.router_local_interests.remove(&id);
                        self.send_point_to_point(ctx.reborrow(), gwy_node_id, |next_hop| {
                            next_hop.primitives.send_interest(RoutingContext::with_expr(
                                &mut Interest {
                                    id,
                                    mode: InterestMode::Final,
                                    // NOTE: InterestMode::Final options are undefined in the current protocol specification,
                                    // they are initialized here for internal use by local egress interceptors.
                                    options: msg.options,
                                    wire_expr: None,
                                    ext_qos: msg.ext_qos,
                                    ext_tstamp: msg.ext_tstamp,
                                    ext_nodeid: interest::ext::NodeIdType { node_id: src_nid },
                                },
                                "".to_string(),
                            ));
                        });
                    }
                }
            } else {
                tracing::debug!(
                    bnd = ?self.bound,
                    "No gateway found in router subregion. Will not propagate interest finalization"
                );
            }
        }
    }

    #[tracing::instrument(level = "trace", skip_all, fields(wai = %self.whatami().short(), bnd = %self.bound))]
    fn route_final_declaration(
        &mut self,
        _ctx: BaseContext,
        _interest_id: InterestId,
        _south_hats: BoundMap<&mut dyn HatTrait>,
    ) {
        // I have received a Declare Final.
        // I should be the north hat.
        // Either the msg is destined for me, in which case there should be a pending current interest with matching interest id
        // and I should call the owner south hat with .send_final_declaration(..).
        // Or, it is destined for another router in my region, in which case I should pass the parcel till the interest source.

        assert!(self.bound().is_north());

        // TODO(regions): this requires a protocol ext for dst NIDs.
        unimplemented!()
    }

    #[tracing::instrument(level = "trace", skip_all, fields(wai = %self.whatami().short(), bnd = %self.bound))]
    fn send_declarations(
        &mut self,
        ctx: BaseContext,
        msg: &Interest,
        res: Option<&mut Arc<Resource>>,
    ) {
        // I have received a current or current-future interest from my subregion.
        //   1. If the interest is current, I need to send all current declarations with the source IID
        //   2. If the interest is future, I need to register it as a remote interest identified by ZID and IID

        assert!(!self.bound().is_north());
        self.assert_proper_ownership(&ctx);

        let src_node_id = self.net().idx.index() as NodeId;

        // FIXME(regions): send to the proper next-hop face in the tree centered at the source

        if msg.options.subscribers() {
            self.declare_sub_interest(
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
            self.declare_qabl_interest(
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
            // Note: aggregation is forbidden for tokens. The flag is ignored.
            self.declare_token_interest(
                ctx.tables,
                ctx.src_face,
                msg.id,
                res.as_deref().cloned().as_mut(),
                msg.mode,
                src_node_id,
                ctx.send_declare,
            )
        }
    }

    #[tracing::instrument(level = "trace", skip_all, fields(wai = %self.whatami().short(), bnd = %self.bound))]
    fn send_final_declaration(&mut self, mut ctx: BaseContext, id: InterestId, src: &Remote) {
        // I should send a DeclareFinal to the source of the current interest identified by the given IID and ZID

        let zid = self.hat_remote(src);

        let Some(dst_node_id) = self.net().get_idx(zid).map(|idx| idx.index() as NodeId) else {
            tracing::error!(zid = zid.short(), "ZID not found in router network");
            return;
        };

        self.send_declare_point_to_point(ctx.reborrow(), dst_node_id, |send_declare, next_hop| {
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
        });
    }

    #[tracing::instrument(level = "trace", skip_all, fields(wai = %self.whatami().short(), bnd = %self.bound))]
    fn register_interest(
        &mut self,
        ctx: BaseContext,
        msg: &Interest,
        res: Option<&mut Arc<Resource>>,
    ) {
        assert!(!self.bound().is_north());
        self.assert_proper_ownership(&ctx);

        let Some(zid) = self.get_router(ctx.src_face, msg.ext_nodeid.node_id) else {
            tracing::error!(
                face = %ctx.src_face,
                nid = msg.ext_nodeid.node_id,
                "Unknown interest source"
            );
            return;
        };

        self.router_remote_interests.insert(
            (zid, msg.id),
            RemoteInterest {
                res: res.cloned(),
                options: msg.options,
                mode: msg.mode,
            },
        );
    }

    #[tracing::instrument(level = "trace", skip_all, fields(wai = %self.whatami().short(), bnd = %self.bound))]
    fn unregister_interest(&mut self, ctx: BaseContext, msg: &Interest) -> Option<RemoteInterest> {
        assert!(!self.bound().is_north());
        self.assert_proper_ownership(&ctx);

        let Some(zid) = self.get_router(ctx.src_face, msg.ext_nodeid.node_id) else {
            tracing::error!(
                face = %ctx.src_face,
                nid = msg.ext_nodeid.node_id,
                "Unknown interest source"
            );
            return None;
        };

        // TODO(regions): how should the simple/aggregated resource distinction apply to router regions? (see 20a95fb)

        self.router_remote_interests.remove(&(zid, msg.id))
    }
}

impl Hat {
    fn register_pending_current_interest(
        &mut self,
        ctx: BaseContext,
        msg: &Interest,
        owner_hat: &mut dyn HatTrait,
        id: InterestId,
    ) {
        let Some(src) = owner_hat.new_remote(ctx.src_face, msg.ext_nodeid.node_id) else {
            return;
        };

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

            let cleanup = move |print_warning| {
                let _span = tracing::trace_span!(
                    "cleanup_pending_current_interest",
                    wai = %WhatAmI::Router.short(),
                    bnd = %Bound::north(),
                    id
                )
                .entered();

                let ctrl_lock = zlock!(tables_lock.ctrl_lock);
                let mut wtables = zwrite!(tables_lock.tables);
                let tables = &mut *wtables;

                let this = tables.hats[Bound::north()]
                    .as_any_mut()
                    .downcast_mut::<Hat>()
                    .unwrap();

                let Some(PendingCurrentInterest {
                    interest,
                    cancellation_token,
                    ..
                }) = this.router_pending_current_interests.remove(&id)
                else {
                    tracing::error!(id, "Unknown current interest");
                    return;
                };

                drop(ctrl_lock);

                cancellation_token.cancel();

                if print_warning {
                    tracing::warn!(
                        id = id,
                        timeout = ?interests_timeout,
                        "Gateway failed to finalize declarations for current interest before timeout",
                    );
                }

                tracing::debug!(
                    interest.src_interest_id,
                    "Finalizing declarations to source point of current interest",
                );

                let Some(mut src_face) = src_face.upgrade() else {
                    // FIXME(regions): this src face is not strictly needed,
                    // but a consequence of the BaseContext type definition.
                    // Hat impls should use Remote instead
                    tracing::error!("Could not get strong count on interest source face");
                    return;
                };

                tables.hats[interest.src_bound].send_final_declaration(
                    BaseContext {
                        tables_lock: &tables_lock,
                        tables: &mut tables.data,
                        src_face: &mut src_face,
                        send_declare: &mut |p, m| m.with_mut(|m| p.send_declare(m)),
                    },
                    interest.src_interest_id,
                    &interest.src,
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

        self.router_pending_current_interests.insert(
            id,
            PendingCurrentInterest {
                interest: Arc::new(CurrentInterest {
                    src,
                    src_interest_id: id,
                    src_bound: ctx.src_face.local_bound,
                    mode: msg.mode,
                }),
                cancellation_token,
                rejection_token,
            },
        );
    }
}
