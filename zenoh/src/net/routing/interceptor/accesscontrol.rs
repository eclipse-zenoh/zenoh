use std::sync::Arc;

use zenoh_config::ZenohId;
use zenoh_protocol::{
    network::{NetworkBody, NetworkMessage, Push},
    zenoh::PushBody,
};
use zenoh_transport::{multicast::TransportMulticast, unicast::TransportUnicast};

use crate::net::routing::RoutingContext;

use super::{
    authz::{Action, Attribute, PolicyEnforcer, RequestInfo},
    EgressInterceptor, IngressInterceptor, InterceptorFactoryTrait, InterceptorTrait,
};
pub(crate) struct AclEnforcer {
    pub(crate) e: Arc<PolicyEnforcer>,
}

impl InterceptorFactoryTrait for AclEnforcer {
    fn new_transport_unicast(
        &self,
        transport: &TransportUnicast,
    ) -> (Option<IngressInterceptor>, Option<EgressInterceptor>) {
        let uid = transport.get_zid().unwrap();
        (
            Some(Box::new(IngressAclEnforcer {
                e: self.e.clone(),
                zid: uid,
            })),
            Some(Box::new(EgressAclEnforcer {
                zid: uid,
                e: self.e.clone(),
            })),
        )
    }

    fn new_transport_multicast(
        &self,
        _transport: &TransportMulticast,
    ) -> Option<EgressInterceptor> {
        let e = &self.e;
        Some(Box::new(EgressAclEnforcer {
            e: e.clone(),
            zid: ZenohId::default(),
        }))
    }

    fn new_peer_multicast(&self, _transport: &TransportMulticast) -> Option<IngressInterceptor> {
        let e = &self.e;
        Some(Box::new(IngressAclEnforcer {
            e: e.clone(),
            zid: ZenohId::default(),
        }))
    }
}

struct IngressAclEnforcer {
    e: Arc<PolicyEnforcer>,
    zid: ZenohId,
}

impl InterceptorTrait for IngressAclEnforcer {
    fn intercept<'a>(
        &self,
        ctx: RoutingContext<NetworkMessage>,
    ) -> Option<RoutingContext<NetworkMessage>> {
        if let NetworkBody::Push(Push {
            payload: PushBody::Put(_),
            ..
        }) = &ctx.msg.body
        {
            let e = &self.e;
            let ke = ctx.full_expr().unwrap();
            let network_type = "wlan0";  //for testing
            let mut sub_info: Vec<Attribute> = Vec::new();
            let attribute_list = e.get_attribute_list().unwrap();
            for i in attribute_list {
                match i.as_str() {
                    "UserId" => sub_info.push(Attribute::UserId(self.zid)),
                    "NetworkType" => sub_info.push(Attribute::NetworkType(network_type.to_owned())),
                    _ => { //other metadata values
                    }
                }
            }
            let request_info = RequestInfo {
                sub: sub_info,
                ke: ke.to_string(),
                action: Action::Write,
            };
            match e.policy_enforcement_point(request_info) {
                Ok(decision) => {
                    if !decision {
                        return None;
                    }
                }
                Err(_) => return None,
            }
        }

        Some(ctx)
    }
}

struct EgressAclEnforcer {
    e: Arc<PolicyEnforcer>,
    zid: ZenohId,
}

impl InterceptorTrait for EgressAclEnforcer {
    fn intercept(
        &self,
        ctx: RoutingContext<NetworkMessage>,
    ) -> Option<RoutingContext<NetworkMessage>> {
        if let NetworkBody::Push(Push {
            payload: PushBody::Put(_),
            ..
        }) = &ctx.msg.body
        {
            let e = &self.e;
            let ke = ctx.full_expr().unwrap();
            let network_type = "wlan0"; //for testing
            let mut sub_info: Vec<Attribute> = Vec::new();
            let attribute_list = e.get_attribute_list().unwrap();
            for i in attribute_list {
                match i.as_str() {
                    "UserId" => sub_info.push(Attribute::UserId(self.zid)),
                    "NetworkType" => sub_info.push(Attribute::NetworkType(network_type.to_owned())),
                    _ => { //other metadata values,
                    }
                }
            }
            let request_info = RequestInfo {
                sub: sub_info,
                ke: ke.to_string(),
                action: Action::Read,
            };
            match e.policy_enforcement_point(request_info) {
                Ok(decision) => {
                    if !decision {
                        return None;
                    }
                }
                Err(_) => return None,
            }
        }

        Some(ctx)
    }
}
