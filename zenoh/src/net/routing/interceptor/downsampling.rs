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
use zenoh_config::DownsamplingItemConf;
use zenoh_protocol::core::key_expr::OwnedKeyExpr;

const RATELIMIT_STRATEGY: &str = "ratelimit";

// TODO(sashacmc): this is ratelimit strategy, we can also add decimation (with "factor" option)

pub(crate) struct EgressMsgDownsamplerRatelimit {
    keyexprs: Option<Vec<OwnedKeyExpr>>,
    threshold: std::time::Duration,
    latest_message_timestamp: Arc<Mutex<std::time::Instant>>,
}

impl InterceptorTrait for EgressMsgDownsamplerRatelimit {
    fn intercept(
        &self,
        ctx: RoutingContext<NetworkMessage>,
    ) -> Option<RoutingContext<NetworkMessage>> {
        if let Some(cfg_keyexprs) = self.keyexprs.as_ref() {
            let mut matched = false;
            if let Some(keyexpr) = ctx.full_key_expr() {
                for cfg_keyexpr in cfg_keyexprs {
                    if cfg_keyexpr.intersects(&keyexpr) {
                        matched = true;
                    }
                }
            }
            if !matched {
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

impl EgressMsgDownsamplerRatelimit {
    pub fn new(conf: DownsamplingItemConf) -> Self {
        if let Some(threshold_ms) = conf.threshold_ms {
            let threshold = std::time::Duration::from_millis(threshold_ms);
            Self {
                keyexprs: conf.keyexprs,
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
    conf: DownsamplingItemConf,
}

impl DownsamplerInterceptor {
    pub fn new(conf: DownsamplingItemConf) -> Self {
        log::debug!("DownsamplerInterceptor enabled: {:?}", conf);
        Self { conf }
    }
}

impl InterceptorFactoryTrait for DownsamplerInterceptor {
    fn new_transport_unicast(
        &self,
        transport: &TransportUnicast,
    ) -> (Option<IngressInterceptor>, Option<EgressInterceptor>) {
        log::debug!("New downsampler transport unicast {:?}", transport);
        if let Some(interfaces) = &self.conf.interfaces {
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

        let strategy = self
            .conf
            .strategy
            .as_ref()
            .map_or_else(|| RATELIMIT_STRATEGY.to_string(), |s| s.clone());

        if strategy == RATELIMIT_STRATEGY {
            (
                None,
                Some(Box::new(EgressMsgDownsamplerRatelimit::new(
                    self.conf.clone(),
                ))),
            )
        } else {
            panic!("Unsupported downsampling strategy: {}", strategy)
        }
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
