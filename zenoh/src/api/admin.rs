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
};

use zenoh_config::{WhatAmI, ZenohId};
use zenoh_core::Wait;
use zenoh_keyexpr::{keyexpr, OwnedKeyExpr};
use zenoh_link::Locator;
use zenoh_macros::ke;
use zenoh_protocol::core::{CongestionControl, Reliability};

use crate::{
    self as zenoh,
    api::{
        encoding::Encoding,
        info::{Link, LinkEvent, Transport, TransportEvent},
        key_expr::KeyExpr,
        publisher::Priority,
        queryable::Query,
        sample::{Locality, SampleKind},
        session::WeakSession,
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
static KE_TRANSPORT_MULTICAST: &keyexpr = ke!("transport/multicast");
static KE_LINK: &keyexpr = ke!("link");

// JSON structures for making adminspace independent of internal structures
#[derive(serde::Serialize)]
struct TransportJson {
    zid: ZenohId,
    whatami: WhatAmI,
    is_qos: bool,
    #[cfg(feature = "shared-memory")]
    is_shm: bool,
}

impl From<Transport> for TransportJson {
    fn from(transport: Transport) -> Self {
        Self {
            zid: transport.zid,
            whatami: transport.whatami,
            is_qos: transport.is_qos,
            #[cfg(feature = "shared-memory")]
            is_shm: transport.is_shm,
        }
    }
}

#[derive(serde::Serialize)]
struct PrioritiesJson {
    start: zenoh_protocol::core::Priority,
    end: zenoh_protocol::core::Priority,
}

impl From<(u8, u8)> for PrioritiesJson {
    fn from(priorities: (u8, u8)) -> Self {
        Self {
            start: priorities.0.try_into().unwrap_or_default(),
            end: priorities.1.try_into().unwrap_or_default(),
        }
    }
}

#[derive(serde::Serialize)]
struct LinkJson {
    src: Locator,
    dst: Locator,
    group: Option<Locator>,
    mtu: u16,
    is_streamed: bool,
    interfaces: Vec<String>,
    auth_identifier: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    priorities: Option<PrioritiesJson>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reliability: Option<Reliability>,
}

impl From<Link> for LinkJson {
    fn from(link: Link) -> Self {
        Self {
            src: link.src,
            dst: link.dst,
            group: link.group,
            mtu: link.mtu,
            is_streamed: link.is_streamed,
            interfaces: link.interfaces,
            auth_identifier: link.auth_identifier,
            priorities: link.priorities.map(PrioritiesJson::from),
            reliability: link.reliability,
        }
    }
}

fn ke_prefix(own_zid: &keyexpr) -> OwnedKeyExpr {
    KE_AT / own_zid / KE_SESSION
}

fn ke_adv_prefix(own_zid: &keyexpr) -> OwnedKeyExpr {
    KE_ADV_PREFIX / KE_PUB / own_zid / KE_EMPTY / KE_EMPTY / KE_AT / KE_AT / own_zid / KE_SESSION
}

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
    let prefix = ke_prefix(&own_zid);
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
    let prefix = ke_prefix(&own_zid);
    let adv_prefix = ke_adv_prefix(&own_zid);
    let _admin_adv_qabl = session.declare_queryable_inner(
        &KeyExpr::from(&adv_prefix / KE_STARSTAR),
        true,
        Locality::SessionLocal,
        Callback::from({
            let session = session.clone();
            move |q| on_admin_query(&session, &adv_prefix, &prefix, q)
        }),
    );

    // Subscribe to transport events and publish them to the adminspace
    // key "@/<own_zid>/session/transport/<unicast|multicast>/<peer_zid>"
    let callback = Callback::from({
        let session = session.clone();
        let own_zid = session.zid().into_keyexpr();
        move |event: TransportEvent| {
            let key_expr = ke_prefix(&own_zid) / &ke_transport(&event.transport);
            let key_expr = KeyExpr::from(key_expr);
            tracing::info!(
                "Publishing transport event: {:?} : {:?} on {}",
                &event.kind,
                &event.transport,
                key_expr
            );
            let payload = match &event.kind {
                SampleKind::Put => {
                    let json = TransportJson::from(event.transport);
                    serde_json::to_vec(&json).unwrap_or_default()
                }
                SampleKind::Delete => Vec::new(),
            };
            if let Err(e) = session.resolve_put(
                &key_expr,
                payload.into(),
                event.kind,
                Encoding::APPLICATION_JSON,
                CongestionControl::default(),
                Priority::default(),
                false,
                Locality::SessionLocal,
                #[cfg(feature = "unstable")]
                Reliability::default(),
                None,
                #[cfg(feature = "unstable")]
                None,
                None,
            ) {
                tracing::error!("Unable to publish transport event: {}", e);
            }
        }
    });
    if let Err(e) = session.declare_transport_events_listener_inner(callback, false) {
        tracing::error!("Unable to subscribe to transport events: {}", e);
    }

    // Subscribe to link events and publish them to the adminspace
    // key "@/<own_zid>/session/transport/<unicast|multicast>/<peer_zid>/link/<link_hash>"
    let callback = Callback::from({
        let session = session.clone();
        let own_zid = session.zid().into_keyexpr();
        move |event: LinkEvent| {
            // Find the transport to determine if it's multicast
            let transport_zid = &event.link.zid;
            let transport = session
                .runtime()
                .get_transports()
                .find(|t| t.zid == *transport_zid);

            if let Some(transport) = transport {
                let key_expr =
                    ke_prefix(&own_zid) / &ke_transport(&transport) / &ke_link(&event.link);
                let key_expr = KeyExpr::from(key_expr);
                tracing::info!(
                    "Publishing link event: {:?} : {:?} on {}",
                    &event.kind,
                    &event.link,
                    key_expr
                );
                let payload = match &event.kind {
                    SampleKind::Put => {
                        serde_json::to_vec(&LinkJson::from(event.link)).unwrap_or_default()
                    }
                    SampleKind::Delete => Vec::new(),
                };
                if let Err(e) = session.resolve_put(
                    &key_expr,
                    payload.into(),
                    event.kind,
                    Encoding::APPLICATION_JSON,
                    CongestionControl::default(),
                    Priority::default(),
                    false,
                    Locality::SessionLocal,
                    #[cfg(feature = "unstable")]
                    Reliability::default(),
                    None,
                    #[cfg(feature = "unstable")]
                    None,
                    None,
                ) {
                    tracing::error!("Unable to publish link event: {}", e);
                }
            } else {
                tracing::warn!("Unable to find transport for link event: {}", transport_zid);
            }
        }
    });
    if let Err(e) = session.declare_transport_links_listener_inner(callback, false, None) {
        tracing::error!("Unable to subscribe to link events: {}", e);
    }
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
    for transport in session.runtime().get_transports() {
        let ke_transport = ke_transport(&transport);
        let transport_json = TransportJson::from(transport.clone());
        reply(
            match_prefix,
            reply_prefix,
            &ke_transport,
            &query,
            &transport_json,
        );
        for link in session.runtime().get_links(Some(&transport)) {
            let ke_link = &ke_transport / &ke_link(&link);
            let link_json = LinkJson::from(link);
            reply(match_prefix, reply_prefix, &ke_link, &query, &link_json);
        }
    }
}
