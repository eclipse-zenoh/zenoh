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
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use zenoh_protocol::network::{
    declare::queryable::ext::QueryableInfoType, interest::InterestId, Interest,
};

use super::Hat;
use crate::net::routing::{
    dispatcher::{
        interests::{CurrentInterest, RemoteInterest},
        resource::Resource,
        tables::TablesData,
    },
    gateway::SubscriberInfo,
    hat::{
        DispatcherContext, HatBaseTrait, HatInterestTrait, Remote, RouteCurrentDeclareResult,
        RouteInterestResult,
    },
};

impl HatInterestTrait for Hat {
    #[tracing::instrument(level = "debug", skip(ctx, _msg), fields(%src), ret)]
    fn route_interest(
        &mut self,
        ctx: DispatcherContext,
        _msg: &Interest,
        _res: Option<Arc<Resource>>,
        src: &Remote,
    ) -> RouteInterestResult {
        debug_assert!(self.region().bound().is_north());
        debug_assert!(ctx.src_face.region.bound().is_south());

        RouteInterestResult::ResolvedCurrentInterest
    }

    #[tracing::instrument(level = "debug", skip(ctx, _msg), ret)]
    fn route_interest_final(
        &mut self,
        ctx: DispatcherContext,
        _msg: &Interest,
        _remote_interest: &RemoteInterest,
    ) {
        debug_assert!(self.region().bound().is_north());
        debug_assert!(ctx.src_face.region.bound().is_south());
    }

    #[tracing::instrument(level = "debug", skip(ctx), ret)]
    fn route_declare_final(
        &mut self,
        ctx: DispatcherContext,
        _interest_id: InterestId,
    ) -> RouteCurrentDeclareResult {
        debug_assert!(self.region().bound().is_north());
        debug_assert!(ctx.src_face.region.bound().is_south());

        RouteCurrentDeclareResult::Noop
    }

    #[tracing::instrument(level = "debug", skip(ctx), ret)]
    fn route_current_token(
        &mut self,
        ctx: DispatcherContext,
        _interest_id: InterestId,
        _res: Arc<Resource>,
    ) -> RouteCurrentDeclareResult {
        debug_assert!(self.region().bound().is_north());
        debug_assert!(ctx.src_face.region.bound().is_north());

        RouteCurrentDeclareResult::Noop
    }

    #[tracing::instrument(level = "debug", skip(ctx, _msg), ret)]
    fn send_current_subscribers(
        &self,
        ctx: DispatcherContext,
        _msg: &Interest,
        _res: Option<Arc<Resource>>,
        _other_matches: HashMap<Arc<Resource>, SubscriberInfo>,
    ) {
        debug_assert!(self.owns(ctx.src_face));
        debug_assert!(ctx.src_face.region.bound().is_south());
    }

    #[tracing::instrument(level = "debug", skip(ctx, _msg), ret)]
    fn send_current_queryables(
        &self,
        ctx: DispatcherContext,
        _msg: &Interest,
        _res: Option<Arc<Resource>>,
        _other_matches: HashMap<Arc<Resource>, QueryableInfoType>,
    ) {
        debug_assert!(self.owns(ctx.src_face));
        debug_assert!(ctx.src_face.region.bound().is_south());
    }

    #[tracing::instrument(level = "debug", skip(ctx, _msg), ret)]
    fn send_current_tokens(
        &self,
        ctx: DispatcherContext,
        _msg: &Interest,
        res: Option<Arc<Resource>>,
        other_matches: HashSet<Arc<Resource>>,
    ) {
        debug_assert!(self.owns(ctx.src_face));
        debug_assert!(ctx.src_face.region.bound().is_south());
    }

    #[tracing::instrument(level = "debug", skip(ctx), ret)]
    fn propagate_current_token(
        &self,
        ctx: DispatcherContext,
        _res: Arc<Resource>,
        _interest: CurrentInterest,
    ) {
        debug_assert!(self.owns(ctx.src_face));
        debug_assert!(ctx.src_face.region.bound().is_south());
    }

    #[tracing::instrument(level = "debug", skip(_ctx, _dst), ret)]
    fn send_declare_final(&mut self, _ctx: DispatcherContext, _id: InterestId, _dst: &Remote) {}

    #[tracing::instrument(level = "debug", skip(ctx, _msg), ret)]
    fn register_interest(
        &mut self,
        ctx: DispatcherContext,
        _msg: &Interest,
        _res: Option<Arc<Resource>>,
    ) {
        debug_assert!(self.owns(ctx.src_face));
        debug_assert!(self.region().bound().is_south());
    }

    #[tracing::instrument(level = "debug", skip(ctx, _msg), ret)]
    fn unregister_interest(
        &mut self,
        ctx: DispatcherContext,
        _msg: &Interest,
    ) -> Option<RemoteInterest> {
        debug_assert!(self.owns(ctx.src_face));
        debug_assert!(self.region().bound().is_south());

        None
    }

    #[tracing::instrument(level = "trace", skip(_tables), ret)]
    fn remote_interests(&self, _tables: &TablesData) -> HashSet<RemoteInterest> {
        HashSet::default()
    }
}
