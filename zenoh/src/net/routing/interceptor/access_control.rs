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

//! ⚠️ WARNING ⚠️
//!
//! This module is intended for Zenoh's internal use.
//!
//! [Click here for Zenoh's documentation](../zenoh/index.html)

use crate::KeyExpr;
use std::any::Any;
use std::sync::Arc;
use zenoh_config::{AclConfig, Action, Permission, Subject, ZenohId};
use zenoh_protocol::{
    network::{NetworkBody, NetworkMessage, Push, Request},
    zenoh::{PushBody, RequestBody},
};
use zenoh_result::ZResult;
use zenoh_transport::{multicast::TransportMulticast, unicast::TransportUnicast};

use crate::net::routing::RoutingContext;

use super::{
    authorization::PolicyEnforcer, EgressInterceptor, IngressInterceptor, InterceptorFactory,
    InterceptorFactoryTrait, InterceptorTrait,
};
pub(crate) struct AclEnforcer {
    pub(crate) enforcer: Arc<PolicyEnforcer>,
}
struct EgressAclEnforcer {
    policy_enforcer: Arc<PolicyEnforcer>,
    interface_list: Vec<i32>,
    //default_permission: bool,
    zid: ZenohId,
}
struct IngressAclEnforcer {
    policy_enforcer: Arc<PolicyEnforcer>,
    interface_list: Vec<i32>,
    //default_permission: bool,
    zid: ZenohId,
}

pub(crate) fn acl_interceptor_factories(acl_config: AclConfig) -> ZResult<Vec<InterceptorFactory>> {
    let mut res: Vec<InterceptorFactory> = vec![];

    if acl_config.enabled {
        let mut policy_enforcer = PolicyEnforcer::new();
        match policy_enforcer.init(acl_config) {
            Ok(_) => {
                log::info!("[ACCESS LOG]: Access control is enabled and initialized");
                res.push(Box::new(AclEnforcer {
                    enforcer: Arc::new(policy_enforcer),
                }))
            }
            Err(e) => log::error!(
                "[ACCESS LOG]: Access control enabled but not initialized with error {}!",
                e
            ),
        }
    } else {
        log::warn!("[ACCESS LOG]: Access Control is disabled in config!");
    }

    Ok(res)
}

impl InterceptorFactoryTrait for AclEnforcer {
    fn new_transport_unicast(
        &self,
        transport: &TransportUnicast,
    ) -> (Option<IngressInterceptor>, Option<EgressInterceptor>) {
        let mut zid: ZenohId = ZenohId::default();
        match transport.get_zid() {
            Ok(id) => {
                zid = id;
            }
            Err(e) => {
                log::error!("[ACCESS LOG]: Failed to get zid with error :{}", e);
                return (None, None);
            }
        }
        let mut interface_list: Vec<i32> = Vec::new();
        match transport.get_links() {
            Ok(links) => {
                for link in links {
                    let enforcer = self.enforcer.clone();
                    if let Some(subject_map) = &enforcer.subject_map {
                        for face in link.interfaces {
                            let subject = &Subject::Interface(face);
                            if let Some(val) = subject_map.get(subject) {
                                interface_list.push(*val);
                            }
                        }
                    }
                }
            }
            Err(e) => {
                log::error!(
                    "[ACCESS LOG]: Couldn't get interface list with error: {}",
                    e
                );
                return (None, None);
            }
        }
        (
            Some(Box::new(IngressAclEnforcer {
                policy_enforcer: self.enforcer.clone(),
                interface_list: interface_list.clone(),

                zid,
            })),
            Some(Box::new(EgressAclEnforcer {
                policy_enforcer: self.enforcer.clone(),
                interface_list,
                zid,
            })),
        )
    }

    fn new_transport_multicast(
        &self,
        _transport: &TransportMulticast,
    ) -> Option<EgressInterceptor> {
        log::debug!("[ACCESS LOG]: Transport Multicast is not enabled in interceptor");
        None
    }

    fn new_peer_multicast(&self, _transport: &TransportMulticast) -> Option<IngressInterceptor> {
        log::debug!("[ACCESS LOG]: Peer Multicast is not enabled in interceptor");

        None
    }
}

impl InterceptorTrait for IngressAclEnforcer {
    fn compute_keyexpr_cache(&self, key_expr: &KeyExpr<'_>) -> Option<Box<dyn Any + Send + Sync>> {
        Some(Box::new(key_expr.to_string()))
    }
    fn intercept<'a>(
        &self,
        ctx: RoutingContext<NetworkMessage>,
        cache: Option<&Box<dyn Any + Send + Sync>>,
    ) -> Option<RoutingContext<NetworkMessage>> {
        let key_expr = cache
            .and_then(|i| i.downcast_ref::<String>().map(|e| e.as_str()))
            .or_else(|| ctx.full_expr())?;
        if let NetworkBody::Push(Push {
            payload: PushBody::Put(_),
            ..
        }) = &ctx.msg.body
        {
            let mut decision = self.policy_enforcer.default_permission.clone();
            for subject in &self.interface_list {
                match self
                    .policy_enforcer
                    .policy_decision_point(*subject, Action::Put, key_expr)
                {
                    Ok(Permission::Allow) => {
                        decision = Permission::Allow;
                        break;
                    }
                    Ok(Permission::Deny) => {
                        decision = Permission::Deny;
                        continue;
                    }
                    Err(e) => {
                        log::debug!("[ACCESS LOG]: Authorization incomplete due to error {}", e);
                        return None;
                    }
                }
            }

            if decision == Permission::Deny {
                log::debug!("[ACCESS LOG]: {} is unauthorized to Put", self.zid);
                return None;
            }
            log::trace!("[ACCESS LOG]: {} is authorized access to Put", self.zid);
        } else if let NetworkBody::Request(Request {
            payload: RequestBody::Query(_),
            ..
        }) = &ctx.msg.body
        {
            let mut decision = self.policy_enforcer.default_permission.clone();
            for subject in &self.interface_list {
                match self
                    .policy_enforcer
                    .policy_decision_point(*subject, Action::Get, key_expr)
                {
                    Ok(Permission::Allow) => {
                        decision = Permission::Allow;
                        break;
                    }
                    Ok(Permission::Deny) => {
                        decision = Permission::Deny;
                        continue;
                    }
                    Err(e) => {
                        log::debug!("[ACCESS LOG]: Authorization incomplete due to error {}", e);
                        return None;
                    }
                }
            }
            if decision == Permission::Deny {
                log::warn!("[ACCESS LOG]: {} is unauthorized to Query/Get", self.zid);
                return None;
            }

            log::trace!("[ACCESS LOG]: {} is authorized access to Query", self.zid);
        }
        Some(ctx)
    }
}

impl InterceptorTrait for EgressAclEnforcer {
    fn compute_keyexpr_cache(&self, key_expr: &KeyExpr<'_>) -> Option<Box<dyn Any + Send + Sync>> {
        Some(Box::new(key_expr.to_string()))
    }
    fn intercept(
        &self,
        ctx: RoutingContext<NetworkMessage>,
        cache: Option<&Box<dyn Any + Send + Sync>>,
    ) -> Option<RoutingContext<NetworkMessage>> {
        let key_expr = cache
            .and_then(|i| i.downcast_ref::<String>().map(|e| e.as_str()))
            .or_else(|| ctx.full_expr())?;
        if let NetworkBody::Push(Push {
            payload: PushBody::Put(_),
            ..
        }) = &ctx.msg.body
        {
            let mut decision = self.policy_enforcer.default_permission.clone();
            for subject in &self.interface_list {
                match self.policy_enforcer.policy_decision_point(
                    *subject,
                    Action::DeclareSubscriber,
                    key_expr,
                ) {
                    Ok(Permission::Allow) => {
                        decision = Permission::Allow;
                        break;
                    }
                    Ok(Permission::Deny) => {
                        decision = Permission::Deny;
                        continue;
                    }
                    Err(e) => {
                        log::debug!("[ACCESS LOG]: Authorization incomplete due to error {}", e);
                        return None;
                    }
                }
            }
            if decision == Permission::Deny {
                log::debug!(
                    "[ACCESS LOG]: {} is unauthorized to be Subscriber",
                    self.zid
                );
                return None;
            }

            log::trace!(
                "[ACCESS LOG]: {} is authorized access to be Subscriber",
                self.zid
            );
        } else if let NetworkBody::Request(Request {
            payload: RequestBody::Query(_),
            ..
        }) = &ctx.msg.body
        {
            let mut decision = self.policy_enforcer.default_permission.clone();
            for subject in &self.interface_list {
                match self.policy_enforcer.policy_decision_point(
                    *subject,
                    Action::DeclareQueryable,
                    key_expr,
                ) {
                    Ok(Permission::Allow) => {
                        decision = Permission::Allow;
                        break;
                    }
                    Ok(Permission::Deny) => {
                        decision = Permission::Deny;
                        continue;
                    }
                    Err(e) => {
                        log::debug!("[ACCESS LOG]: Authorization incomplete due to error {}", e);
                        return None;
                    }
                }
            }
            if decision == Permission::Deny {
                log::debug!("[ACCESS LOG]: {} is unauthorized to be Queryable", self.zid);
                return None;
            }

            log::trace!(
                "[ACCESS LOG]: {} is authorized access to be Queryable",
                self.zid
            );
        }

        Some(ctx)
    }
}

// pub fn decide_permission() -> ZResult<Permission> {
//     Ok(Permission::Deny)
// }

#[cfg(tests)]
mod tests {

    pub fn allow_then_deny() {}
}
