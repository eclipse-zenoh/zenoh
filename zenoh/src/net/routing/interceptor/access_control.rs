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
//! [Click here for Zenoh's documentation](https://docs.rs/zenoh/latest/zenoh)

use std::{any::Any, collections::HashSet, iter, sync::Arc};

use itertools::Itertools;
use zenoh_config::{
    AclConfig, AclMessage, CertCommonName, InterceptorFlow, Interface, Permission, Username,
    ZenohId,
};
use zenoh_keyexpr::keyexpr;
use zenoh_link::LinkAuthId;
use zenoh_protocol::{
    core::ZenohIdProto,
    network::{
        interest::InterestMode, Declare, DeclareBody, Interest, NetworkBodyMut, NetworkMessageMut,
        Push, Request, Response,
    },
    zenoh::{PushBody, RequestBody},
};
use zenoh_result::ZResult;
use zenoh_transport::{multicast::TransportMulticast, unicast::TransportUnicast};

use super::{
    authorization::PolicyEnforcer, EgressInterceptor, IngressInterceptor, InterceptorFactory,
    InterceptorFactoryTrait, InterceptorLinkWrapper, InterceptorTrait,
};
use crate::{
    key_expr::KeyExpr,
    net::routing::interceptor::{authorization::SubjectQuery, InterceptorContext},
};
pub struct AclEnforcer {
    enforcer: Arc<PolicyEnforcer>,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AuthSubject {
    id: usize,
    name: String,
}

struct EgressAclEnforcer {
    policy_enforcer: Arc<PolicyEnforcer>,
    subject: Vec<AuthSubject>,
    zid: ZenohIdProto,
}

impl EgressAclEnforcer {
    #[inline]
    fn cached_result_or_action(
        &self,
        cached_permission: Option<Permission>,
        action: AclMessage,
        log_msg: &str,
        key_expr: KeyExpr,
    ) -> Permission {
        match cached_permission {
            Some(p) => {
                match p {
                    Permission::Allow => tracing::trace!(
                        "Using cached result: {} is authorized to {} on {}",
                        self.zid(),
                        log_msg,
                        key_expr
                    ),
                    Permission::Deny => tracing::trace!(
                        "Using cached result: {} is unauthorized to {} on {}",
                        self.zid(),
                        log_msg,
                        key_expr
                    ),
                }
                p
            }
            None => self.action(action, log_msg, &key_expr),
        }
    }
}

struct IngressAclEnforcer {
    policy_enforcer: Arc<PolicyEnforcer>,
    subject: Vec<AuthSubject>,
    zid: ZenohIdProto,
}

impl IngressAclEnforcer {
    #[inline]
    fn cached_result_or_action(
        &self,
        cached_permission: Option<Permission>,
        action: AclMessage,
        log_msg: &str,
        key_expr: KeyExpr,
    ) -> Permission {
        match cached_permission {
            Some(p) => {
                match p {
                    Permission::Allow => tracing::trace!(
                        "Using cached result: {} is authorized to {} on {}",
                        self.zid(),
                        log_msg,
                        key_expr
                    ),
                    Permission::Deny => tracing::trace!(
                        "Using cached result: {} is unauthorized to {} on {}",
                        self.zid(),
                        log_msg,
                        key_expr
                    ),
                }
                p
            }
            None => self.action(action, log_msg, &key_expr),
        }
    }

    #[inline]
    fn cached_result_or_action_undecl(
        &self,
        cached_permission: Option<Permission>,
        action: AclMessage,
        log_msg: &str,
        key_expr: Option<KeyExpr>,
    ) -> Permission {
        match cached_permission {
            Some(p) => {
                match p {
                    Permission::Allow => tracing::trace!(
                        "Using cached result: {} is authorized to {} on {:?}",
                        self.zid(),
                        log_msg,
                        key_expr
                    ),
                    Permission::Deny => tracing::trace!(
                        "Using cached result: {} is unauthorized to {} on {:?}",
                        self.zid(),
                        log_msg,
                        key_expr
                    ),
                }
                p
            }
            None => {
                // Undeclarations in ingress are only filtered if the ext_wire_expr is set.
                // If it's not set, we let the undeclaration pass, it will be rejected by the routing logic
                // if its associated declaration was denied.
                match key_expr {
                    Some(ke) => self.action(action, log_msg, &ke),
                    None => Permission::Allow,
                }
            }
        }
    }
}

pub(crate) fn acl_interceptor_factories(
    acl_config: &AclConfig,
) -> ZResult<Vec<InterceptorFactory>> {
    let mut res: Vec<InterceptorFactory> = vec![];

    if acl_config.enabled {
        let mut policy_enforcer = PolicyEnforcer::new();
        match policy_enforcer.init(acl_config) {
            Ok(_) => {
                tracing::debug!("Access control is enabled");
                res.push(Box::new(AclEnforcer {
                    enforcer: Arc::new(policy_enforcer),
                }))
            }
            Err(e) => bail!("Access control not enabled due to: {}", e),
        }
    } else {
        tracing::debug!("Access control is disabled");
    }

    Ok(res)
}

impl InterceptorFactoryTrait for AclEnforcer {
    fn new_transport_unicast(
        &self,
        transport: &TransportUnicast,
    ) -> (Option<IngressInterceptor>, Option<EgressInterceptor>) {
        let auth_ids = match transport.get_auth_ids() {
            Ok(auth_ids) => auth_ids,
            Err(err) => {
                tracing::error!("Couldn't get Transport Auth IDs: {}", err);
                return (None, None);
            }
        };

        let mut cert_common_names = Vec::new();
        let mut link_protocols = Vec::new();
        let username = auth_ids.username().cloned().map(Username);
        let zid: ZenohId = (*auth_ids.zid()).into();

        for auth_id in auth_ids.link_auth_ids() {
            match auth_id {
                LinkAuthId::Tls(value) => {
                    cert_common_names.push(value.as_ref().map(|v| CertCommonName(v.clone())));
                }
                LinkAuthId::Quic(value) => {
                    cert_common_names.push(value.as_ref().map(|v| CertCommonName(v.clone())));
                }
                _ => {}
            }
            link_protocols.push(Some(InterceptorLinkWrapper::from(auth_id).0));
        }
        if cert_common_names.is_empty() {
            cert_common_names.push(None);
        }

        let links = match transport.get_links() {
            Ok(links) => links,
            Err(err) => {
                tracing::error!("Couldn't get Transport links: {}", err);
                return (None, None);
            }
        };
        let mut interfaces = links
            .into_iter()
            .flat_map(|link| {
                link.interfaces
                    .into_iter()
                    .map(|interface| Some(Interface(interface)))
            })
            .collect::<Vec<_>>();
        if interfaces.is_empty() {
            interfaces.push(None);
        } else if interfaces.len() > 1 {
            tracing::warn!("Transport returned multiple network interfaces, current ACL logic might incorrectly apply filters in this case!");
        }

        let mut auth_subjects = HashSet::new();

        for ((((username, interface), cert_common_name), link_protocol), zid) in
            iter::once(username)
                .cartesian_product(interfaces.into_iter())
                .cartesian_product(cert_common_names.into_iter())
                .cartesian_product(link_protocols.into_iter())
                .cartesian_product(iter::once(Some(zid)))
        {
            let query = SubjectQuery {
                interface,
                cert_common_name,
                username,
                link_protocol,
                zid,
            };

            for entry in self.enforcer.subject_store.query(&query) {
                auth_subjects.insert(AuthSubject {
                    id: entry.id,
                    name: format!("{query}"),
                });
            }
        }

        let zid = match transport.get_zid() {
            Ok(zid) => zid,
            Err(err) => {
                tracing::error!("Couldn't get Transport zid: {}", err);
                return (None, None);
            }
        };
        // FIXME: Investigate if `AuthSubject` can have duplicates above and try to avoid this conversion
        let auth_subjects = auth_subjects.into_iter().collect::<Vec<AuthSubject>>();
        if auth_subjects.is_empty() {
            tracing::info!(
                "{zid} did not match any configured ACL subject. Default permission `{:?}` will be applied on all messages",
                self.enforcer.default_permission
            );
        }
        let ingress_interceptor = Box::new(IngressAclEnforcer {
            policy_enforcer: self.enforcer.clone(),
            zid,
            subject: auth_subjects.clone(),
        });
        let egress_interceptor = Box::new(EgressAclEnforcer {
            policy_enforcer: self.enforcer.clone(),
            zid,
            subject: auth_subjects,
        });
        (
            self.enforcer
                .interface_enabled
                .ingress
                .then_some(ingress_interceptor),
            self.enforcer
                .interface_enabled
                .egress
                .then_some(egress_interceptor),
        )
    }

    fn new_transport_multicast(
        &self,
        _transport: &TransportMulticast,
    ) -> Option<EgressInterceptor> {
        tracing::debug!("Transport Multicast is disabled in interceptor");
        None
    }

    fn new_peer_multicast(&self, _transport: &TransportMulticast) -> Option<IngressInterceptor> {
        tracing::debug!("Peer Multicast is disabled in interceptor");
        None
    }
}

struct Cache {
    query: Permission,
    reply: Permission,
    put: Permission,
    delete: Permission,
    declare_subscriber: Permission,
    declare_queryable: Permission,
    declare_token: Permission,
    query_token: Permission,
    declare_liveliness_subscriber: Permission,
}

impl InterceptorTrait for IngressAclEnforcer {
    fn compute_keyexpr_cache(&self, key_expr: &keyexpr) -> Option<Box<dyn Any + Send + Sync>> {
        tracing::trace!("ACL (ingress): caching permissions for `{}` ...", key_expr);
        Some(Box::new(Cache {
            query: self.action(AclMessage::Query, "Query (ingress)", key_expr),
            reply: self.action(AclMessage::Reply, "Reply (ingress)", key_expr),
            put: self.action(AclMessage::Put, "Put (ingress)", key_expr),
            delete: self.action(AclMessage::Delete, "Delete (ingress)", key_expr),
            declare_subscriber: self.action(
                AclMessage::DeclareSubscriber,
                "Declare/Undeclare Subscriber (ingress)",
                key_expr,
            ),
            declare_queryable: self.action(
                AclMessage::DeclareQueryable,
                "Declare/Undeclare Queryable (ingress)",
                key_expr,
            ),
            declare_token: self.action(
                AclMessage::LivelinessToken,
                "Declare/Undeclare Liveliness Token (ingress)",
                key_expr,
            ),
            query_token: self.action(
                AclMessage::LivelinessQuery,
                "Liveliness Query (ingress)",
                key_expr,
            ),
            declare_liveliness_subscriber: self.action(
                AclMessage::DeclareLivelinessSubscriber,
                "Declare Liveliness Subscriber (ingress)",
                key_expr,
            ),
        }))
    }

    fn intercept<'a>(&self, msg: &mut NetworkMessageMut, ctx: &mut dyn InterceptorContext) -> bool {
        let cache = ctx
            .get_cache(msg)
            .and_then(|i| match i.downcast_ref::<Cache>() {
                Some(c) => Some(c),
                None => {
                    tracing::debug!("Cache content type is incorrect");
                    None
                }
            });

        match &msg.body {
            NetworkBodyMut::Request(Request {
                payload: RequestBody::Query(_),
                ..
            }) => {
                let Some(keyexpr) = ctx.full_keyexpr(msg) else {
                    return false;
                };
                if self.cached_result_or_action(
                    cache.map(|c| c.query),
                    AclMessage::Query,
                    "Query (ingress)",
                    keyexpr,
                ) == Permission::Deny
                {
                    return false;
                }
            }
            NetworkBodyMut::Response(Response { .. }) => {
                let Some(keyexpr) = ctx.full_keyexpr(msg) else {
                    return false;
                };
                if self.cached_result_or_action(
                    cache.map(|c| c.reply),
                    AclMessage::Reply,
                    "Reply (ingress)",
                    keyexpr,
                ) == Permission::Deny
                {
                    return false;
                }
            }
            NetworkBodyMut::Push(Push {
                payload: PushBody::Put(_),
                ..
            }) => {
                let Some(keyexpr) = ctx.full_keyexpr(msg) else {
                    return false;
                };
                if self.cached_result_or_action(
                    cache.map(|c| c.put),
                    AclMessage::Put,
                    "Put (ingress)",
                    keyexpr,
                ) == Permission::Deny
                {
                    return false;
                }
            }
            NetworkBodyMut::Push(Push {
                payload: PushBody::Del(_),
                ..
            }) => {
                let Some(keyexpr) = ctx.full_keyexpr(msg) else {
                    return false;
                };
                if self.cached_result_or_action(
                    cache.map(|c| c.delete),
                    AclMessage::Delete,
                    "Delete (ingress)",
                    keyexpr,
                ) == Permission::Deny
                {
                    return false;
                }
            }
            NetworkBodyMut::Declare(Declare {
                body: DeclareBody::DeclareSubscriber(_),
                ..
            }) => {
                let Some(keyexpr) = ctx.full_keyexpr(msg) else {
                    return false;
                };
                if self.cached_result_or_action(
                    cache.map(|c| c.declare_subscriber),
                    AclMessage::DeclareSubscriber,
                    "Declare Subscriber (ingress)",
                    keyexpr,
                ) == Permission::Deny
                {
                    return false;
                }
            }
            NetworkBodyMut::Declare(Declare {
                body: DeclareBody::UndeclareSubscriber(_),
                ..
            }) => {
                // Undeclaration filtering diverges between ingress and egress:
                // Undeclarations in ingress are only filtered if the ext_wire_expr is set.
                // If it's not set, we let the undeclaration pass, it will be rejected by the routing logic
                // if its associated declaration was denied.
                if self.cached_result_or_action_undecl(
                    cache.map(|c| c.declare_subscriber),
                    AclMessage::DeclareSubscriber,
                    "Undeclare Subscriber (ingress)",
                    ctx.full_keyexpr(msg),
                ) == Permission::Deny
                {
                    return false;
                }
            }
            NetworkBodyMut::Declare(Declare {
                body: DeclareBody::DeclareQueryable(_),
                ..
            }) => {
                let Some(keyexpr) = ctx.full_keyexpr(msg) else {
                    return false;
                };
                if self.cached_result_or_action(
                    cache.map(|c| c.declare_queryable),
                    AclMessage::DeclareQueryable,
                    "Declare Queryable (ingress)",
                    keyexpr,
                ) == Permission::Deny
                {
                    return false;
                }
            }
            NetworkBodyMut::Declare(Declare {
                body: DeclareBody::UndeclareQueryable(_),
                ..
            }) => {
                // Undeclaration filtering diverges between ingress and egress:
                // Undeclarations in ingress are only filtered if the ext_wire_expr is set.
                // If it's not set, we let the undeclaration pass, it will be rejected by the routing logic
                // if its associated declaration was denied.
                if self.cached_result_or_action_undecl(
                    cache.map(|c| c.declare_queryable),
                    AclMessage::DeclareQueryable,
                    "Undeclare Queryable (ingress)",
                    ctx.full_keyexpr(msg),
                ) == Permission::Deny
                {
                    return false;
                }
            }
            NetworkBodyMut::Declare(Declare {
                body: DeclareBody::DeclareToken(_),
                ..
            }) => {
                let Some(keyexpr) = ctx.full_keyexpr(msg) else {
                    return false;
                };
                if self.cached_result_or_action(
                    cache.map(|c| c.declare_token),
                    AclMessage::LivelinessToken,
                    "Liveliness Token (ingress)",
                    keyexpr,
                ) == Permission::Deny
                {
                    return false;
                }
            }

            NetworkBodyMut::Declare(Declare {
                body: DeclareBody::UndeclareToken(_),
                ..
            }) => {
                // Undeclaration filtering diverges between ingress and egress:
                // Undeclarations in ingress are only filtered if the ext_wire_expr is set.
                // If it's not set, we let the undeclaration pass, it will be rejected by the routing logic
                // if its associated declaration was denied.
                if self.cached_result_or_action_undecl(
                    cache.map(|c| c.declare_token),
                    AclMessage::LivelinessToken,
                    "Undeclare Liveliness Token (ingress)",
                    ctx.full_keyexpr(msg),
                ) == Permission::Deny
                {
                    return false;
                }
            }
            NetworkBodyMut::Interest(Interest {
                mode: InterestMode::Current,
                options,
                ..
            }) if options.tokens() => {
                let Some(keyexpr) = ctx.full_keyexpr(msg) else {
                    return false;
                };
                if self.cached_result_or_action(
                    cache.map(|c| c.query_token),
                    AclMessage::LivelinessQuery,
                    "Liveliness Query (ingress)",
                    keyexpr,
                ) == Permission::Deny
                {
                    return false;
                }
            }
            NetworkBodyMut::Interest(Interest {
                mode: InterestMode::Future | InterestMode::CurrentFuture,
                options,
                ..
            }) if options.tokens() => {
                let Some(keyexpr) = ctx.full_keyexpr(msg) else {
                    return false;
                };
                if self.cached_result_or_action(
                    cache.map(|c| c.declare_liveliness_subscriber),
                    AclMessage::DeclareLivelinessSubscriber,
                    "Declare Liveliness Subscriber (ingress)",
                    keyexpr,
                ) == Permission::Deny
                {
                    return false;
                }
            }
            NetworkBodyMut::Interest(Interest {
                mode: InterestMode::Final,
                ..
            }) => {
                // InterestMode::Final filtering diverges between ingress and egress:
                // InterestMode::Final ingress is always allowed, it will be rejected by routing logic if its associated Interest was denied
            }
            // Unfiltered Declare messages
            NetworkBodyMut::Declare(Declare {
                body: DeclareBody::DeclareKeyExpr(_),
                ..
            })
            | NetworkBodyMut::Declare(Declare {
                body: DeclareBody::DeclareFinal(_),
                ..
            }) => {}
            // Unfiltered Undeclare messages
            NetworkBodyMut::Declare(Declare {
                body: DeclareBody::UndeclareKeyExpr(_),
                ..
            }) => {}
            // Unfiltered remaining message types
            NetworkBodyMut::Interest(_)
            | NetworkBodyMut::OAM(_)
            | NetworkBodyMut::ResponseFinal(_) => {}
        }
        true
    }
}

impl InterceptorTrait for EgressAclEnforcer {
    fn compute_keyexpr_cache(&self, key_expr: &keyexpr) -> Option<Box<dyn Any + Send + Sync>> {
        tracing::trace!("ACL (egress): caching permissions for `{}` ...", key_expr);
        Some(Box::new(Cache {
            query: self.action(AclMessage::Query, "Query (egress)", key_expr),
            reply: self.action(AclMessage::Reply, "Reply (egress)", key_expr),
            put: self.action(AclMessage::Put, "Put (egress)", key_expr),
            delete: self.action(AclMessage::Delete, "Delete (egress)", key_expr),
            declare_subscriber: self.action(
                AclMessage::DeclareSubscriber,
                "Declare/Undeclare Subscriber (egress)",
                key_expr,
            ),
            declare_queryable: self.action(
                AclMessage::DeclareQueryable,
                "Declare/Undeclare Queryable (egress)",
                key_expr,
            ),
            declare_token: self.action(
                AclMessage::LivelinessToken,
                "Declare/Undeclare Liveliness Token (egress)",
                key_expr,
            ),
            query_token: self.action(
                AclMessage::LivelinessQuery,
                "Liveliness Query (egress)",
                key_expr,
            ),
            declare_liveliness_subscriber: self.action(
                AclMessage::DeclareLivelinessSubscriber,
                "Declare Liveliness Subscriber (egress)",
                key_expr,
            ),
        }))
    }

    fn intercept(
        // String, Arc<str>, Box<str>, etc... & -> &str
        &self,
        msg: &mut NetworkMessageMut,
        ctx: &mut dyn InterceptorContext,
    ) -> bool {
        let cache = ctx
            .get_cache(msg)
            .and_then(|i| match i.downcast_ref::<Cache>() {
                Some(c) => Some(c),
                None => {
                    tracing::debug!("Cache content type is incorrect");
                    None
                }
            });

        match &msg.body {
            NetworkBodyMut::Request(Request {
                payload: RequestBody::Query(_),
                ..
            }) => {
                let Some(keyexpr) = ctx.full_keyexpr(msg) else {
                    return false;
                };
                if self.cached_result_or_action(
                    cache.map(|c| c.query),
                    AclMessage::Query,
                    "Query (egress)",
                    keyexpr,
                ) == Permission::Deny
                {
                    return false;
                }
            }
            NetworkBodyMut::Response(Response { .. }) => {
                let Some(keyexpr) = ctx.full_keyexpr(msg) else {
                    return false;
                };
                if self.cached_result_or_action(
                    cache.map(|c| c.reply),
                    AclMessage::Reply,
                    "Reply (egress)",
                    keyexpr,
                ) == Permission::Deny
                {
                    return false;
                }
            }
            NetworkBodyMut::Push(Push {
                payload: PushBody::Put(_),
                ..
            }) => {
                let Some(keyexpr) = ctx.full_keyexpr(msg) else {
                    return false;
                };
                if self.cached_result_or_action(
                    cache.map(|c| c.put),
                    AclMessage::Put,
                    "Put (egress)",
                    keyexpr,
                ) == Permission::Deny
                {
                    return false;
                }
            }
            NetworkBodyMut::Push(Push {
                payload: PushBody::Del(_),
                ..
            }) => {
                let Some(keyexpr) = ctx.full_keyexpr(msg) else {
                    return false;
                };
                if self.cached_result_or_action(
                    cache.map(|c| c.put),
                    AclMessage::Delete,
                    "Delete (egress)",
                    keyexpr,
                ) == Permission::Deny
                {
                    return false;
                }
            }
            NetworkBodyMut::Declare(Declare {
                body: DeclareBody::DeclareSubscriber(_),
                ..
            }) => {
                let Some(keyexpr) = ctx.full_keyexpr(msg) else {
                    return false;
                };
                if self.cached_result_or_action(
                    cache.map(|c| c.declare_subscriber),
                    AclMessage::DeclareSubscriber,
                    "Declare Subscriber (egress)",
                    keyexpr,
                ) == Permission::Deny
                {
                    return false;
                }
            }
            NetworkBodyMut::Declare(Declare {
                body: DeclareBody::UndeclareSubscriber(_),
                ..
            }) => {
                let Some(keyexpr) = ctx.full_keyexpr(msg) else {
                    return false;
                };
                // Undeclaration filtering diverges between ingress and egress:
                // in egress the keyexpr has to be provided in the RoutingContext
                if self.cached_result_or_action(
                    cache.map(|c| c.declare_subscriber),
                    AclMessage::DeclareSubscriber,
                    "Undeclare Subscriber (egress)",
                    keyexpr,
                ) == Permission::Deny
                {
                    return false;
                }
            }
            NetworkBodyMut::Declare(Declare {
                body: DeclareBody::DeclareQueryable(_),
                ..
            }) => {
                let Some(keyexpr) = ctx.full_keyexpr(msg) else {
                    return false;
                };
                if self.cached_result_or_action(
                    cache.map(|c| c.declare_queryable),
                    AclMessage::DeclareQueryable,
                    "Declare Queryable (egress)",
                    keyexpr,
                ) == Permission::Deny
                {
                    return false;
                }
            }
            NetworkBodyMut::Declare(Declare {
                body: DeclareBody::UndeclareQueryable(_),
                ..
            }) => {
                let Some(keyexpr) = ctx.full_keyexpr(msg) else {
                    return false;
                };
                // Undeclaration filtering diverges between ingress and egress:
                // in egress the keyexpr has to be provided in the RoutingContext
                if self.cached_result_or_action(
                    cache.map(|c| c.declare_queryable),
                    AclMessage::DeclareQueryable,
                    "Undeclare Queryable (egress)",
                    keyexpr,
                ) == Permission::Deny
                {
                    return false;
                }
            }
            NetworkBodyMut::Declare(Declare {
                body: DeclareBody::DeclareToken(_),
                ..
            }) => {
                let Some(keyexpr) = ctx.full_keyexpr(msg) else {
                    return false;
                };
                if self.cached_result_or_action(
                    cache.map(|c| c.declare_token),
                    AclMessage::LivelinessToken,
                    "Liveliness Token (egress)",
                    keyexpr,
                ) == Permission::Deny
                {
                    return false;
                }
            }
            NetworkBodyMut::Declare(Declare {
                body: DeclareBody::UndeclareToken(_),
                ..
            }) => {
                let Some(keyexpr) = ctx.full_keyexpr(msg) else {
                    return false;
                };
                // Undeclaration filtering diverges between ingress and egress:
                // in egress the keyexpr has to be provided in the RoutingContext
                if self.cached_result_or_action(
                    cache.map(|c| c.declare_token),
                    AclMessage::LivelinessToken,
                    "Undeclare Liveliness Token (egress)",
                    keyexpr,
                ) == Permission::Deny
                {
                    return false;
                }
            }
            NetworkBodyMut::Interest(Interest {
                mode: InterestMode::Current,
                options,
                ..
            }) if options.tokens() => {
                let Some(keyexpr) = ctx.full_keyexpr(msg) else {
                    return false;
                };
                if self.cached_result_or_action(
                    cache.map(|c| c.query_token),
                    AclMessage::LivelinessQuery,
                    "Liveliness Query (egress)",
                    keyexpr,
                ) == Permission::Deny
                {
                    return false;
                }
            }
            NetworkBodyMut::Interest(Interest {
                mode: InterestMode::Future | InterestMode::CurrentFuture,
                options,
                ..
            }) if options.tokens() => {
                let Some(keyexpr) = ctx.full_keyexpr(msg) else {
                    return false;
                };
                if self.cached_result_or_action(
                    cache.map(|c| c.declare_liveliness_subscriber),
                    AclMessage::DeclareLivelinessSubscriber,
                    "Declare Liveliness Subscriber (egress)",
                    keyexpr,
                ) == Permission::Deny
                {
                    return false;
                }
            }
            NetworkBodyMut::Interest(Interest {
                mode: InterestMode::Final,
                options,
                ..
            }) if options.tokens() => {
                let Some(keyexpr) = ctx.full_keyexpr(msg) else {
                    return false;
                };
                // Note: options are set for InterestMode::Final for internal use only by egress interceptors.

                // InterestMode::Final filtering diverges between ingress and egress:
                // in egress the keyexpr has to be provided in the RoutingContext
                if self.cached_result_or_action(
                    cache.map(|c| c.declare_liveliness_subscriber),
                    AclMessage::DeclareLivelinessSubscriber,
                    "Undeclare Liveliness Subscriber (egress)",
                    keyexpr,
                ) == Permission::Deny
                {
                    return false;
                }
            }
            // Unfiltered Declare messages
            NetworkBodyMut::Declare(Declare {
                body: DeclareBody::DeclareKeyExpr(_),
                ..
            })
            | NetworkBodyMut::Declare(Declare {
                body: DeclareBody::DeclareFinal(_),
                ..
            }) => {}
            // Unfiltered Undeclare messages
            NetworkBodyMut::Declare(Declare {
                body: DeclareBody::UndeclareKeyExpr(_),
                ..
            }) => {}
            // Unfiltered remaining message types
            NetworkBodyMut::Interest(_)
            | NetworkBodyMut::OAM(_)
            | NetworkBodyMut::ResponseFinal(_) => {}
        }
        true
    }
}

pub trait AclActionMethods {
    fn policy_enforcer(&self) -> &PolicyEnforcer;
    fn zid(&self) -> &ZenohIdProto;
    fn flow(&self) -> InterceptorFlow;
    fn authn_ids(&self) -> &Vec<AuthSubject>;
    fn action(&self, action: AclMessage, log_msg: &str, key_expr: &keyexpr) -> Permission {
        let policy_enforcer = self.policy_enforcer();
        let authn_ids = self.authn_ids();
        let zid = self.zid();
        let mut decision = policy_enforcer.default_permission;
        for subject in authn_ids {
            match policy_enforcer.policy_decision_point(subject.id, self.flow(), action, key_expr) {
                Ok(Permission::Allow) => {
                    tracing::trace!(
                        "{} on {} is authorized to {} on {}",
                        zid,
                        subject.name,
                        log_msg,
                        key_expr
                    );
                    decision = Permission::Allow;
                    break;
                }
                Ok(Permission::Deny) => {
                    tracing::trace!(
                        "{} on {} is unauthorized to {} on {}",
                        zid,
                        subject.name,
                        log_msg,
                        key_expr
                    );

                    decision = Permission::Deny;
                    continue;
                }
                Err(e) => {
                    tracing::debug!(
                        "{} on {} has an authorization error to {} on {}: {}",
                        zid,
                        subject.name,
                        log_msg,
                        key_expr,
                        e
                    );
                    return Permission::Deny;
                }
            }
        }
        decision
    }
}

impl AclActionMethods for EgressAclEnforcer {
    fn policy_enforcer(&self) -> &PolicyEnforcer {
        &self.policy_enforcer
    }

    fn zid(&self) -> &ZenohIdProto {
        &self.zid
    }

    fn flow(&self) -> InterceptorFlow {
        InterceptorFlow::Egress
    }

    fn authn_ids(&self) -> &Vec<AuthSubject> {
        &self.subject
    }
}

impl AclActionMethods for IngressAclEnforcer {
    fn policy_enforcer(&self) -> &PolicyEnforcer {
        &self.policy_enforcer
    }

    fn zid(&self) -> &ZenohIdProto {
        &self.zid
    }

    fn flow(&self) -> InterceptorFlow {
        InterceptorFlow::Ingress
    }

    fn authn_ids(&self) -> &Vec<AuthSubject> {
        &self.subject
    }
}
