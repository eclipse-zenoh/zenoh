//
// Copyright (c) 2023 ZettaScale Technology
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
//!
mod access_control;
use access_control::acl_interceptor_factories;
use nonempty_collections::NEVec;
use zenoh_link::LinkAuthId;

mod authorization;
use std::any::Any;

mod low_pass;
use low_pass::low_pass_interceptor_factories;
use zenoh_config::{Config, InterceptorFlow, InterceptorLink};
use zenoh_keyexpr::{keyexpr, OwnedKeyExpr};
use zenoh_protocol::network::NetworkMessageMut;
use zenoh_result::ZResult;
use zenoh_transport::{multicast::TransportMulticast, unicast::TransportUnicast};

pub mod downsampling;
use crate::{
    key_expr::KeyExpr,
    net::routing::{
        dispatcher::face::Face, interceptor::downsampling::downsampling_interceptor_factories,
    },
};

pub mod qos_overwrite;
use crate::net::routing::interceptor::qos_overwrite::qos_overwrite_interceptor_factories;

#[derive(Default, Debug)]
pub struct InterfaceEnabled {
    pub ingress: bool,
    pub egress: bool,
}

impl From<&NEVec<InterceptorFlow>> for InterfaceEnabled {
    fn from(value: &NEVec<InterceptorFlow>) -> Self {
        let mut res = Self {
            ingress: false,
            egress: false,
        };
        for v in value {
            match v {
                InterceptorFlow::Egress => res.egress = true,
                InterceptorFlow::Ingress => res.ingress = true,
            }
        }
        res
    }
}

/// Wrapper for InterceptorLink in order to implement From trait.
pub(crate) struct InterceptorLinkWrapper(pub(crate) InterceptorLink);

impl From<&LinkAuthId> for InterceptorLinkWrapper {
    fn from(value: &LinkAuthId) -> Self {
        match value {
            LinkAuthId::Tls(_) => Self(InterceptorLink::Tls),
            LinkAuthId::Quic(_) => Self(InterceptorLink::Quic),
            LinkAuthId::Tcp => Self(InterceptorLink::Tcp),
            LinkAuthId::Udp => Self(InterceptorLink::Udp),
            LinkAuthId::Serial => Self(InterceptorLink::Serial),
            LinkAuthId::Unixpipe => Self(InterceptorLink::Unixpipe),
            LinkAuthId::UnixsockStream => Self(InterceptorLink::UnixsockStream),
            LinkAuthId::Vsock => Self(InterceptorLink::Vsock),
            LinkAuthId::Ws => Self(InterceptorLink::Ws),
        }
    }
}

pub(crate) trait InterceptorContext {
    #[allow(dead_code)]
    fn face(&self) -> Option<Face>;
    fn full_expr(&self, msg: &NetworkMessageMut) -> Option<&str>;
    #[inline]
    fn full_keyexpr(&self, msg: &NetworkMessageMut) -> Option<KeyExpr<'_>> {
        let full_expr = self.full_expr(msg)?;
        KeyExpr::new(full_expr).ok()
    }
    fn get_cache(&self, msg: &NetworkMessageMut) -> Option<&Box<dyn Any + Send + Sync>>;
}

pub(crate) trait InterceptorTrait {
    fn compute_keyexpr_cache(&self, key_expr: &keyexpr) -> Option<Box<dyn Any + Send + Sync>>;

    fn intercept(&self, msg: &mut NetworkMessageMut, ctx: &mut dyn InterceptorContext) -> bool;
}

pub(crate) type Interceptor = Box<dyn InterceptorTrait + Send + Sync>;
pub(crate) type IngressInterceptor = Interceptor;
pub(crate) type EgressInterceptor = Interceptor;

pub(crate) trait InterceptorFactoryTrait {
    fn new_transport_unicast(
        &self,
        transport: &TransportUnicast,
    ) -> (Option<IngressInterceptor>, Option<EgressInterceptor>);
    fn new_transport_multicast(&self, transport: &TransportMulticast) -> Option<EgressInterceptor>;
    fn new_peer_multicast(&self, transport: &TransportMulticast) -> Option<IngressInterceptor>;
}

pub(crate) type InterceptorFactory = Box<dyn InterceptorFactoryTrait + Send + Sync>;

pub(crate) fn interceptor_factories(config: &Config) -> ZResult<Vec<InterceptorFactory>> {
    let mut res: Vec<InterceptorFactory> = vec![];
    // Uncomment to log the interceptors initialisation
    // res.push(Box::new(LoggerInterceptor {}));
    #[cfg(test)]
    if let Some(id) = config.id() {
        if let Some(test_interceptors) = tests::ID_TO_INTERCEPTOR_FACTORIES.lock().unwrap().get(id)
        {
            res.extend((test_interceptors.as_ref())());
        }
    }
    res.extend(downsampling_interceptor_factories(config.downsampling())?);
    res.extend(acl_interceptor_factories(config.access_control())?);
    res.extend(qos_overwrite_interceptor_factories(config.qos().network())?);
    res.extend(low_pass_interceptor_factories(config.low_pass_filter())?);
    Ok(res)
}

pub(crate) struct InterceptorsChain {
    pub(crate) interceptors: Vec<Interceptor>,
    pub(crate) version: usize,
}

impl InterceptorsChain {
    #[allow(dead_code)]
    pub(crate) fn empty() -> Self {
        Self {
            interceptors: vec![],
            version: 0,
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.interceptors.is_empty()
    }

    pub(crate) fn new(interceptors: Vec<Interceptor>, version: usize) -> Self {
        InterceptorsChain {
            interceptors,
            version,
        }
    }
}

impl InterceptorTrait for InterceptorsChain {
    fn compute_keyexpr_cache(&self, key_expr: &keyexpr) -> Option<Box<dyn Any + Send + Sync>> {
        Some(Box::new(
            self.interceptors
                .iter()
                .map(|i| i.compute_keyexpr_cache(key_expr))
                .collect::<Vec<Option<Box<dyn Any + Send + Sync>>>>(),
        ))
    }

    fn intercept<'a>(&self, msg: &mut NetworkMessageMut, ctx: &mut dyn InterceptorContext) -> bool {
        let mut ctx = ChainContext { ctx, index: 0 };
        for interceptor in &self.interceptors {
            if !interceptor.intercept(msg, &mut ctx as &mut dyn InterceptorContext) {
                tracing::trace!("Msg intercepted!");
                return false;
            }
            ctx.index += 1;
        }
        true
    }
}

struct ChainContext<'a> {
    ctx: &'a mut dyn InterceptorContext,
    index: usize,
}

impl InterceptorContext for ChainContext<'_> {
    fn face(&self) -> Option<Face> {
        self.ctx.face()
    }

    fn full_expr(&self, msg: &NetworkMessageMut) -> Option<&str> {
        self.ctx.full_expr(msg)
    }

    fn full_keyexpr(&self, msg: &NetworkMessageMut) -> Option<KeyExpr<'_>> {
        self.ctx.full_keyexpr(msg)
    }

    fn get_cache(&self, msg: &NetworkMessageMut) -> Option<&Box<dyn Any + Send + Sync>> {
        let caches = self.ctx.get_cache(msg)?;
        let caches = caches.downcast_ref::<Vec<Option<Box<dyn Any + Send + Sync>>>>()?;
        caches[self.index].as_ref()
    }
}

#[allow(dead_code)]
pub(crate) struct IngressMsgLogger {}

impl InterceptorTrait for IngressMsgLogger {
    fn compute_keyexpr_cache(&self, key_expr: &keyexpr) -> Option<Box<dyn Any + Send + Sync>> {
        Some(Box::new(OwnedKeyExpr::from(key_expr)))
    }

    fn intercept(&self, msg: &mut NetworkMessageMut, ctx: &mut dyn InterceptorContext) -> bool {
        let expr = ctx
            .get_cache(msg)
            .and_then(|i| i.downcast_ref::<String>().map(|e| e.as_str()))
            .or_else(|| ctx.full_expr(msg));

        tracing::debug!(
            "{} Recv {} Expr:{:?}",
            ctx.face()
                .map(|f| f.to_string())
                .unwrap_or("None".to_string()),
            msg,
            expr,
        );
        true
    }
}

#[allow(dead_code)]
pub(crate) struct EgressMsgLogger {}

impl InterceptorTrait for EgressMsgLogger {
    fn compute_keyexpr_cache(&self, key_expr: &keyexpr) -> Option<Box<dyn Any + Send + Sync>> {
        Some(Box::new(OwnedKeyExpr::from(key_expr)))
    }

    fn intercept(&self, msg: &mut NetworkMessageMut, ctx: &mut dyn InterceptorContext) -> bool {
        let expr = ctx
            .get_cache(msg)
            .and_then(|i| i.downcast_ref::<String>().map(|e| e.as_str()))
            .or_else(|| ctx.full_expr(msg));
        tracing::debug!(
            "{} Send {} Expr:{:?}",
            ctx.face()
                .map(|f| f.to_string())
                .unwrap_or("None".to_string()),
            msg,
            expr
        );
        true
    }
}

#[allow(dead_code)]
pub(crate) struct LoggerInterceptor {}

impl InterceptorFactoryTrait for LoggerInterceptor {
    fn new_transport_unicast(
        &self,
        transport: &TransportUnicast,
    ) -> (Option<IngressInterceptor>, Option<EgressInterceptor>) {
        tracing::debug!("New transport unicast {:?}", transport);
        (
            Some(Box::new(IngressMsgLogger {})),
            Some(Box::new(EgressMsgLogger {})),
        )
    }

    fn new_transport_multicast(&self, transport: &TransportMulticast) -> Option<EgressInterceptor> {
        tracing::debug!("New transport multicast {:?}", transport);
        Some(Box::new(EgressMsgLogger {}))
    }

    fn new_peer_multicast(&self, transport: &TransportMulticast) -> Option<IngressInterceptor> {
        tracing::debug!("New peer multicast {:?}", transport);
        Some(Box::new(IngressMsgLogger {}))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::{
        collections::HashMap,
        sync::{Arc, Mutex},
    };

    use once_cell::sync::Lazy;
    use zenoh_config::ZenohId;

    use super::InterceptorFactory;

    #[allow(clippy::type_complexity)]
    pub(crate) static ID_TO_INTERCEPTOR_FACTORIES: Lazy<
        Arc<Mutex<HashMap<ZenohId, Box<dyn Fn() -> Vec<InterceptorFactory> + Sync + Send>>>>,
    > = Lazy::new(|| Arc::new(Mutex::new(HashMap::new())));
}
