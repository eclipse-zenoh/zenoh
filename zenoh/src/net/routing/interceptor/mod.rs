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
use super::RoutingContext;
use zenoh_config::Config;
use zenoh_protocol::network::NetworkMessage;
use zenoh_result::ZResult;
use zenoh_transport::{multicast::TransportMulticast, unicast::TransportUnicast};

pub(crate) trait InterceptorTrait {
    fn intercept(
        &self,
        ctx: RoutingContext<NetworkMessage>,
    ) -> Option<RoutingContext<NetworkMessage>>;
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

pub(crate) fn interceptor_factories(_config: &Config) -> ZResult<Vec<InterceptorFactory>> {
    // Add interceptors here
    // @TODO build the list of intercetors with the correct order from the config
    // vec![Box::new(LoggerInterceptor {})]
    Ok(vec![])
}

pub(crate) struct InterceptorsChain {
    pub(crate) interceptors: Vec<Interceptor>,
}

impl InterceptorsChain {
    #[allow(dead_code)]
    pub(crate) fn empty() -> Self {
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
    fn intercept(
        &self,
        mut ctx: RoutingContext<NetworkMessage>,
    ) -> Option<RoutingContext<NetworkMessage>> {
        for interceptor in &self.interceptors {
            match interceptor.intercept(ctx) {
                Some(newctx) => ctx = newctx,
                None => {
                    log::trace!("Msg intercepted!");
                    return None;
                }
            }
        }
        Some(ctx)
    }
}

pub(crate) struct IngressMsgLogger {}

impl InterceptorTrait for IngressMsgLogger {
    fn intercept(
        &self,
        ctx: RoutingContext<NetworkMessage>,
    ) -> Option<RoutingContext<NetworkMessage>> {
        log::debug!(
            "Recv {} {} Expr:{:?}",
            ctx.inface()
                .map(|f| f.to_string())
                .unwrap_or("None".to_string()),
            ctx.msg,
            ctx.full_expr(),
        );
        Some(ctx)
    }
}
pub(crate) struct EgressMsgLogger {}

impl InterceptorTrait for EgressMsgLogger {
    fn intercept(
        &self,
        ctx: RoutingContext<NetworkMessage>,
    ) -> Option<RoutingContext<NetworkMessage>> {
        log::debug!("Send {} Expr:{:?}", ctx.msg, ctx.full_expr());
        Some(ctx)
    }
}

pub(crate) struct LoggerInterceptor {}

impl InterceptorFactoryTrait for LoggerInterceptor {
    fn new_transport_unicast(
        &self,
        transport: &TransportUnicast,
    ) -> (Option<IngressInterceptor>, Option<EgressInterceptor>) {
        log::debug!("New transport unicast {:?}", transport);
        (
            Some(Box::new(IngressMsgLogger {})),
            Some(Box::new(EgressMsgLogger {})),
        )
    }

    fn new_transport_multicast(&self, transport: &TransportMulticast) -> Option<EgressInterceptor> {
        log::debug!("New transport multicast {:?}", transport);
        Some(Box::new(EgressMsgLogger {}))
    }

    fn new_peer_multicast(&self, transport: &TransportMulticast) -> Option<IngressInterceptor> {
        log::debug!("New peer multicast {:?}", transport);
        Some(Box::new(IngressMsgLogger {}))
    }
}
