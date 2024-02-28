use std::sync::Arc;

use zenoh_protocol::{
    network::{NetworkBody, NetworkMessage, Push},
    zenoh::PushBody,
};
use zenoh_transport::{multicast::TransportMulticast, unicast::TransportUnicast};

use crate::net::routing::RoutingContext;

use super::{
    authz::{Action, PolicyEnforcer, Subject},
    EgressInterceptor, IngressInterceptor, InterceptorFactoryTrait, InterceptorTrait,
};
pub(crate) struct AclEnforcer {
    pub(crate) e: Arc<PolicyEnforcer>,
    pub(crate) interfaces: Option<Vec<String>>, //to keep the interfaces
}
struct EgressAclEnforcer {
    pe: Arc<PolicyEnforcer>,
    subject: i32,
}
struct IngressAclEnforcer {
    pe: Arc<PolicyEnforcer>,
    subject: i32,
}

impl InterceptorFactoryTrait for AclEnforcer {
    fn new_transport_unicast(
        &self,
        transport: &TransportUnicast,
    ) -> (Option<IngressInterceptor>, Option<EgressInterceptor>) {
        //        let uid = transport.get_zid().unwrap();
        if let Some(interfaces) = &self.interfaces {
            log::debug!(
                "New downsampler transport unicast config interfaces: {:?}",
                interfaces
            );
            if let Ok(links) = transport.get_links() {
                for link in links {
                    log::debug!(
                        "New downsampler transport unicast link interfaces: {:?}",
                        link.interfaces
                    );
                    if !link.interfaces.iter().any(|x| interfaces.contains(x)) {
                        return (None, None);
                    }
                }
            }
        };
        let interface_key = Subject::NetworkInterface("wifi0".to_string());

        //get value from the subject_map
        let e = self.e.clone();
        let mut interface_value = 0;
        if let Some(sm) = &e.subject_map {
            interface_value = *sm.get(&interface_key).unwrap();
        }

        let pe = self.e.clone();
        (
            Some(Box::new(IngressAclEnforcer {
                pe: pe.clone(),
                subject: interface_value,
            })),
            Some(Box::new(EgressAclEnforcer {
                pe: pe.clone(),
                subject: interface_value,
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
        //intercept msg and send it to PEP
        if let NetworkBody::Push(Push {
            payload: PushBody::Put(_),
            ..
        }) = &ctx.msg.body
        {
            let kexpr = ctx.full_expr().unwrap(); //add the cache here

            let subject = self.subject;

            match self
                .pe
                .policy_decision_point(subject, Action::Pub, kexpr.to_string())
            {
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
            let kexpr = ctx.full_expr().unwrap(); //add the cache here

            let subject = self.subject;
            match self
                .pe
                .policy_decision_point(subject, Action::Sub, kexpr.to_string())
            {
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
