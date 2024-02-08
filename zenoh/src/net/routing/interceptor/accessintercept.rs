use std::sync::Arc;

use zenoh_config::ZenohId;
use zenoh_protocol::{
    network::{NetworkBody, NetworkMessage, Push},
    zenoh::PushBody,
};
use zenoh_transport::{multicast::TransportMulticast, unicast::TransportUnicast};

use crate::net::routing::RoutingContext;

use super::{
    authz::{ActionFlag, NewCtx, NewPolicyEnforcer},
    EgressInterceptor, IngressInterceptor, InterceptorFactoryTrait, InterceptorTrait,
};
pub(crate) struct AclEnforcer {
    pub(crate) e: Arc<NewPolicyEnforcer>,
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
    e: Arc<NewPolicyEnforcer>,
    zid: ZenohId,
}

impl InterceptorTrait for IngressAclEnforcer {
    fn intercept<'a>(
        &self,
        ctx: RoutingContext<NetworkMessage>,
    ) -> Option<RoutingContext<NetworkMessage>> {
        //intercept msg and send it to PEP
        if let NetworkBody::Push(Push {
            payload: PushBody::Put(_),
            ..
        }) = &ctx.msg.body
        {
            let e = &self.e;

            let ke: String = ctx.full_expr().unwrap().to_owned();
            // let ke: String = "test/thr".to_owned(); //for testing
            let new_ctx = NewCtx {
                ke: &ke,
                zid: self.zid,
                attributes: None,
            };
            match e.policy_enforcement_point(new_ctx, ActionFlag::Write) {
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
    e: Arc<NewPolicyEnforcer>,
    zid: ZenohId,
}

impl InterceptorTrait for EgressAclEnforcer {
    fn intercept(
        &self,
        ctx: RoutingContext<NetworkMessage>,
    ) -> Option<RoutingContext<NetworkMessage>> {
        //  intercept msg and send it to PEP
        if let NetworkBody::Push(Push {
            payload: PushBody::Put(_),
            ..
        }) = &ctx.msg.body
        {
            let e = &self.e;
            let ke: String = ctx.full_expr().unwrap().to_owned();

            // let ke: String = "test/thr".to_owned(); //for testing
            let new_ctx = NewCtx {
                ke: &ke,
                zid: self.zid,
                attributes: None,
            };
            match e.policy_enforcement_point(new_ctx, ActionFlag::Read) {
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
