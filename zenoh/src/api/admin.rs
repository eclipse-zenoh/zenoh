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
use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    sync::Arc,
};

use zenoh_core::{Result as ZResult, Wait};
use zenoh_keyexpr::keyexpr;
use zenoh_macros::ke;
#[cfg(feature = "unstable")]
use zenoh_protocol::core::Reliability;
use zenoh_protocol::{core::WireExpr, network::NetworkMessageMut};
use zenoh_transport::{
    TransportEventHandler, TransportMulticastEventHandler, TransportPeer, TransportPeerEventHandler,
};

use crate as zenoh;
use crate::{
    api::{
        encoding::Encoding,
        key_expr::KeyExpr,
        queryable::Query,
        sample::{DataInfo, Locality, SampleKind},
        session::WeakSession,
        subscriber::SubscriberKind,
    },
    handlers::Callback,
};

#[cfg(feature = "internal")]
pub static KE_AT: &keyexpr = ke!("@");
#[cfg(not(feature = "internal"))]
static KE_AT: &keyexpr = ke!("@");
#[cfg(feature = "internal")]
pub static KE_ADV_PREFIX: &keyexpr = ke!("@adv");
#[cfg(not(feature = "internal"))]
static KE_ADV_PREFIX: &keyexpr = ke!("@adv");
#[cfg(feature = "internal")]
pub static KE_PUB: &keyexpr = ke!("pub");
#[cfg(not(feature = "internal"))]
static KE_PUB: &keyexpr = ke!("pub");
#[cfg(feature = "internal")]
pub static KE_SUB: &keyexpr = ke!("sub");
#[cfg(feature = "internal")]
pub static KE_EMPTY: &keyexpr = ke!("_");
#[cfg(not(feature = "internal"))]
static KE_EMPTY: &keyexpr = ke!("_");
#[cfg(feature = "internal")]
pub static KE_STAR: &keyexpr = ke!("*");
#[cfg(feature = "internal")]
pub static KE_STARSTAR: &keyexpr = ke!("**");
#[cfg(not(feature = "internal"))]
static KE_STARSTAR: &keyexpr = ke!("**");

static KE_SESSION: &keyexpr = ke!("session");
static KE_TRANSPORT_UNICAST: &keyexpr = ke!("transport/unicast");
static KE_LINK: &keyexpr = ke!("link");

pub(crate) fn init(session: WeakSession) {
    if let Ok(own_zid) = keyexpr::new(&session.zid().to_string()) {
        let _admin_qabl = session.declare_queryable_inner(
            &KeyExpr::from(KE_AT / own_zid / KE_SESSION / KE_STARSTAR),
            true,
            Locality::SessionLocal,
            Callback::new(Arc::new({
                let session = session.clone();
                move |q| on_admin_query(&session, KE_AT, q)
            })),
        );

        let adv_prefix = KE_ADV_PREFIX / KE_PUB / own_zid / KE_EMPTY / KE_EMPTY / KE_AT / KE_AT;

        let _admin_adv_qabl = session.declare_queryable_inner(
            &KeyExpr::from(&adv_prefix / own_zid / KE_SESSION / KE_STARSTAR),
            true,
            Locality::SessionLocal,
            Callback::new(Arc::new({
                let session = session.clone();
                move |q| on_admin_query(&session, &adv_prefix, q)
            })),
        );
    }
}

pub(crate) fn on_admin_query(session: &WeakSession, prefix: &keyexpr, query: Query) {
    fn reply_peer(prefix: &keyexpr, own_zid: &keyexpr, query: &Query, peer: TransportPeer) {
        let zid = peer.zid.to_string();
        if let Ok(zid) = keyexpr::new(&zid) {
            let key_expr = prefix / own_zid / KE_SESSION / KE_TRANSPORT_UNICAST / zid;
            if query.key_expr().intersects(&key_expr) {
                match serde_json::to_vec(&peer) {
                    Ok(bytes) => {
                        let reply_expr = KE_AT / own_zid / KE_SESSION / KE_TRANSPORT_UNICAST / zid;
                        let _ = query.reply(reply_expr, bytes).wait();
                    }
                    Err(e) => tracing::debug!("Admin query error: {}", e),
                }
            }

            for link in peer.links {
                let mut s = DefaultHasher::new();
                link.hash(&mut s);
                if let Ok(lid) = keyexpr::new(&s.finish().to_string()) {
                    let key_expr =
                        prefix / own_zid / KE_SESSION / KE_TRANSPORT_UNICAST / zid / KE_LINK / lid;
                    if query.key_expr().intersects(&key_expr) {
                        match serde_json::to_vec(&link) {
                            Ok(bytes) => {
                                let reply_expr = KE_AT
                                    / own_zid
                                    / KE_SESSION
                                    / KE_TRANSPORT_UNICAST
                                    / zid
                                    / KE_LINK
                                    / lid;
                                let _ = query.reply(reply_expr, bytes).wait();
                            }
                            Err(e) => tracing::debug!("Admin query error: {}", e),
                        }
                    }
                }
            }
        }
    }

    if let Ok(own_zid) = keyexpr::new(&session.zid().to_string()) {
        for transport in zenoh_runtime::ZRuntime::Net
            .block_in_place(session.runtime.manager().get_transports_unicast())
        {
            if let Ok(peer) = transport.get_peer() {
                reply_peer(prefix, own_zid, &query, peer);
            }
        }
        for transport in zenoh_runtime::ZRuntime::Net
            .block_in_place(session.runtime.manager().get_transports_multicast())
        {
            for peer in transport.get_peers().unwrap_or_default() {
                reply_peer(prefix, own_zid, &query, peer);
            }
        }
    }
}

#[derive(Clone)]
pub(crate) struct Handler {
    pub(crate) session: WeakSession,
}

impl Handler {
    pub(crate) fn new(session: WeakSession) -> Self {
        Self { session }
    }
}

impl TransportEventHandler for Handler {
    fn new_unicast(
        &self,
        peer: zenoh_transport::TransportPeer,
        _transport: zenoh_transport::unicast::TransportUnicast,
    ) -> ZResult<Arc<dyn zenoh_transport::TransportPeerEventHandler>> {
        self.new_peer(peer)
    }

    fn new_multicast(
        &self,
        _transport: zenoh_transport::multicast::TransportMulticast,
    ) -> ZResult<Arc<dyn zenoh_transport::TransportMulticastEventHandler>> {
        Ok(Arc::new(self.clone()))
    }
}

impl TransportMulticastEventHandler for Handler {
    fn new_peer(
        &self,
        peer: zenoh_transport::TransportPeer,
    ) -> ZResult<Arc<dyn TransportPeerEventHandler>> {
        if let Ok(own_zid) = keyexpr::new(&self.session.zid().to_string()) {
            if let Ok(zid) = keyexpr::new(&peer.zid.to_string()) {
                let expr =
                    WireExpr::from(&(KE_AT / own_zid / KE_SESSION / KE_TRANSPORT_UNICAST / zid))
                        .to_owned();
                let info = DataInfo {
                    encoding: Some(Encoding::APPLICATION_JSON),
                    ..Default::default()
                };
                self.session.execute_subscriber_callbacks(
                    true,
                    &expr,
                    Some(info),
                    serde_json::to_vec(&peer).unwrap().into(),
                    SubscriberKind::Subscriber,
                    #[cfg(feature = "unstable")]
                    Reliability::Reliable,
                    None,
                );
                Ok(Arc::new(PeerHandler {
                    expr,
                    session: self.session.clone(),
                }))
            } else {
                bail!("Unable to build keyexpr from zid")
            }
        } else {
            bail!("Unable to build keyexpr from zid")
        }
    }

    fn closed(&self) {}

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

pub(crate) struct PeerHandler {
    pub(crate) expr: WireExpr<'static>,
    pub(crate) session: WeakSession,
}

impl TransportPeerEventHandler for PeerHandler {
    fn handle_message(&self, _msg: NetworkMessageMut) -> ZResult<()> {
        Ok(())
    }

    fn new_link(&self, link: zenoh_link::Link) {
        let mut s = DefaultHasher::new();
        link.hash(&mut s);
        let info = DataInfo {
            encoding: Some(Encoding::APPLICATION_JSON),
            ..Default::default()
        };
        self.session.execute_subscriber_callbacks(
            true,
            &self
                .expr
                .clone()
                .with_suffix(&format!("/link/{}", s.finish())),
            Some(info),
            serde_json::to_vec(&link).unwrap().into(),
            SubscriberKind::Subscriber,
            #[cfg(feature = "unstable")]
            Reliability::Reliable,
            None,
        );
    }

    fn del_link(&self, link: zenoh_link::Link) {
        let mut s = DefaultHasher::new();
        link.hash(&mut s);
        let info = DataInfo {
            kind: SampleKind::Delete,
            ..Default::default()
        };
        self.session.execute_subscriber_callbacks(
            true,
            &self
                .expr
                .clone()
                .with_suffix(&format!("/link/{}", s.finish())),
            Some(info),
            vec![0u8; 0].into(),
            SubscriberKind::Subscriber,
            #[cfg(feature = "unstable")]
            Reliability::Reliable,
            None,
        );
    }

    fn closed(&self) {
        let info = DataInfo {
            kind: SampleKind::Delete,
            ..Default::default()
        };
        self.session.execute_subscriber_callbacks(
            true,
            &self.expr,
            Some(info),
            vec![0u8; 0].into(),
            SubscriberKind::Subscriber,
            #[cfg(feature = "unstable")]
            Reliability::Reliable,
            None,
        );
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
