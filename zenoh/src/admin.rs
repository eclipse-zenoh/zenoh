use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    sync::Arc,
};

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
    prelude::sync::{KeyExpr, Locality},
    queryable::Query,
    Sample, Session, ZResult,
};
use zenoh_core::SyncResolve;
use zenoh_protocol::{
    core::{Encoding, KnownEncoding, SampleKind, WireExpr},
    zenoh::{DataInfo, ZenohMessage},
};
use zenoh_transport::{TransportEventHandler, TransportPeerEventHandler};

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
    if let Ok(own_zid) = keyexpr::new(&session.zid().to_string()) {
        for transport in session.runtime.manager().get_transports() {
            if let Ok(zid) = transport.get_zid().map(|zid| zid.to_string()) {
                if let Ok(zid) = keyexpr::new(&zid) {
                    let key_expr = *KE_PREFIX / own_zid / *KE_TRANSPORT_UNICAST / zid;
                    if query.key_expr().intersects(&key_expr) {
                        if let Ok(value) = transport
                            .get_peer()
                            .map_err(|_| ())
                            .and_then(|p| serde_json::value::to_value(p).map_err(|_| ()))
                        {
                            let _ = query.reply(Ok(Sample::new(key_expr, value))).res_sync();
                        }
                    }

                    for link in transport.get_links().unwrap_or_else(|_| vec![]) {
                        let mut s = DefaultHasher::new();
                        link.hash(&mut s);
                        if let Ok(lid) = keyexpr::new(&s.finish().to_string()) {
                            let key_expr =
                                *KE_PREFIX / own_zid / *KE_TRANSPORT_UNICAST / zid / *KE_LINK / lid;
                            if query.key_expr().intersects(&key_expr) {
                                if let Ok(value) = serde_json::value::to_value(link) {
                                    let _ =
                                        query.reply(Ok(Sample::new(key_expr, value))).res_sync();
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

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
        _transport: zenoh_transport::TransportUnicast,
    ) -> ZResult<Arc<dyn zenoh_transport::TransportPeerEventHandler>> {
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

    fn new_multicast(
        &self,
        _transport: zenoh_transport::TransportMulticast,
    ) -> ZResult<Arc<dyn zenoh_transport::TransportMulticastEventHandler>> {
        bail!("unimplemented")
    }
}

pub(crate) struct PeerHandler {
    pub(crate) expr: WireExpr<'static>,
    pub(crate) session: Arc<Session>,
}

impl TransportPeerEventHandler for PeerHandler {
    fn handle_message(&self, _msg: ZenohMessage) -> ZResult<()> {
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
        );
    }

    fn closing(&self) {}

    fn closed(&self) {
        let info = DataInfo {
            kind: SampleKind::Delete,
            ..Default::default()
        };
        self.session
            .handle_data(true, &self.expr, Some(info), vec![0u8; 0].into());
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
