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
//use zenoh_keyexpr::key_expr;
use zenoh_protocol::{
    network::{Declare, DeclareBody, NetworkBody, NetworkMessage, Push, Request},
    zenoh::{PushBody, RequestBody},
};
use zenoh_result::ZResult;
use zenoh_transport::{multicast::TransportMulticast, unicast::TransportUnicast};

use crate::net::routing::RoutingContext;

use super::{
    authorization::Flow, authorization::PolicyEnforcer, EgressInterceptor, IngressInterceptor,
    InterceptorFactory, InterceptorFactoryTrait, InterceptorTrait,
};
pub(crate) struct AclEnforcer {
    pub(crate) enforcer: Arc<PolicyEnforcer>,
}
struct EgressAclEnforcer {
    policy_enforcer: Arc<PolicyEnforcer>,
    interface_list: Vec<i32>,
    zid: ZenohId,
}
struct IngressAclEnforcer {
    policy_enforcer: Arc<PolicyEnforcer>,
    interface_list: Vec<i32>,
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
        match transport.get_zid() {
            Ok(zid) => {
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
            Err(e) => {
                log::error!("[ACCESS LOG]: Failed to get zid with error :{}", e);
                (None, None)
            }
        }
    }

    fn new_transport_multicast(
        &self,
        _transport: &TransportMulticast,
    ) -> Option<EgressInterceptor> {
        log::debug!("[ACCESS LOG]: Transport Multicast is disabled in interceptor");
        None
    }

    fn new_peer_multicast(&self, _transport: &TransportMulticast) -> Option<IngressInterceptor> {
        log::debug!("[ACCESS LOG]: Peer Multicast is disabled in interceptor");
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
            if self.put(key_expr) == Permission::Deny {
                return None;
            }
        } else if let NetworkBody::Request(Request {
            payload: RequestBody::Query(_),
            ..
        }) = &ctx.msg.body
        {
            if self.get(key_expr) == Permission::Deny {
                return None;
            }
        } else if let NetworkBody::Declare(Declare {
            body: DeclareBody::DeclareSubscriber(_),
            ..
        }) = &ctx.msg.body
        {
            if self.declare_subscriber(key_expr) == Permission::Deny {
                return None;
            }
        } else if let NetworkBody::Declare(Declare {
            body: DeclareBody::DeclareQueryable(_),
            ..
        }) = &ctx.msg.body
        {
            if self.declare_queryable(key_expr) == Permission::Deny {
                return None;
            }
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
            if self.put(key_expr) == Permission::Deny {
                return None;
            }
        } else if let NetworkBody::Request(Request {
            payload: RequestBody::Query(_),
            ..
        }) = &ctx.msg.body
        {
            if self.get(key_expr) == Permission::Deny {
                return None;
            }
        } else if let NetworkBody::Declare(Declare {
            body: DeclareBody::DeclareSubscriber(_),
            ..
        }) = &ctx.msg.body
        {
            if self.declare_subscriber(key_expr) == Permission::Deny {
                return None;
            }
        } else if let NetworkBody::Declare(Declare {
            body: DeclareBody::DeclareQueryable(_),
            ..
        }) = &ctx.msg.body
        {
            if self.declare_queryable(key_expr) == Permission::Deny {
                return None;
            }
        }
        Some(ctx)
    }
}

pub trait AclActionMethods {
    fn policy_enforcer(&self) -> Arc<PolicyEnforcer>;
    fn interface_list(&self) -> Vec<i32>;
    fn zid(&self) -> ZenohId;
    fn flow(&self) -> Flow;

    fn put(&self, key_expr: &str) -> Permission {
        let policy_enforcer = self.policy_enforcer();
        let interface_list = self.interface_list();
        let zid = self.zid();
        let mut decision = policy_enforcer.default_permission.clone();
        for subject in &interface_list {
            match policy_enforcer.policy_decision_point(
                *subject,
                self.flow(),
                Action::Put,
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
                    return Permission::Deny;
                }
            }
        }

        if decision == Permission::Deny {
            log::debug!("[ACCESS LOG]: {} is unauthorized to Put", zid);
            return Permission::Deny;
        }
        log::trace!("[ACCESS LOG]: {} is authorized access to Put", zid);
        Permission::Allow
    }

    fn get(&self, key_expr: &str) -> Permission {
        let policy_enforcer = self.policy_enforcer();
        let interface_list = self.interface_list();
        let zid = self.zid();
        let mut decision = policy_enforcer.default_permission.clone();
        for subject in &interface_list {
            match policy_enforcer.policy_decision_point(
                *subject,
                self.flow(),
                Action::Get,
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
                    return Permission::Deny;
                }
            }
        }

        if decision == Permission::Deny {
            log::debug!("[ACCESS LOG]: {} is unauthorized to Query/Get", zid);
            return Permission::Deny;
        }
        log::trace!("[ACCESS LOG]: {} is authorized access to Query/Get", zid);
        Permission::Allow
    }
    fn declare_subscriber(&self, key_expr: &str) -> Permission {
        let policy_enforcer = self.policy_enforcer();
        let interface_list = self.interface_list();
        let zid = self.zid();
        let mut decision = policy_enforcer.default_permission.clone();
        for subject in &interface_list {
            match policy_enforcer.policy_decision_point(
                *subject,
                self.flow(),
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
                    return Permission::Deny;
                }
            }
        }

        if decision == Permission::Deny {
            log::debug!("[ACCESS LOG]: {} is unauthorized to be a Subscriber", zid);
            return Permission::Deny;
        }
        log::trace!(
            "[ACCESS LOG]: {} is authorized access to be a Subscriber",
            zid
        );
        Permission::Allow
    }

    fn declare_queryable(&self, key_expr: &str) -> Permission {
        let policy_enforcer = self.policy_enforcer();
        let interface_list = self.interface_list();
        let zid = self.zid();
        let mut decision = policy_enforcer.default_permission.clone();
        for subject in &interface_list {
            match policy_enforcer.policy_decision_point(
                *subject,
                self.flow(),
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
                    return Permission::Deny;
                }
            }
        }

        if decision == Permission::Deny {
            log::debug!("[ACCESS LOG]: {} is unauthorized to be a Queryable", zid);
            return Permission::Deny;
        }
        log::trace!(
            "[ACCESS LOG]: {} is authorized access to be a Queryable",
            zid
        );
        Permission::Allow
    }
}

impl AclActionMethods for EgressAclEnforcer {
    fn policy_enforcer(&self) -> Arc<PolicyEnforcer> {
        self.policy_enforcer.clone()
    }

    fn interface_list(&self) -> Vec<i32> {
        self.interface_list.clone()
    }

    fn zid(&self) -> ZenohId {
        self.zid
    }
    fn flow(&self) -> Flow {
        Flow::Egress
    }
}

impl AclActionMethods for IngressAclEnforcer {
    fn policy_enforcer(&self) -> Arc<PolicyEnforcer> {
        self.policy_enforcer.clone()
    }

    fn interface_list(&self) -> Vec<i32> {
        self.interface_list.clone()
    }

    fn zid(&self) -> ZenohId {
        self.zid
    }
    fn flow(&self) -> Flow {
        Flow::Ingress
    }
}
