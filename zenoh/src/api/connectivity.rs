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

//! Connectivity event handler - independent from adminspace
//!
//! This handler subscribes to transport events and broadcasts them
//! to user-registered callbacks through the connectivity API.

use std::sync::Arc;

use zenoh_result::ZResult;
use zenoh_transport::{
    TransportEventHandler, TransportMulticastEventHandler, TransportPeer, TransportPeerEventHandler,
};

use crate::net::runtime::DynamicRuntime;

/// Handler for connectivity events - independent from adminspace
#[cfg(feature = "unstable")]
pub(crate) struct ConnectivityHandler {
    runtime: DynamicRuntime,
}

#[cfg(feature = "unstable")]
impl ConnectivityHandler {
    pub(crate) fn new(runtime: DynamicRuntime) -> Self {
        Self { runtime }
    }
}

#[cfg(feature = "unstable")]
impl TransportEventHandler for ConnectivityHandler {
    fn new_unicast(
        &self,
        peer: TransportPeer,
        _transport: zenoh_transport::unicast::TransportUnicast,
    ) -> ZResult<Arc<dyn TransportPeerEventHandler>> {
        // Broadcast transport opened event
        use crate::api::sample::SampleKind;
        self.runtime
            .broadcast_transport_event(SampleKind::Put, &peer);

        // Return ConnectivityPeerHandler
        Ok(Arc::new(ConnectivityPeerHandler {
            runtime: self.runtime.clone(),
            peer_zid: peer.zid,
            peer,
        }))
    }

    fn new_multicast(
        &self,
        _transport: zenoh_transport::multicast::TransportMulticast,
    ) -> ZResult<Arc<dyn TransportMulticastEventHandler>> {
        Ok(Arc::new(ConnectivityMulticastHandler {
            runtime: self.runtime.clone(),
        }))
    }
}

/// Peer handler for connectivity events
#[cfg(feature = "unstable")]
pub(crate) struct ConnectivityPeerHandler {
    runtime: DynamicRuntime,
    peer_zid: zenoh_protocol::core::ZenohIdProto,
    peer: TransportPeer,
}

#[cfg(feature = "unstable")]
impl TransportPeerEventHandler for ConnectivityPeerHandler {
    fn handle_message(&self, _msg: zenoh_protocol::network::NetworkMessageMut) -> ZResult<()> {
        // Connectivity doesn't need to handle messages
        Ok(())
    }

    fn new_link(&self, link: zenoh_link::Link) {
        // Broadcast link added event
        use crate::api::sample::SampleKind;
        self.runtime
            .broadcast_link_event(SampleKind::Put, self.peer_zid, &link);
    }

    fn del_link(&self, link: zenoh_link::Link) {
        // Broadcast link removed event
        use crate::api::sample::SampleKind;
        self.runtime
            .broadcast_link_event(SampleKind::Delete, self.peer_zid, &link);
    }

    fn closed(&self) {
        // Broadcast transport closed event
        use crate::api::sample::SampleKind;
        self.runtime
            .broadcast_transport_event(SampleKind::Delete, &self.peer);
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// Multicast handler for connectivity events
#[cfg(feature = "unstable")]
pub(crate) struct ConnectivityMulticastHandler {
    runtime: DynamicRuntime,
}

#[cfg(feature = "unstable")]
impl TransportMulticastEventHandler for ConnectivityMulticastHandler {
    fn new_peer(&self, peer: TransportPeer) -> ZResult<Arc<dyn TransportPeerEventHandler>> {
        // Broadcast transport opened event
        use crate::api::sample::SampleKind;
        self.runtime
            .broadcast_transport_event(SampleKind::Put, &peer);

        // Return ConnectivityPeerHandler
        Ok(Arc::new(ConnectivityPeerHandler {
            runtime: self.runtime.clone(),
            peer_zid: peer.zid,
            peer,
        }))
    }

    fn closed(&self) {
        // Nothing to do for multicast group closure
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
