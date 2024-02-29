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
    // pub(crate) interfaces: Option<Vec<String>>, //to keep the interfaces
}

// pub(crate) fn acl_enforcer_init(
//     config: &Config,
// ) -> ZResult<Vec<InterceptorFactory>> {

//     let res :InterceptorFactory;

//     let acl_config = config.transport().acl().clone();

//     let mut acl_enabled = false;
//     match acl_config.enabled {
//         Some(val) => acl_enabled = val,
//         None => {
//             log::warn!("acl config not setup");
//         }
//     }
//     if acl_enabled {
//         let mut interface_value =
//        // let mut policy_enforcer = PolicyEnforcer::new();
//         match policy_enforcer.init(acl_config) {
//             Ok(_) => res.push(Box::new(AclEnforcer {
//                 e: Arc::new(policy_enforcer),
//                 interfaces: None,
//             })),
//             Err(e) => log::error!(
//                 "access control enabled but not initialized with error {}!",
//                 e
//             ),
//         }
//     }

//     let mut res: Vec<InterceptorFactory> = vec![];

//     for ds in config {
//         res.push(Box::new(DownsamplingInterceptorFactory::new(ds.clone())));
//     }

//     Ok(res)
// }

struct EgressAclEnforcer {
    pe: Arc<PolicyEnforcer>,
    interface_list: Vec<i32>,
}
struct IngressAclEnforcer {
    pe: Arc<PolicyEnforcer>,
    interface_list: Vec<i32>,
}

impl InterceptorFactoryTrait for AclEnforcer {
    fn new_transport_unicast(
        &self,
        transport: &TransportUnicast,
    ) -> (Option<IngressInterceptor>, Option<EgressInterceptor>) {
        let mut interface_list: Vec<i32> = Vec::new();
        if let Ok(links) = transport.get_links() {
            println!("links are {:?}", links);
            for link in links {
                let e = self.e.clone();
                if let Some(sm) = &e.subject_map {
                    for i in link.interfaces {
                        let x = &Subject::NetworkInterface(i.to_string());
                        if sm.contains_key(x) {
                            interface_list.push(*sm.get(x).unwrap());
                        }
                    }
                }
            }
        }

        // let interface_key = Subject::NetworkInterface("wifi0".to_string());

        // //get value from the subject_map
        // let e = self.e.clone();
        // let mut interface_value = 0;
        // if let Some(sm) = &e.subject_map {
        //     interface_value = *sm.get(&interface_key).unwrap();
        // }

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
        //intercept msg and send it to PEP
        if let NetworkBody::Push(Push {
            payload: PushBody::Put(_),
            ..
        }) = &ctx.msg.body
        {
            let kexpr = ctx.full_expr().unwrap(); //add the cache here

            let subject_list = &self.interface_list;
            let mut decision = false;
            for subject in subject_list {
                match self
                    .pe
                    .policy_decision_point(*subject, Action::Pub, kexpr.to_string())
                {
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
        //  intercept msg and send it to PEP
        if let NetworkBody::Push(Push {
            payload: PushBody::Put(_),
            ..
        }) = &ctx.msg.body
        {
            let kexpr = ctx.full_expr().unwrap(); //add the cache here

            let subject_list = &self.interface_list;
            let mut decision = false;
            for subject in subject_list {
                match self
                    .pe
                    .policy_decision_point(*subject, Action::Sub, kexpr.to_string())
                {
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
            // let kexpr = ctx.full_expr().unwrap(); //add the cache here

            // let subject = self.subject;
            // match self
            //     .pe
            //     .policy_decision_point(subject, Action::Sub, kexpr.to_string())
            // {
            //     Ok(decision) => {
            //         if !decision {
            //             return None;
            //         }
            //     }
            //     Err(_) => return None,
            // }
        }

        Some(ctx)
    }
}
