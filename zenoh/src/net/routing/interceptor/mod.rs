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
use zenoh_protocol::network::NetworkMessage;
use zenoh_transport::{TransportMulticast, TransportUnicast};

pub(crate) trait Intercept {
    fn intercept(&self, msg: NetworkMessage) -> Option<NetworkMessage>;
}

pub(crate) struct InterceptsChain {
    pub(crate) intercepts: Vec<Box<dyn Intercept + Send + Sync>>,
}

impl InterceptsChain {
    #[allow(dead_code)]
    pub(crate) fn empty() -> Self {
        Self { intercepts: vec![] }
    }
}

impl From<Vec<Box<dyn Intercept + Send + Sync>>> for InterceptsChain {
    fn from(intercepts: Vec<Box<dyn Intercept + Send + Sync>>) -> Self {
        InterceptsChain { intercepts }
    }
}

impl Intercept for InterceptsChain {
    fn intercept(&self, mut msg: NetworkMessage) -> Option<NetworkMessage> {
        for intercept in &self.intercepts {
            match intercept.intercept(msg) {
                Some(newmsg) => msg = newmsg,
                None => {
                    log::trace!("Msg intercepted!");
                    return None;
                }
            }
        }
        Some(msg)
    }
}

pub(crate) type IngressObj = Box<dyn Intercept + Send + Sync>;
pub(crate) type EgressObj = Box<dyn Intercept + Send + Sync>;

pub(crate) trait Interceptor {
    fn new_transport_unicast(
        &self,
        transport: &TransportUnicast,
    ) -> (Option<IngressObj>, Option<EgressObj>);
    fn new_transport_multicast(&self, transport: &TransportMulticast) -> Option<EgressObj>;
    fn new_peer_multicast(&self, transport: &TransportMulticast) -> Option<IngressObj>;
}

pub(crate) fn interceptors() -> Vec<Box<dyn Interceptor + Send + Sync>> {
    // Add interceptors here
    vec![]
}
