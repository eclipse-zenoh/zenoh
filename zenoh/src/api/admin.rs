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
use zenoh_keyexpr::{keyexpr, OwnedKeyExpr};
use zenoh_macros::ke;
#[cfg(feature = "unstable")]
use zenoh_protocol::core::Reliability;
use zenoh_protocol::{
    core::WireExpr,
    network::NetworkMessageMut,
    zenoh::{Del, PushBody, Put},
};
use zenoh_transport::{
    TransportEventHandler, TransportMulticastEventHandler, TransportPeerEventHandler,
};

use crate::{
    self as zenoh,
    api::info::{Link, Transport},
};
use crate::{
    api::{
        encoding::Encoding,
        key_expr::KeyExpr,
        queryable::Query,
        sample::{Locality, SampleKind},
        session::WeakSession,
        subscriber::SubscriberKind,
    },
    bytes::ZBytes,
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
static KE_TRANSPORT_MULTICAST: &keyexpr = ke!("transport/multicast");
static KE_LINK: &keyexpr = ke!("link");

fn ke_transport(transport: &Transport) -> OwnedKeyExpr {
    if transport.is_multicast {
        KE_TRANSPORT_MULTICAST / &transport.zid.into_keyexpr()
    } else {
        KE_TRANSPORT_UNICAST / &transport.zid.into_keyexpr()
    }
}

fn ke_link(link: &Link) -> OwnedKeyExpr {
    let mut s = DefaultHasher::new();
    link.hash(&mut s);
    let link_hash = s.finish().to_string();
    // link_hash is number, no disallowed characters, it is safe to use unchecked
    let link_key = unsafe { keyexpr::from_str_unchecked(&link_hash) };
    KE_LINK / link_key
}

pub(crate) fn init(session: WeakSession) {
    // Normal adminspace queryable
    let own_zid = session.zid().into_keyexpr();
    let prefix = KE_AT / &own_zid / KE_SESSION;
    let _admin_qabl = session.declare_queryable_inner(
        &KeyExpr::from(&prefix / KE_STARSTAR),
        true,
        Locality::SessionLocal,
        Callback::from({
            let session = session.clone();
            move |q| on_admin_query(&session, &prefix, &prefix, q)
        }),
    );

    // Queryable simulating advanced publisher to allow advanced subscriber to receive historical data
    let prefix = KE_AT / &own_zid / KE_SESSION;
    let adv_prefix = KE_ADV_PREFIX
        / KE_PUB
        / &own_zid
        / KE_EMPTY
        / KE_EMPTY
        / KE_AT
        / KE_AT
        / &own_zid
        / KE_SESSION;

    let _admin_adv_qabl = session.declare_queryable_inner(
        &KeyExpr::from(&adv_prefix / KE_STARSTAR),
        true,
        Locality::SessionLocal,
        Callback::from({
            let session = session.clone();
            move |q| on_admin_query(&session, &adv_prefix, &prefix, q)
        }),
    );

    // Subscribe to transport events
}

fn reply<T: serde::Serialize>(
    match_prefix: &keyexpr,
    reply_prefix: &keyexpr,
    key: &keyexpr,
    query: &Query,
    payload: &T,
) {
    let match_keyexpr = match_prefix / key;
    if query.key_expr().intersects(&match_keyexpr) {
        let reply_keyexpr = reply_prefix / key;
        match serde_json::to_vec(payload) {
            Ok(bytes) => {
                let _ = query
                    .reply(reply_keyexpr, bytes)
                    .encoding(Encoding::APPLICATION_JSON)
                    .wait();
            }
            Err(e) => tracing::debug!("Admin query error: {}", e),
        }
    }
}

pub(crate) fn on_admin_query(
    session: &WeakSession,
    match_prefix: &keyexpr,
    reply_prefix: &keyexpr,
    query: Query,
) {
    for transport in session.runtime.get_transports() {
        let ke_transport = ke_transport(&transport);
        reply(
            match_prefix,
            reply_prefix,
            &ke_transport,
            &query,
            &transport,
        );
        for link in session.runtime.get_links(Some(&transport)) {
            let ke_link = &ke_transport / &ke_link(&link);
            reply(match_prefix, reply_prefix, &ke_link, &query, &link);
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
        self.new_peer(peer, false)
    }

    fn new_multicast(
        &self,
        _transport: zenoh_transport::multicast::TransportMulticast,
    ) -> ZResult<Arc<dyn zenoh_transport::TransportMulticastEventHandler>> {
        Ok(Arc::new(self.clone()))
    }
}

impl Handler {
    fn new_peer(
        &self,
        peer: zenoh_transport::TransportPeer,
        multicast: bool,
    ) -> ZResult<Arc<dyn TransportPeerEventHandler>> {
        let transport = if multicast {
            KE_TRANSPORT_MULTICAST
        } else {
            KE_TRANSPORT_UNICAST
        };
        if let Ok(own_zid) = keyexpr::new(&self.session.zid().to_string()) {
            if let Ok(zid) = keyexpr::new(&peer.zid.to_string()) {
                let wire_expr =
                    WireExpr::from(&(KE_AT / own_zid / KE_SESSION / transport / zid)).to_owned();
                let mut body = None;
                self.session.execute_subscriber_callbacks(
                    true,
                    SubscriberKind::Subscriber,
                    &wire_expr,
                    Default::default(),
                    || {
                        body.insert(PushBody::from(Put {
                            encoding: Encoding::APPLICATION_JSON.into(),
                            payload: serde_json::to_vec(&peer).unwrap().into(),
                            ..Put::default()
                        }))
                    },
                    false,
                    #[cfg(feature = "unstable")]
                    Reliability::Reliable,
                );
                Ok(Arc::new(PeerHandler {
                    expr: wire_expr,
                    session: self.session.clone(),
                }))
            } else {
                bail!("Unable to build keyexpr from zid")
            }
        } else {
            bail!("Unable to build keyexpr from zid")
        }
    }
}

impl TransportMulticastEventHandler for Handler {
    fn new_peer(
        &self,
        peer: zenoh_transport::TransportPeer,
    ) -> ZResult<Arc<dyn TransportPeerEventHandler>> {
        self.new_peer(peer, true)
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

impl PeerHandler {
    fn push(
        &self,
        kind: SampleKind,
        suffix: &str,
        encoding: Option<Encoding>,
        payload: Option<ZBytes>,
    ) {
        let mut body = None;
        let msg = || match kind {
            SampleKind::Put => body.insert(PushBody::Put(Put {
                encoding: encoding.unwrap_or_default().into(),
                payload: payload.unwrap_or_default().into(),
                ..Put::default()
            })),
            SampleKind::Delete => body.insert(PushBody::Del(Del::default())),
        };
        self.session.execute_subscriber_callbacks(
            true,
            SubscriberKind::Subscriber,
            &self.expr.clone().with_suffix(suffix),
            Default::default(),
            msg,
            false,
            #[cfg(feature = "unstable")]
            Reliability::Reliable,
        );
    }
}

impl TransportPeerEventHandler for PeerHandler {
    fn handle_message(&self, _msg: NetworkMessageMut) -> ZResult<()> {
        Ok(())
    }

    fn new_link(&self, link: zenoh_link::Link) {
        let mut s = DefaultHasher::new();
        link.hash(&mut s);
        self.push(
            SampleKind::Put,
            &format!("/link/{}", s.finish()),
            Some(Encoding::APPLICATION_JSON),
            Some(serde_json::to_vec(&link).unwrap().into()),
        );
    }

    fn del_link(&self, link: zenoh_link::Link) {
        let mut s = DefaultHasher::new();
        link.hash(&mut s);
        self.push(
            SampleKind::Delete,
            &format!("/link/{}", s.finish()),
            None,
            None,
        );
    }

    fn closed(&self) {
        self.push(SampleKind::Delete, "", None, None);
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
