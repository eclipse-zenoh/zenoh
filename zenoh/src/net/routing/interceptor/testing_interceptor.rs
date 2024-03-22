use crate::KeyExpr;
use std::any::Any;

use crate::net::routing::RoutingContext;
use zenoh_protocol::network::NetworkMessage;
use zenoh_result::ZResult;
use zenoh_transport::unicast::authentication::AuthId;
use zenoh_transport::{multicast::TransportMulticast, unicast::TransportUnicast};

use super::{
    EgressInterceptor, IngressInterceptor, InterceptorFactory, InterceptorFactoryTrait,
    InterceptorTrait,
};

pub(crate) struct TestInterceptor {}

struct EgressTestInterceptor {
    _auth_id: Option<AuthId>,
}
struct IngressTestInterceptor {
    _auth_id: Option<AuthId>,
}

pub(crate) fn new_test_interceptor() -> ZResult<Vec<InterceptorFactory>> {
    let res: Vec<InterceptorFactory> = vec![Box::new(TestInterceptor {})];
    Ok(res)
}

impl InterceptorFactoryTrait for TestInterceptor {
    fn new_transport_unicast(
        &self,
        transport: &TransportUnicast,
    ) -> (Option<IngressInterceptor>, Option<EgressInterceptor>) {
        if let Ok(ids) = transport.get_auth_ids() {
            for id in ids {
                println!("value recevied in interceptor {:?}", id);
            }
        }
        (
            Some(Box::new(IngressTestInterceptor { _auth_id: None })),
            Some(Box::new(EgressTestInterceptor { _auth_id: None })),
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

impl InterceptorTrait for IngressTestInterceptor {
    fn compute_keyexpr_cache(&self, key_expr: &KeyExpr<'_>) -> Option<Box<dyn Any + Send + Sync>> {
        let _ = key_expr;
        None
    }
    fn intercept<'a>(
        &self,
        ctx: RoutingContext<NetworkMessage>,
        cache: Option<&Box<dyn Any + Send + Sync>>,
    ) -> Option<RoutingContext<NetworkMessage>> {
        let _ = cache;
        Some(ctx)
    }
}

impl InterceptorTrait for EgressTestInterceptor {
    fn compute_keyexpr_cache(&self, key_expr: &KeyExpr<'_>) -> Option<Box<dyn Any + Send + Sync>> {
        let _ = key_expr;
        None
    }
    fn intercept<'a>(
        &self,
        ctx: RoutingContext<NetworkMessage>,
        cache: Option<&Box<dyn Any + Send + Sync>>,
    ) -> Option<RoutingContext<NetworkMessage>> {
        let _ = cache;
        Some(ctx)
    }
}
