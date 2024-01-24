//
// Copyright (c) 2024 ZettaScale Technology
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

use crate::net::routing::interceptor::*;
use crate::KeyExpr;
use std::sync::{Arc, Mutex};
use zenoh_config::DownsamplerConf;

pub(crate) struct IngressMsgDownsampler {
    conf: DownsamplerConf,
    latest_message_timestamp: Arc<Mutex<std::time::Instant>>,
}

impl InterceptorTrait for IngressMsgDownsampler {
    fn intercept(
        &self,
        ctx: RoutingContext<NetworkMessage>,
    ) -> Option<RoutingContext<NetworkMessage>> {
        if let Some(full_expr) = ctx.full_expr() {
            match KeyExpr::new(full_expr) {
                Ok(keyexpr) => {
                    if !self.conf.keyexpr.intersects(&keyexpr) {
                        return Some(ctx);
                    }

                    let timestamp = std::time::Instant::now();
                    let mut latest_message_timestamp =
                        self.latest_message_timestamp.lock().unwrap();

                    if timestamp - *latest_message_timestamp
                        >= std::time::Duration::from_millis(self.conf.threshold_ms)
                    {
                        *latest_message_timestamp = timestamp;
                        log::trace!("Interceptor: Passed threshold, passing.");
                        Some(ctx)
                    } else {
                        log::trace!("Interceptor: Skipped due to threshold.");
                        None
                    }
                }
                Err(_) => {
                    log::warn!("Interceptor: Wrong KeyExpr, passing.");
                    Some(ctx)
                }
            }
        } else {
            // message has no key expr
            Some(ctx)
        }
    }
}

impl IngressMsgDownsampler {
    pub fn new(conf: DownsamplerConf) -> Self {
        // TODO (sashacmc): I need just := 0, but how???
        let zero_ts =
            std::time::Instant::now() - std::time::Duration::from_micros(conf.threshold_ms);
        Self {
            conf,
            latest_message_timestamp: Arc::new(Mutex::new(zero_ts)),
        }
    }
}

pub(crate) struct EgressMsgDownsampler {}

impl InterceptorTrait for EgressMsgDownsampler {
    fn intercept(
        &self,
        ctx: RoutingContext<NetworkMessage>,
    ) -> Option<RoutingContext<NetworkMessage>> {
        // TODO(sashacmc): Do we need Ergress Downsampler?
        Some(ctx)
    }
}

pub struct DownsamplerInterceptor {
    conf: DownsamplerConf,
}

impl DownsamplerInterceptor {
    pub fn new(conf: DownsamplerConf) -> Self {
        log::debug!("DownsamplerInterceptor enabled: {:?}", conf);
        Self { conf }
    }
}

impl InterceptorFactoryTrait for DownsamplerInterceptor {
    fn new_transport_unicast(
        &self,
        transport: &TransportUnicast,
    ) -> (Option<IngressInterceptor>, Option<EgressInterceptor>) {
        log::debug!("New transport unicast {:?}", transport);
        (
            Some(Box::new(IngressMsgDownsampler::new(self.conf.clone()))),
            Some(Box::new(EgressMsgDownsampler {})),
        )
    }

    fn new_transport_multicast(&self, transport: &TransportMulticast) -> Option<EgressInterceptor> {
        log::debug!("New transport multicast {:?}", transport);
        Some(Box::new(EgressMsgDownsampler {}))
    }

    fn new_peer_multicast(&self, transport: &TransportMulticast) -> Option<IngressInterceptor> {
        log::debug!("New peer multicast {:?}", transport);
        Some(Box::new(IngressMsgDownsampler::new(self.conf.clone())))
    }
}
