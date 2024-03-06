use crate::KeyExpr;
use std::any::Any;
use std::sync::Arc;
use zenoh_config::{AclConfig, Action, Permission, Subject, ZenohId};
use zenoh_protocol::{
    network::{NetworkBody, NetworkMessage, Push, Request, Response},
    zenoh::{PushBody, RequestBody, ResponseBody},
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

/* effective blocking of multicast bypass */
// struct DropAllMsg;

// impl InterceptorTrait for DropAllMsg {
//     fn intercept(
//         &self,
//         ctx: RoutingContext<NetworkMessage>,
//     ) -> Option<RoutingContext<NetworkMessage>> {
//         None
//     }
// }

struct EgressAclEnforcer {
    policy_enforcer: Arc<PolicyEnforcer>,
    interface_list: Vec<i32>,
    default_decision: bool,
    zid: ZenohId,
}
struct IngressAclEnforcer {
    policy_enforcer: Arc<PolicyEnforcer>,
    interface_list: Vec<i32>,
    default_decision: bool,
    zid: ZenohId,
}

pub(crate) fn acl_interceptor_factories(acl_config: AclConfig) -> ZResult<Vec<InterceptorFactory>> {
    let mut res: Vec<InterceptorFactory> = vec![];

    if acl_config.enabled {
        let mut policy_enforcer = PolicyEnforcer::new();
        match policy_enforcer.init(acl_config) {
            Ok(_) => {
                log::info!("Access control is enabled and initialized");
                res.push(Box::new(AclEnforcer {
                    enforcer: Arc::new(policy_enforcer),
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
        let mut zid: ZenohId = ZenohId::default();
        if let Ok(id) = transport.get_zid() {
            zid = id;
        } else {
            log::error!("Error in trying to get zid");
        }
        let mut interface_list: Vec<i32> = Vec::new();
        if let Ok(links) = transport.get_links() {
            for link in links {
                let enforcer = self.enforcer.clone();
                if let Some(subject_map) = &enforcer.subject_map {
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
        let policy_enforcer = self.enforcer.clone();
        (
            Some(Box::new(IngressAclEnforcer {
                policy_enforcer: policy_enforcer.clone(),
                interface_list: interface_list.clone(),
                default_decision: match policy_enforcer.default_permission {
                    Permission::Allow => true,
                    Permission::Deny => false,
                },
                zid,
            })),
            Some(Box::new(EgressAclEnforcer {
                policy_enforcer: policy_enforcer.clone(),
                interface_list,
                default_decision: match policy_enforcer.default_permission {
                    Permission::Allow => true,
                    Permission::Deny => false,
                },
                zid,
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
    fn compute_keyexpr_cache(&self, key_expr: &KeyExpr<'_>) -> Option<Box<dyn Any + Send + Sync>> {
        // let ke_id = zlock!(self.ke_id);
        // if let Some(id) = ke_id.weight_at(&key_expr.clone()) {
        //     Some(Box::new(Some(*id)))
        // } else {
        //     Some(Box::new(None::<usize>))
        // }
        None
    }
    fn intercept<'a>(
        &self,
        ctx: RoutingContext<NetworkMessage>,
        cache: Option<&Box<dyn Any + Send + Sync>>,
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
                match self.policy_enforcer.policy_decision_point(
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
                    Err(e) => {
                        log::error!("Authorization incomplete due to error {}", e);
                        return None;
                    }
                }
            }

            if !decision {
                log::warn!("{} is unauthorized to Put", self.zid);
                return None;
            }
            log::info!("{} is authorized access to Put", self.zid);
        } else if let NetworkBody::Request(Request {
            payload: RequestBody::Query(_),
            ..
        }) = &ctx.msg.body
        {
            let mut decision = self.default_decision;
            for subject in &self.interface_list {
                match self.policy_enforcer.policy_decision_point(
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
                    Err(e) => {
                        log::error!("Authorization incomplete due to error {}", e);
                        return None;
                    }
                }
            }
            if !decision {
                log::warn!("{} is unauthorized to Query/Get", self.zid);
                return None;
            }

            log::info!("{} is authorized access to Query", self.zid);
        }
        Some(ctx)
    }
}

impl InterceptorTrait for EgressAclEnforcer {
    fn compute_keyexpr_cache(&self, key_expr: &KeyExpr<'_>) -> Option<Box<dyn Any + Send + Sync>> {
        // let ke_id = zlock!(self.ke_id);
        // if let Some(id) = ke_id.weight_at(&key_expr.clone()) {
        //     Some(Box::new(Some(*id)))
        // } else {
        //     Some(Box::new(None::<usize>))
        // }
        None
    }
    fn intercept(
        &self,
        ctx: RoutingContext<NetworkMessage>,
        cache: Option<&Box<dyn Any + Send + Sync>>,
    ) -> Option<RoutingContext<NetworkMessage>> {
        let key_expr = ctx.full_expr()?; //TODO add caching
        if let NetworkBody::Push(Push {
            payload: PushBody::Put(_),
            ..
        }) = &ctx.msg.body
        {
            let mut decision = self.default_decision;
            for subject in &self.interface_list {
                match self.policy_enforcer.policy_decision_point(
                    *subject,
                    Action::DeclareSubscriber,
                    key_expr,
                    self.default_decision,
                ) {
                    Ok(true) => {
                        decision = true;
                        break;
                    }
                    Ok(false) => continue,
                    Err(e) => {
                        log::error!("Authorization incomplete due to error {}", e);
                        return None;
                    }
                }
            }
            if !decision {
                log::warn!("{} is unauthorized to be Subscriber", self.zid);
                return None;
            }

            log::info!("{} is authorized access to be Subscriber", self.zid);
        } else if let NetworkBody::Request(Request {
            payload: RequestBody::Query(_),
            ..
        }) = &ctx.msg.body
        {
            let mut decision = self.default_decision;
            for subject in &self.interface_list {
                match self.policy_enforcer.policy_decision_point(
                    *subject,
                    Action::DeclareQueryable,
                    key_expr,
                    self.default_decision,
                ) {
                    Ok(true) => {
                        decision = true;
                        break;
                    }
                    Ok(false) => continue,
                    Err(e) => {
                        log::error!("Authorization incomplete due to error {}", e);
                        return None;
                    }
                }
            }
            if !decision {
                log::warn!("{} is unauthorized to be Queryable", self.zid);
                return None;
            }

            log::info!("{} is authorized access to be Queryable", self.zid);
        }

        Some(ctx)
    }
}
