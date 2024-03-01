use std::sync::Arc;
use zenoh_config::{AclConfig, Action, Permission, Subject};
use zenoh_protocol::{
    network::{NetworkBody, NetworkMessage, Push, Request, Response},
    zenoh::{PushBody, RequestBody, ResponseBody},
};
use zenoh_result::ZResult;
use zenoh_transport::{multicast::TransportMulticast, unicast::TransportUnicast};

use crate::net::routing::RoutingContext;

use super::{
    authz::PolicyEnforcer, EgressInterceptor, IngressInterceptor, InterceptorFactory,
    InterceptorFactoryTrait, InterceptorTrait,
};
pub(crate) struct AclEnforcer {
    pub(crate) e: Arc<PolicyEnforcer>,
}

struct EgressAclEnforcer {
    pe: Arc<PolicyEnforcer>,
    interface_list: Vec<i32>,
    default_decision: bool,
}
struct IngressAclEnforcer {
    pe: Arc<PolicyEnforcer>,
    interface_list: Vec<i32>,
    default_decision: bool,
}

pub(crate) fn acl_interceptor_factories(acl_config: AclConfig) -> ZResult<Vec<InterceptorFactory>> {
    let mut res: Vec<InterceptorFactory> = vec![];

    if acl_config.enabled {
        let mut policy_enforcer = PolicyEnforcer::new();
        match policy_enforcer.init(acl_config) {
            Ok(_) => {
                log::debug!("Access control is enabled and initialized");
                res.push(Box::new(AclEnforcer {
                    e: Arc::new(policy_enforcer),
                }))
            }
            Err(e) => log::error!(
                "Access control enabled but not initialized with error {}!",
                e
            ),
        }
    } else {
        log::warn!("Access Control is disabled in config!");
    }

    Ok(res)
}

impl InterceptorFactoryTrait for AclEnforcer {
    fn new_transport_unicast(
        &self,
        transport: &TransportUnicast,
    ) -> (Option<IngressInterceptor>, Option<EgressInterceptor>) {
        let mut interface_list: Vec<i32> = Vec::new();
        if let Ok(links) = transport.get_links() {
            log::debug!("acl interceptor links details {:?}", links);
            for link in links {
                let e = self.e.clone();
                if let Some(subject_map) = &e.subject_map {
                    for face in link.interfaces {
                        let subject = &Subject::Interface(face);
                        match subject_map.get(subject) {
                            Some(val) => interface_list.push(*val),
                            None => continue,
                        }
                    }
                }
            }
        }
        let pe = self.e.clone();
        (
            Some(Box::new(IngressAclEnforcer {
                pe: pe.clone(),
                interface_list: interface_list.clone(),
                default_decision: match pe.default_permission {
                    Permission::Allow => true,
                    Permission::Deny => false,
                },
            })),
            Some(Box::new(EgressAclEnforcer {
                pe: pe.clone(),
                interface_list,
                default_decision: match pe.default_permission {
                    Permission::Allow => true,
                    Permission::Deny => false,
                },
            })),
        )
    }

    fn new_transport_multicast(
        &self,
        _transport: &TransportMulticast,
    ) -> Option<EgressInterceptor> {
        None
    }

    fn new_peer_multicast(&self, _transport: &TransportMulticast) -> Option<IngressInterceptor> {
        None
    }
}

impl InterceptorTrait for IngressAclEnforcer {
    fn intercept<'a>(
        &self,
        ctx: RoutingContext<NetworkMessage>,
    ) -> Option<RoutingContext<NetworkMessage>> {
        let key_expr = ctx.full_expr()?; //TODO add caching
        if let NetworkBody::Push(Push {
            payload: PushBody::Put(_),
            ..
        })
        | NetworkBody::Request(Request {
            payload: RequestBody::Put(_),
            ..
        })
        | NetworkBody::Response(Response {
            payload: ResponseBody::Put(_),
            ..
        }) = &ctx.msg.body
        {
            let mut decision = self.default_decision;

            for subject in &self.interface_list {
                match self.pe.policy_decision_point(
                    *subject,
                    Action::Put,
                    key_expr,
                    self.default_decision,
                ) {
                    Ok(true) => {
                        decision = true;
                        break;
                    }
                    Ok(false) => continue,
                    Err(_) => return None,
                }
            }

            if !decision {
                return None;
            }
        } else if let NetworkBody::Request(Request {
            payload: RequestBody::Query(_),
            ..
        }) = &ctx.msg.body
        {
            let mut decision = self.default_decision;
            for subject in &self.interface_list {
                match self.pe.policy_decision_point(
                    *subject,
                    Action::Get,
                    key_expr,
                    self.default_decision,
                ) {
                    Ok(true) => {
                        decision = true;
                        break;
                    }
                    Ok(false) => continue,
                    Err(_) => return None,
                }
            }
            if !decision {
                return None;
            }
        }
        Some(ctx)
    }
}

impl InterceptorTrait for EgressAclEnforcer {
    fn intercept(
        &self,
        ctx: RoutingContext<NetworkMessage>,
    ) -> Option<RoutingContext<NetworkMessage>> {
        let key_expr = ctx.full_expr()?; //TODO add caching
        if let NetworkBody::Push(Push {
            payload: PushBody::Put(_),
            ..
        }) = &ctx.msg.body
        {
            let mut decision = self.default_decision;
            for subject in &self.interface_list {
                match self.pe.policy_decision_point(
                    *subject,
                    Action::Sub,
                    key_expr,
                    self.default_decision,
                ) {
                    Ok(true) => {
                        decision = true;
                        break;
                    }
                    Ok(false) => continue,
                    Err(_) => return None,
                }
            }
            if !decision {
                return None;
            }
        }
        Some(ctx)
    }
}
