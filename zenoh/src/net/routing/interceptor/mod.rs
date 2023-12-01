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
use zenoh_protocol::network::NetworkMessage;
use zenoh_transport::{TransportMulticast, TransportUnicast};

pub(crate) trait InterceptTrait {
    fn intercept(
        &self,
        ctx: RoutingContext<NetworkMessage>,
    ) -> Option<RoutingContext<NetworkMessage>>;
}

pub(crate) type Intercept = Box<dyn InterceptTrait + Send + Sync>;
pub(crate) type IngressIntercept = Intercept;
pub(crate) type EgressIntercept = Intercept;

pub(crate) trait InterceptorTrait {
    fn new_transport_unicast(
        &self,
        transport: &TransportUnicast,
    ) -> (Option<IngressIntercept>, Option<EgressIntercept>);
    fn new_transport_multicast(&self, transport: &TransportMulticast) -> Option<EgressIntercept>;
    fn new_peer_multicast(&self, transport: &TransportMulticast) -> Option<IngressIntercept>;
}

pub(crate) type Interceptor = Box<dyn InterceptorTrait + Send + Sync>;

pub(crate) fn interceptors() -> Vec<Interceptor> {
    // Add interceptors here
    // vec![Box::new(LoggerInterceptor {})]
    vec![]
}

pub(crate) struct InterceptsChain {
    pub(crate) intercepts: Vec<Intercept>,
}

impl InterceptsChain {
    #[allow(dead_code)]
    pub(crate) fn empty() -> Self {
        Self { intercepts: vec![] }
    }
}

impl From<Vec<Intercept>> for InterceptsChain {
    fn from(intercepts: Vec<Intercept>) -> Self {
        InterceptsChain { intercepts }
    }
}

impl InterceptTrait for InterceptsChain {
    fn intercept(
        &self,
        mut ctx: RoutingContext<NetworkMessage>,
    ) -> Option<RoutingContext<NetworkMessage>> {
        for intercept in &self.intercepts {
            match intercept.intercept(ctx) {
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

impl InterceptTrait for IngressMsgLogger {
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

impl InterceptTrait for EgressMsgLogger {
    fn intercept(
        &self,
        ctx: RoutingContext<NetworkMessage>,
    ) -> Option<RoutingContext<NetworkMessage>> {
        log::debug!("Send {} Expr:{:?}", ctx.msg, ctx.full_expr());
        Some(ctx)
    }
}

pub(crate) struct LoggerInterceptor {}

impl InterceptorTrait for LoggerInterceptor {
    fn new_transport_unicast(
        &self,
        transport: &TransportUnicast,
    ) -> (Option<IngressIntercept>, Option<EgressIntercept>) {
        log::debug!("New transport unicast {:?}", transport);
        (
            Some(Box::new(IngressMsgLogger {})),
            Some(Box::new(EgressMsgLogger {})),
        )
    }

    fn new_transport_multicast(&self, transport: &TransportMulticast) -> Option<EgressIntercept> {
        log::debug!("New transport multicast {:?}", transport);
        Some(Box::new(EgressMsgLogger {}))
    }

    fn new_peer_multicast(&self, transport: &TransportMulticast) -> Option<IngressIntercept> {
        log::debug!("New peer multicast {:?}", transport);
        Some(Box::new(IngressMsgLogger {}))
    }
}
