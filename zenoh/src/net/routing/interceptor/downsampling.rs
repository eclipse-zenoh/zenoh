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
use std::sync::{Arc, Mutex};
use zenoh_config::DownsamplerConf;
use zenoh_protocol::core::key_expr::OwnedKeyExpr;

// TODO(sashacmc): this is ratelimit strategy, we should also add decimation (with "factor" option)

pub(crate) struct IngressMsgDownsampler {}

impl InterceptorTrait for IngressMsgDownsampler {
    fn intercept(
        &self,
        ctx: RoutingContext<NetworkMessage>,
    ) -> Option<RoutingContext<NetworkMessage>> {
        Some(ctx)
    }
}

pub(crate) struct EgressMsgDownsampler {
    keyexpr: Option<OwnedKeyExpr>,
    threshold: std::time::Duration,
    latest_message_timestamp: Arc<Mutex<std::time::Instant>>,
}

impl InterceptorTrait for EgressMsgDownsampler {
    fn intercept(
        &self,
        ctx: RoutingContext<NetworkMessage>,
    ) -> Option<RoutingContext<NetworkMessage>> {
        if let Some(cfg_keyexpr) = self.keyexpr.as_ref() {
            if let Some(keyexpr) = ctx.full_key_expr() {
                if !cfg_keyexpr.intersects(&keyexpr) {
                    return Some(ctx);
                }
            } else {
                return Some(ctx);
            }
        }

        let timestamp = std::time::Instant::now();
        let mut latest_message_timestamp = self.latest_message_timestamp.lock().unwrap();

        if timestamp - *latest_message_timestamp >= self.threshold {
            *latest_message_timestamp = timestamp;
            log::debug!("Interceptor: Passed threshold, passing.");
            Some(ctx)
        } else {
            log::debug!("Interceptor: Skipped due to threshold.");
            None
        }
    }
}

impl EgressMsgDownsampler {
    pub fn new(conf: DownsamplerConf) -> Self {
        if let Some(threshold_ms) = conf.threshold_ms {
            let threshold = std::time::Duration::from_millis(threshold_ms);
            Self {
                keyexpr: conf.keyexpr,
                threshold,
                // TODO (sashacmc): I need just := 0, but how???
                latest_message_timestamp: Arc::new(Mutex::new(
                    std::time::Instant::now() - threshold,
                )),
            }
        } else {
            // TODO (sashacmc): how correctly process an error?
            panic!("Rate limit downsampler shoud have a threshold_ms parameter");
        }
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
            Some(Box::new(IngressMsgDownsampler {})),
            Some(Box::new(EgressMsgDownsampler::new(self.conf.clone()))),
        )
    }

    fn new_transport_multicast(&self, transport: &TransportMulticast) -> Option<EgressInterceptor> {
        log::debug!("New transport multicast {:?}", transport);
        Some(Box::new(EgressMsgDownsampler::new(self.conf.clone())))
    }

    fn new_peer_multicast(&self, transport: &TransportMulticast) -> Option<IngressInterceptor> {
        log::debug!("New peer multicast {:?}", transport);
        Some(Box::new(IngressMsgDownsampler {}))
    }
}
