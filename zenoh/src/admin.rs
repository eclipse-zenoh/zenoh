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
use crate::{
    keyexpr,
    prelude::sync::{KeyExpr, Locality, SampleKind},
    queryable::Query,
    sample::DataInfo,
    Sample, Session, ZResult,
};
use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    sync::Arc,
};
use zenoh_core::SyncResolve;
use zenoh_protocol::{
    core::{Encoding, KnownEncoding, WireExpr},
    network::NetworkMessage,
};
use zenoh_transport::{
    TransportEventHandler, TransportMulticastEventHandler, TransportPeer, TransportPeerEventHandler,
};

macro_rules! ke_for_sure {
    ($val:expr) => {
        unsafe { keyexpr::from_str_unchecked($val) }
    };
}

lazy_static::lazy_static!(
    static ref KE_STARSTAR: &'static keyexpr = ke_for_sure!("**");
    static ref KE_PREFIX: &'static keyexpr = ke_for_sure!("@/session");
    static ref KE_TRANSPORT_UNICAST: &'static keyexpr = ke_for_sure!("transport/unicast");
    static ref KE_LINK: &'static keyexpr = ke_for_sure!("link");
);

pub(crate) fn init(session: &Session) {
    if let Ok(own_zid) = keyexpr::new(&session.zid().to_string()) {
        let admin_key = KeyExpr::from(*KE_PREFIX / own_zid / *KE_STARSTAR)
            .to_wire(session)
            .to_owned();

        let _admin_qabl = session.declare_queryable_inner(
            &admin_key,
            true,
            Locality::SessionLocal,
            Arc::new({
                let session = session.clone();
                move |q| super::admin::on_admin_query(&session, q)
            }),
        );
    }
}

pub(crate) fn on_admin_query(session: &Session, query: Query) {
    fn reply_peer(own_zid: &keyexpr, query: &Query, peer: TransportPeer) {
        let zid = peer.zid.to_string();
        if let Ok(zid) = keyexpr::new(&zid) {
            let key_expr = *KE_PREFIX / own_zid / *KE_TRANSPORT_UNICAST / zid;
            if query.key_expr().intersects(&key_expr) {
                if let Ok(value) = serde_json::value::to_value(peer.clone()) {
                    let _ = query.reply(Ok(Sample::new(key_expr, value))).res_sync();
                }
            }

            for link in peer.links {
                let mut s = DefaultHasher::new();
                link.hash(&mut s);
                if let Ok(lid) = keyexpr::new(&s.finish().to_string()) {
                    let key_expr =
                        *KE_PREFIX / own_zid / *KE_TRANSPORT_UNICAST / zid / *KE_LINK / lid;
                    if query.key_expr().intersects(&key_expr) {
                        if let Ok(value) = serde_json::value::to_value(link) {
                            let _ = query.reply(Ok(Sample::new(key_expr, value))).res_sync();
                        }
                    }
                }
            }
        }
    }

    if let Ok(own_zid) = keyexpr::new(&session.zid().to_string()) {
        for transport in tokio::task::block_in_place(|| {
            zenoh_runtime::ZRuntime::Net
                .block_on(session.runtime.manager().get_transports_unicast())
        }) {
            if let Ok(peer) = transport.get_peer() {
                reply_peer(own_zid, &query, peer);
            }
        }
        for transport in tokio::task::block_in_place(|| {
            zenoh_runtime::ZRuntime::Net
                .block_on(session.runtime.manager().get_transports_multicast())
        }) {
            for peer in transport.get_peers().unwrap_or_default() {
                reply_peer(own_zid, &query, peer);
            }
        }
    }
}

#[derive(Clone)]
pub(crate) struct Handler {
    pub(crate) session: Arc<Session>,
}

impl Handler {
    pub(crate) fn new(session: Session) -> Self {
        Self {
            session: Arc::new(session),
        }
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
                let expr = WireExpr::from(&(*KE_PREFIX / own_zid / *KE_TRANSPORT_UNICAST / zid))
                    .to_owned();
                let info = DataInfo {
                    encoding: Some(Encoding::Exact(KnownEncoding::AppJson)),
                    ..Default::default()
                };
                self.session.handle_data(
                    true,
                    &expr,
                    Some(info),
                    serde_json::to_vec(&peer).unwrap().into(),
                    #[cfg(feature = "unstable")]
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

    fn closing(&self) {}

    fn closed(&self) {}

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

pub(crate) struct PeerHandler {
    pub(crate) expr: WireExpr<'static>,
    pub(crate) session: Arc<Session>,
}

impl TransportPeerEventHandler for PeerHandler {
    fn handle_message(&self, _msg: NetworkMessage) -> ZResult<()> {
        Ok(())
    }

    fn new_link(&self, link: zenoh_link::Link) {
        let mut s = DefaultHasher::new();
        link.hash(&mut s);
        let info = DataInfo {
            encoding: Some(Encoding::Exact(KnownEncoding::AppJson)),
            ..Default::default()
        };
        self.session.handle_data(
            true,
            &self
                .expr
                .clone()
                .with_suffix(&format!("/link/{}", s.finish())),
            Some(info),
            serde_json::to_vec(&link).unwrap().into(),
            #[cfg(feature = "unstable")]
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
        self.session.handle_data(
            true,
            &self
                .expr
                .clone()
                .with_suffix(&format!("/link/{}", s.finish())),
            Some(info),
            vec![0u8; 0].into(),
            #[cfg(feature = "unstable")]
            None,
        );
    }

    fn closing(&self) {}

    fn closed(&self) {
        let info = DataInfo {
            kind: SampleKind::Delete,
            ..Default::default()
        };
        self.session.handle_data(
            true,
            &self.expr,
            Some(info),
            vec![0u8; 0].into(),
            #[cfg(feature = "unstable")]
            None,
        );
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
