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

use zenoh_protocol::core::ZenohIdProto;
use zenoh_result::ZResult;
use zenoh_transport::{
    TransportEventHandler, TransportMulticastEventHandler, TransportPeer, TransportPeerEventHandler,
};

use crate::{api::session::WeakSession, sample::SampleKind};
pub(crate) struct ConnectivityHandler {
    session: WeakSession,
}

impl ConnectivityHandler {
    pub(crate) fn new(session: WeakSession) -> Self {
        Self { session }
    }
}

impl TransportEventHandler for ConnectivityHandler {
    fn new_unicast(
        &self,
        peer: TransportPeer,
        _transport: zenoh_transport::unicast::TransportUnicast,
    ) -> ZResult<Arc<dyn TransportPeerEventHandler>> {
        // Broadcast transport opened event only if session is not closed
        if !self.session.is_closed() {
            self.session
                .broadcast_transport_event(SampleKind::Put, &peer, false);
        }

        // Return ConnectivityPeerHandler
        Ok(Arc::new(ConnectivityPeerHandler {
            session: self.session.clone(),
            peer_zid: peer.zid,
            peer,
            is_multicast: false,
        }))
    }

    fn new_multicast(
        &self,
        _transport: zenoh_transport::multicast::TransportMulticast,
    ) -> ZResult<Arc<dyn TransportMulticastEventHandler>> {
        Ok(Arc::new(ConnectivityMulticastHandler {
            session: self.session.clone(),
        }))
    }
}

/// Peer handler for connectivity events
pub(crate) struct ConnectivityPeerHandler {
    session: WeakSession,
    peer_zid: ZenohIdProto,
    peer: TransportPeer,
    is_multicast: bool,
}

impl TransportPeerEventHandler for ConnectivityPeerHandler {
    fn handle_message(&self, _msg: zenoh_protocol::network::NetworkMessageMut) -> ZResult<()> {
        // Connectivity doesn't need to handle messages
        Ok(())
    }

    fn new_link(&self, link: zenoh_link::Link) {
        // Check if session is closed
        if self.session.is_closed() {
            return;
        }

        // Broadcast link added event
        self.session.broadcast_link_event(
            SampleKind::Put,
            self.peer_zid,
            &link,
            self.is_multicast,
            self.peer.is_qos,
        );
    }

    fn del_link(&self, link: zenoh_link::Link) {
        // Check if session is closed
        if self.session.is_closed() {
            return;
        }

        // Broadcast link removed event
        self.session.broadcast_link_event(
            SampleKind::Delete,
            self.peer_zid,
            &link,
            self.is_multicast,
            self.peer.is_qos,
        );
    }

    fn closed(&self) {
        // Check if session is closed
        if self.session.is_closed() {
            return;
        }

        // Broadcast transport closed event
        self.session
            .broadcast_transport_event(SampleKind::Delete, &self.peer, self.is_multicast);
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// Multicast handler for connectivity events
pub(crate) struct ConnectivityMulticastHandler {
    session: WeakSession,
}

impl TransportMulticastEventHandler for ConnectivityMulticastHandler {
    fn new_peer(&self, peer: TransportPeer) -> ZResult<Arc<dyn TransportPeerEventHandler>> {
        // Broadcast transport opened event only if session is not closed
        if !self.session.is_closed() {
            self.session
                .broadcast_transport_event(SampleKind::Put, &peer, true);
        }

        // Return ConnectivityPeerHandler
        Ok(Arc::new(ConnectivityPeerHandler {
            session: self.session.clone(),
            peer_zid: peer.zid,
            peer,
            is_multicast: true,
        }))
    }

    fn closed(&self) {
        // Nothing to do for multicast group closure
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
