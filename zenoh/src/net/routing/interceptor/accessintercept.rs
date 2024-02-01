use std::sync::Arc;

use zenoh_config::ZenohId;
use zenoh_protocol::{
    network::{NetworkBody, NetworkMessage, Push},
    zenoh::PushBody,
};
use zenoh_transport::{multicast::TransportMulticast, unicast::TransportUnicast};

use crate::net::routing::{interceptor::authz::PolicyEnforcer, RoutingContext};

use super::{
    authz::{self, NewCtx},
    EgressInterceptor, IngressInterceptor, InterceptorFactoryTrait, InterceptorTrait,
};
use authz::{Action, Request, Subject};

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
                zid: Some(uid),
            })),
            Some(Box::new(EgressAclEnforcer {
                zid: Some(uid),
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
            zid: None,
        }))
    }

    fn new_peer_multicast(&self, _transport: &TransportMulticast) -> Option<IngressInterceptor> {
        let e = &self.e;
        Some(Box::new(IngressAclEnforcer {
            e: e.clone(),
            zid: None,
        }))
    }
}

struct IngressAclEnforcer {
    e: Arc<PolicyEnforcer>,
    zid: Option<ZenohId>,
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
            let ke: &str = ctx.full_expr().unwrap();
            let new_ctx = NewCtx { ke, zid: self.zid }; //how to get the zid here
            let decision = e.policy_enforcement_point(new_ctx, Action::Write).unwrap();

            // let sub = Subject {
            //     id: self.zid.unwrap(),
            //     attributes: None,
            // };
            // let request = Request {
            //     sub,
            //     obj: ke.to_owned(),
            //     action: Action::Write,
            // };
            // let decision = e.policy_decision_point(request).unwrap();

            if !decision {
                return None;
            }
        }

        Some(ctx)
    }
}

struct EgressAclEnforcer {
    e: Arc<PolicyEnforcer>,
    zid: Option<ZenohId>,
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
            let ke: &str = ctx.full_expr().unwrap();
            let new_ctx = NewCtx { ke, zid: self.zid };
            let decision = e.policy_enforcement_point(new_ctx, Action::Read).unwrap();

            // let sub = Subject {
            //     id: self.zid.unwrap(),
            //     attributes: None,
            // };
            // let request = Request {
            //     sub,
            //     obj: ke.to_owned(),
            //     action: Action::Read,
            // };
            // let decision = e.policy_decision_point(request).unwrap();

            if !decision {
                return None;
            }
        }

        Some(ctx)
    }
}
