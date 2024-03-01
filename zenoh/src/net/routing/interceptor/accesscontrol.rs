use std::sync::Arc;
use zenoh_config::{AclConfig, Action, Subject};
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
}
struct IngressAclEnforcer {
    pe: Arc<PolicyEnforcer>,
    interface_list: Vec<i32>,
}

pub(crate) fn acl_interceptor_factories(acl_config: AclConfig) -> ZResult<Vec<InterceptorFactory>> {
    let mut res: Vec<InterceptorFactory> = vec![];
    let mut acl_enabled = false;
    match acl_config.enabled {
        Some(val) => acl_enabled = val,
        None => {
            log::warn!("acl config is not setup");
            //return Ok(res);
        }
    }
    if acl_enabled {
        let mut policy_enforcer = PolicyEnforcer::new();
        match policy_enforcer.init(acl_config) {
            Ok(_) => {
                log::debug!("access control is enabled and initialized");
                res.push(Box::new(AclEnforcer {
                    e: Arc::new(policy_enforcer),
                }))
            }
            Err(e) => log::error!(
                "access control enabled but not initialized with error {}!",
                e
            ),
        }
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
                if let Some(sm) = &e.subject_map {
                    for i in link.interfaces {
                        let x = &Subject::Interface(i);
                        if sm.contains_key(x) {
                            interface_list.push(*sm.get(x).unwrap());
                        }
                    }
                }
            }
        }
        log::debug!("log info");
        let pe = self.e.clone();
        (
            Some(Box::new(IngressAclEnforcer {
                pe: pe.clone(),
                interface_list: interface_list.clone(),
            })),
            Some(Box::new(EgressAclEnforcer {
                pe: pe.clone(),
                interface_list,
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
        let kexpr = match ctx.full_expr() {
            Some(val) => val,
            None => return None,
        }; //add the cache here
        let interface_list = &self.interface_list;

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
            // let action = Action::Put;
            let mut decision = false;
            for subject in interface_list {
                match self.pe.policy_decision_point(*subject, Action::Put, kexpr) {
                    Ok(val) => {
                        if val {
                            decision = val;
                            break;
                        }
                    }
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
        let kexpr = match ctx.full_expr() {
            Some(val) => val,
            None => return None,
        }; //add the cache here
        let interface_list = &self.interface_list;
        if let NetworkBody::Push(Push {
            payload: PushBody::Put(_),
            ..
        }) = &ctx.msg.body
        {
            // let action = ;
            let mut decision = false;
            for subject in interface_list {
                match self.pe.policy_decision_point(*subject, Action::Sub, kexpr) {
                    Ok(val) => {
                        if val {
                            decision = val;
                            break;
                        }
                    }
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
