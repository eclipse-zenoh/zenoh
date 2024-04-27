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
//! [Click here for Zenoh's documentation](../zenoh/index.html)
//!
mod access_control;
use access_control::acl_interceptor_factories;

mod authorization;
use super::RoutingContext;
use crate::sealed::api::key_expr::KeyExpr;
use std::any::Any;

use zenoh_config::Config;
use zenoh_protocol::network::NetworkMessage;
use zenoh_result::ZResult;
use zenoh_transport::{multicast::TransportMulticast, unicast::TransportUnicast};

pub(in crate::sealed) mod downsampling;
use crate::sealed::net::routing::interceptor::downsampling::downsampling_interceptor_factories;

pub(in crate::sealed) trait InterceptorTrait {
    fn compute_keyexpr_cache(&self, key_expr: &KeyExpr<'_>) -> Option<Box<dyn Any + Send + Sync>>;

    fn intercept(
        &self,
        ctx: RoutingContext<NetworkMessage>,
        cache: Option<&Box<dyn Any + Send + Sync>>,
    ) -> Option<RoutingContext<NetworkMessage>>;
}

pub(in crate::sealed) type Interceptor = Box<dyn InterceptorTrait + Send + Sync>;
pub(in crate::sealed) type IngressInterceptor = Interceptor;
pub(in crate::sealed) type EgressInterceptor = Interceptor;

pub(in crate::sealed) trait InterceptorFactoryTrait {
    fn new_transport_unicast(
        &self,
        transport: &TransportUnicast,
    ) -> (Option<IngressInterceptor>, Option<EgressInterceptor>);
    fn new_transport_multicast(&self, transport: &TransportMulticast) -> Option<EgressInterceptor>;
    fn new_peer_multicast(&self, transport: &TransportMulticast) -> Option<IngressInterceptor>;
}

pub(in crate::sealed) type InterceptorFactory = Box<dyn InterceptorFactoryTrait + Send + Sync>;

pub(in crate::sealed) fn interceptor_factories(
    config: &Config,
) -> ZResult<Vec<InterceptorFactory>> {
    let mut res: Vec<InterceptorFactory> = vec![];
    // Uncomment to log the interceptors initialisation
    // res.push(Box::new(LoggerInterceptor {}));
    res.extend(downsampling_interceptor_factories(config.downsampling())?);
    res.extend(acl_interceptor_factories(config.access_control())?);
    Ok(res)
}

pub(in crate::sealed) struct InterceptorsChain {
    pub(in crate::sealed) interceptors: Vec<Interceptor>,
}

impl InterceptorsChain {
    #[allow(dead_code)]
    pub(in crate::sealed) fn empty() -> Self {
        Self {
            interceptors: vec![],
        }
    }
}

impl From<Vec<Interceptor>> for InterceptorsChain {
    fn from(interceptors: Vec<Interceptor>) -> Self {
        InterceptorsChain { interceptors }
    }
}

impl InterceptorTrait for InterceptorsChain {
    fn compute_keyexpr_cache(&self, key_expr: &KeyExpr<'_>) -> Option<Box<dyn Any + Send + Sync>> {
        Some(Box::new(
            self.interceptors
                .iter()
                .map(|i| i.compute_keyexpr_cache(key_expr))
                .collect::<Vec<Option<Box<dyn Any + Send + Sync>>>>(),
        ))
    }

    fn intercept<'a>(
        &self,
        mut ctx: RoutingContext<NetworkMessage>,
        caches: Option<&Box<dyn Any + Send + Sync>>,
    ) -> Option<RoutingContext<NetworkMessage>> {
        let caches =
            caches.and_then(|i| i.downcast_ref::<Vec<Option<Box<dyn Any + Send + Sync>>>>());
        for (idx, interceptor) in self.interceptors.iter().enumerate() {
            let cache = caches
                .and_then(|caches| caches.get(idx).map(|k| k.as_ref()))
                .flatten();
            match interceptor.intercept(ctx, cache) {
                Some(newctx) => ctx = newctx,
                None => {
                    tracing::trace!("Msg intercepted!");
                    return None;
                }
            }
        }
        Some(ctx)
    }
}

pub(in crate::sealed) struct ComputeOnMiss<T: InterceptorTrait> {
    interceptor: T,
}

impl<T: InterceptorTrait> ComputeOnMiss<T> {
    #[allow(dead_code)]
    pub(in crate::sealed) fn new(interceptor: T) -> Self {
        Self { interceptor }
    }
}

impl<T: InterceptorTrait> InterceptorTrait for ComputeOnMiss<T> {
    #[inline]
    fn compute_keyexpr_cache(&self, key_expr: &KeyExpr<'_>) -> Option<Box<dyn Any + Send + Sync>> {
        self.interceptor.compute_keyexpr_cache(key_expr)
    }

    #[inline]
    fn intercept<'a>(
        &self,
        ctx: RoutingContext<NetworkMessage>,
        cache: Option<&Box<dyn Any + Send + Sync>>,
    ) -> Option<RoutingContext<NetworkMessage>> {
        if cache.is_some() {
            self.interceptor.intercept(ctx, cache)
        } else if let Some(key_expr) = ctx.full_key_expr() {
            self.interceptor.intercept(
                ctx,
                self.interceptor
                    .compute_keyexpr_cache(&key_expr.into())
                    .as_ref(),
            )
        } else {
            self.interceptor.intercept(ctx, cache)
        }
    }
}

pub(in crate::sealed) struct IngressMsgLogger {}

impl InterceptorTrait for IngressMsgLogger {
    fn compute_keyexpr_cache(&self, key_expr: &KeyExpr<'_>) -> Option<Box<dyn Any + Send + Sync>> {
        Some(Box::new(key_expr.to_string()))
    }

    fn intercept(
        &self,
        ctx: RoutingContext<NetworkMessage>,
        cache: Option<&Box<dyn Any + Send + Sync>>,
    ) -> Option<RoutingContext<NetworkMessage>> {
        let expr = cache
            .and_then(|i| i.downcast_ref::<String>().map(|e| e.as_str()))
            .or_else(|| ctx.full_expr());

        tracing::debug!(
            "{} Recv {} Expr:{:?}",
            ctx.inface()
                .map(|f| f.to_string())
                .unwrap_or("None".to_string()),
            ctx.msg,
            expr,
        );
        Some(ctx)
    }
}
pub(in crate::sealed) struct EgressMsgLogger {}

impl InterceptorTrait for EgressMsgLogger {
    fn compute_keyexpr_cache(&self, key_expr: &KeyExpr<'_>) -> Option<Box<dyn Any + Send + Sync>> {
        Some(Box::new(key_expr.to_string()))
    }

    fn intercept(
        &self,
        ctx: RoutingContext<NetworkMessage>,
        cache: Option<&Box<dyn Any + Send + Sync>>,
    ) -> Option<RoutingContext<NetworkMessage>> {
        let expr = cache
            .and_then(|i| i.downcast_ref::<String>().map(|e| e.as_str()))
            .or_else(|| ctx.full_expr());
        tracing::debug!(
            "{} Send {} Expr:{:?}",
            ctx.outface()
                .map(|f| f.to_string())
                .unwrap_or("None".to_string()),
            ctx.msg,
            expr
        );
        Some(ctx)
    }
}

pub(in crate::sealed) struct LoggerInterceptor {}

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
