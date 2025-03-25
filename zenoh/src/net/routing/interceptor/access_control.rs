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
};
use zenoh_protocol::{
    core::ZenohIdProto,
    network::{
        interest::InterestMode, Declare, DeclareBody, Interest, NetworkBodyMut, NetworkMessageMut,
        Push, Request, Response,
    },
    zenoh::{PushBody, RequestBody},
};
use zenoh_result::ZResult;
use zenoh_transport::{
    multicast::TransportMulticast,
    unicast::{authentication::AuthId, TransportUnicast},
};

use super::{
    authorization::PolicyEnforcer, EgressInterceptor, IngressInterceptor, InterceptorFactory,
    InterceptorFactoryTrait, InterceptorTrait,
};
use crate::{
    api::key_expr::KeyExpr,
    net::routing::{interceptor::authorization::SubjectQuery, RoutingContext},
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

struct IngressAclEnforcer {
    policy_enforcer: Arc<PolicyEnforcer>,
    subject: Vec<AuthSubject>,
    zid: ZenohIdProto,
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
        let mut username = None;

        for auth_id in auth_ids {
            match auth_id {
                AuthId::CertCommonName(value) => {
                    cert_common_names.push(Some(CertCommonName(value)));
                }
                AuthId::Username(value) => {
                    if username.is_some() {
                        tracing::error!("Transport should not report more than one username");
                        return (None, None);
                    }
                    username = Some(Username(value));
                }
                AuthId::None => {}
            }
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

        for ((username, interface), cert_common_name) in iter::once(username)
            .cartesian_product(interfaces.into_iter())
            .cartesian_product(cert_common_names.into_iter())
        {
            let query = SubjectQuery {
                interface,
                cert_common_name,
                username,
            };

            if let Some(entry) = self.enforcer.subject_store.query(&query) {
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

impl InterceptorTrait for IngressAclEnforcer {
    fn compute_keyexpr_cache(&self, key_expr: &KeyExpr<'_>) -> Option<Box<dyn Any + Send + Sync>> {
        Some(Box::new(key_expr.to_string()))
    }

    fn intercept<'a>(
        &self,
        ctx: &mut RoutingContext<NetworkMessageMut>,
        cache: Option<&Box<dyn Any + Send + Sync>>,
    ) -> bool {
        let key_expr = cache
            .and_then(|i| match i.downcast_ref::<String>() {
                Some(e) => Some(e.as_str()),
                None => {
                    tracing::debug!("Cache content was not of type String");
                    None
                }
            })
            .or_else(|| ctx.full_expr());

        match &ctx.msg.body {
            NetworkBodyMut::Request(Request {
                payload: RequestBody::Query(_),
                ..
            }) => {
                return key_expr.is_some_and(|ke| {
                    self.action(AclMessage::Query, "Query (ingress)", ke) == Permission::Allow
                });
            }
            NetworkBodyMut::Response(Response { .. }) => {
                return key_expr.is_some_and(|ke| {
                    self.action(AclMessage::Reply, "Reply (ingress)", ke) == Permission::Allow
                });
            }
            NetworkBodyMut::Push(Push {
                payload: PushBody::Put(_),
                ..
            }) => {
                return key_expr.is_some_and(|ke| {
                    self.action(AclMessage::Put, "Put (ingress)", ke) == Permission::Allow
                });
            }
            NetworkBodyMut::Push(Push {
                payload: PushBody::Del(_),
                ..
            }) => {
                return key_expr.is_some_and(|ke| {
                    self.action(AclMessage::Delete, "Delete (ingress)", ke) == Permission::Allow
                });
            }
            NetworkBodyMut::Declare(Declare {
                body: DeclareBody::DeclareSubscriber(_),
                ..
            }) => {
                return key_expr.is_some_and(|ke| {
                    self.action(
                        AclMessage::DeclareSubscriber,
                        "Declare Subscriber (ingress)",
                        ke,
                    ) == Permission::Allow
                });
            }
            NetworkBodyMut::Declare(Declare {
                body: DeclareBody::UndeclareSubscriber(_),
                ..
            }) => {
                // Undeclaration filtering diverges between ingress and egress:
                // Undeclarations in ingress are only filtered if the ext_wire_expr is set.
                // If it's not set, we let the undeclaration pass, it will be rejected by the routing logic
                // if its associated declaration was denied.
                return key_expr.is_some_and(|ke| {
                    ke.is_empty()
                        || self.action(
                            AclMessage::DeclareSubscriber,
                            "Undeclare Subscriber (ingress)",
                            ke,
                        ) == Permission::Allow
                });
            }
            NetworkBodyMut::Declare(Declare {
                body: DeclareBody::DeclareQueryable(_),
                ..
            }) => {
                return key_expr.is_some_and(|ke| {
                    self.action(
                        AclMessage::DeclareQueryable,
                        "Declare Queryable (ingress)",
                        ke,
                    ) == Permission::Allow
                });
            }
            NetworkBodyMut::Declare(Declare {
                body: DeclareBody::UndeclareQueryable(_),
                ..
            }) => {
                // Undeclaration filtering diverges between ingress and egress:
                // Undeclarations in ingress are only filtered if the ext_wire_expr is set.
                // If it's not set, we let the undeclaration pass, it will be rejected by the routing logic
                // if its associated declaration was denied.
                return key_expr.is_some_and(|ke| {
                    ke.is_empty()
                        || self.action(
                            AclMessage::DeclareQueryable,
                            "Undeclare Queryable (ingress)",
                            ke,
                        ) == Permission::Allow
                });
            }
            NetworkBodyMut::Declare(Declare {
                body: DeclareBody::DeclareToken(_),
                ..
            }) => {
                return key_expr.is_some_and(|ke| {
                    self.action(
                        AclMessage::LivelinessToken,
                        "Declare Liveliness Token (ingress)",
                        ke,
                    ) == Permission::Allow
                });
            }
            NetworkBodyMut::Declare(Declare {
                body: DeclareBody::UndeclareToken(_),
                ..
            }) => {
                return key_expr.is_some_and(|ke| {
                    ke.is_empty()
                        || self.action(
                            AclMessage::LivelinessToken,
                            "Undeclare Liveliness Token (ingress)",
                            ke,
                        ) == Permission::Allow
                });
            }
            NetworkBodyMut::Interest(Interest {
                mode: InterestMode::Current,
                options,
                ..
            }) if options.tokens() => {
                return key_expr.is_some_and(|ke| {
                    self.action(
                        AclMessage::LivelinessQuery,
                        "Liveliness Query (ingress)",
                        ke,
                    ) == Permission::Allow
                });
            }
            NetworkBodyMut::Interest(Interest {
                mode: InterestMode::Future | InterestMode::CurrentFuture,
                options,
                ..
            }) if options.tokens() => {
                return key_expr.is_some_and(|ke| {
                    self.action(
                        AclMessage::DeclareLivelinessSubscriber,
                        "Declare Liveliness Subscriber (ingress)",
                        ke,
                    ) == Permission::Allow
                });
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
    fn compute_keyexpr_cache(&self, key_expr: &KeyExpr<'_>) -> Option<Box<dyn Any + Send + Sync>> {
        Some(Box::new(key_expr.to_string()))
    }

    fn intercept(
        &self,
        ctx: &mut RoutingContext<NetworkMessageMut>,
        cache: Option<&Box<dyn Any + Send + Sync>>,
    ) -> bool {
        let key_expr = cache
            .and_then(|i| match i.downcast_ref::<String>() {
                Some(e) => Some(e.as_str()),
                None => {
                    tracing::debug!("Cache content was not of type String");
                    None
                }
            })
            .or_else(|| ctx.full_expr());

        match &ctx.msg.body {
            NetworkBodyMut::Request(Request {
                payload: RequestBody::Query(_),
                ..
            }) => {
                return key_expr.is_some_and(|ke| {
                    self.action(AclMessage::Query, "Query (egress)", ke) == Permission::Allow
                });
            }
            NetworkBodyMut::Response(Response { .. }) => {
                return key_expr.is_some_and(|ke| {
                    self.action(AclMessage::Reply, "Reply (egress)", ke) == Permission::Allow
                });
            }
            NetworkBodyMut::Push(Push {
                payload: PushBody::Put(_),
                ..
            }) => {
                return key_expr.is_some_and(|ke| {
                    self.action(AclMessage::Put, "Put (egress)", ke) == Permission::Allow
                });
            }
            NetworkBodyMut::Push(Push {
                payload: PushBody::Del(_),
                ..
            }) => {
                return key_expr.is_some_and(|ke| {
                    self.action(AclMessage::Delete, "Delete (egress)", ke) == Permission::Allow
                });
            }
            NetworkBodyMut::Declare(Declare {
                body: DeclareBody::DeclareSubscriber(_),
                ..
            }) => {
                return key_expr.is_some_and(|ke| {
                    self.action(
                        AclMessage::DeclareSubscriber,
                        "Declare Subscriber (egress)",
                        ke,
                    ) == Permission::Allow
                });
            }
            NetworkBodyMut::Declare(Declare {
                body: DeclareBody::UndeclareSubscriber(_),
                ..
            }) => {
                // Undeclaration filtering diverges between ingress and egress:
                // in egress the keyexpr has to be provided in the RoutingContext
                return key_expr.is_some_and(|ke| {
                    self.action(
                        AclMessage::DeclareSubscriber,
                        "Undeclare Subscriber (egress)",
                        ke,
                    ) == Permission::Allow
                });
            }
            NetworkBodyMut::Declare(Declare {
                body: DeclareBody::DeclareQueryable(_),
                ..
            }) => {
                return key_expr.is_some_and(|ke| {
                    self.action(
                        AclMessage::DeclareQueryable,
                        "Declare Queryable (egress)",
                        ke,
                    ) == Permission::Allow
                });
            }
            NetworkBodyMut::Declare(Declare {
                body: DeclareBody::UndeclareQueryable(_),
                ..
            }) => {
                // Undeclaration filtering diverges between ingress and egress:
                // in egress the keyexpr has to be provided in the RoutingContext
                return key_expr.is_some_and(|ke| {
                    self.action(
                        AclMessage::DeclareQueryable,
                        "Undeclare Queryable (egress)",
                        ke,
                    ) == Permission::Allow
                });
            }
            NetworkBodyMut::Declare(Declare {
                body: DeclareBody::DeclareToken(_),
                ..
            }) => {
                return key_expr.is_some_and(|ke| {
                    self.action(
                        AclMessage::LivelinessToken,
                        "Declare Liveliness Token (egress)",
                        ke,
                    ) == Permission::Allow
                });
            }
            NetworkBodyMut::Declare(Declare {
                body: DeclareBody::UndeclareToken(_),
                ..
            }) => {
                // Undeclaration filtering diverges between ingress and egress:
                // in egress the keyexpr has to be provided in the RoutingContext
                return key_expr.is_some_and(|ke| {
                    self.action(
                        AclMessage::LivelinessToken,
                        "Undeclare Liveliness Token (egress)",
                        ke,
                    ) == Permission::Allow
                });
            }
            NetworkBodyMut::Interest(Interest {
                mode: InterestMode::Current,
                options,
                ..
            }) if options.tokens() => {
                return key_expr.is_some_and(|ke| {
                    self.action(AclMessage::LivelinessQuery, "Liveliness Query (egress)", ke)
                        == Permission::Allow
                })
            }
            NetworkBodyMut::Interest(Interest {
                mode: InterestMode::Future | InterestMode::CurrentFuture,
                options,
                ..
            }) if options.tokens() => {
                return key_expr.is_some_and(|ke| {
                    self.action(
                        AclMessage::DeclareLivelinessSubscriber,
                        "Declare Liveliness Subscriber (egress)",
                        ke,
                    ) == Permission::Allow
                });
            }
            NetworkBodyMut::Interest(Interest {
                mode: InterestMode::Final,
                options,
                ..
            }) if options.tokens() => {
                // Note: options are set for InterestMode::Final for internal use only by egress interceptors.

                // InterestMode::Final filtering diverges between ingress and egress:
                // in egress the keyexpr has to be provided in the RoutingContext
                return key_expr.is_some_and(|ke| {
                    self.action(
                        AclMessage::DeclareLivelinessSubscriber,
                        "Declare Liveliness Subscriber (egress)",
                        ke,
                    ) == Permission::Allow
                });
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
    fn policy_enforcer(&self) -> Arc<PolicyEnforcer>;
    fn zid(&self) -> ZenohIdProto;
    fn flow(&self) -> InterceptorFlow;
    fn authn_ids(&self) -> Vec<AuthSubject>;
    fn action(&self, action: AclMessage, log_msg: &str, key_expr: &str) -> Permission {
        let policy_enforcer = self.policy_enforcer();
        let authn_ids: Vec<AuthSubject> = self.authn_ids();
        let zid = self.zid();
        let mut decision = policy_enforcer.default_permission;
        for subject in &authn_ids {
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
                    tracing::debug!(
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
    fn policy_enforcer(&self) -> Arc<PolicyEnforcer> {
        self.policy_enforcer.clone()
    }

    fn zid(&self) -> ZenohIdProto {
        self.zid
    }

    fn flow(&self) -> InterceptorFlow {
        InterceptorFlow::Egress
    }

    fn authn_ids(&self) -> Vec<AuthSubject> {
        self.subject.clone()
    }
}

impl AclActionMethods for IngressAclEnforcer {
    fn policy_enforcer(&self) -> Arc<PolicyEnforcer> {
        self.policy_enforcer.clone()
    }

    fn zid(&self) -> ZenohIdProto {
        self.zid
    }

    fn flow(&self) -> InterceptorFlow {
        InterceptorFlow::Ingress
    }

    fn authn_ids(&self) -> Vec<AuthSubject> {
        self.subject.clone()
    }
}
