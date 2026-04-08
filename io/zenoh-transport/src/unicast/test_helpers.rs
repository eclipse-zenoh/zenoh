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
use std::{fmt::DebugStruct, sync::Arc, time::Duration};

use async_trait::async_trait;
use tokio::sync::MutexGuard as AsyncMutexGuard;
use zenoh_core::zcondfeat;
use zenoh_link::Link;
use zenoh_protocol::{
    core::{Bound, RegionName, WhatAmI, ZenohIdProto},
    network::{NetworkBody, NetworkBodyMut, NetworkMessage, NetworkMessageMut},
    transport::TransportSn,
};
use zenoh_result::ZResult;

use crate::{
    unicast::{
        authentication::TransportAuthId,
        link::LinkUnicastWithOpenAck,
        transport_unicast_inner::{AddLinkResult, TransportStatus, TransportUnicastTrait},
        TransportConfigUnicast, TransportManagerBuilderUnicast, TransportUnicast,
    },
    TransportManager, TransportPeerEventHandler,
};

/// A minimal stub implementation of `TransportUnicastTrait` for use in routing tests.
///
/// All methods panic with `unimplemented!()` except `get_zid`, `get_whatami`, and `schedule`.
/// `schedule` converts each incoming [`NetworkMessageMut`] to an owned [`NetworkMessage`] and
/// calls the `on_schedule` callback provided at construction time, allowing tests to observe
/// every message that the routing layer sends to this face without a real network connection.
pub struct MockTransportUnicastInner {
    zid: ZenohIdProto,
    whatami: WhatAmI,
    on_schedule: Arc<dyn Fn(NetworkMessage) + Send + Sync>,
}

#[async_trait]
impl TransportUnicastTrait for MockTransportUnicastInner {
    fn set_callback(&self, _callback: Arc<dyn TransportPeerEventHandler>) {}

    async fn get_status(&self) -> AsyncMutexGuard<'_, TransportStatus> {
        unimplemented!("MockTransportUnicastInner::get_status")
    }

    fn get_zid(&self) -> ZenohIdProto {
        self.zid
    }

    fn get_whatami(&self) -> WhatAmI {
        self.whatami
    }

    fn get_callback(&self) -> Option<Arc<dyn TransportPeerEventHandler>> {
        unimplemented!("MockTransportUnicastInner::get_callback")
    }

    fn get_links(&self) -> Vec<Link> {
        vec![]
    }

    fn get_auth_ids(&self) -> TransportAuthId {
        unimplemented!("MockTransportUnicastInner::get_auth_ids")
    }

    #[cfg(feature = "shared-memory")]
    fn is_shm(&self) -> bool {
        false
    }

    fn is_qos(&self) -> bool {
        false
    }

    fn region_name(&self) -> Option<RegionName> {
        None
    }

    fn get_bound(&self) -> Option<Bound> {
        None
    }

    fn get_config(&self) -> &TransportConfigUnicast {
        unimplemented!("MockTransportUnicastInner::get_config")
    }

    #[cfg(feature = "stats")]
    fn stats(&self) -> zenoh_stats::TransportStats {
        unimplemented!("MockTransportUnicastInner::stats")
    }

    async fn add_link(
        &self,
        _link: LinkUnicastWithOpenAck,
        _other_initial_sn: TransportSn,
        _other_lease: Duration,
    ) -> AddLinkResult {
        unimplemented!("MockTransportUnicastInner::add_link")
    }

    fn schedule(&self, msg: NetworkMessageMut) -> ZResult<bool> {
        let body = match msg.body {
            NetworkBodyMut::Push(p) => NetworkBody::Push(p.clone()),
            NetworkBodyMut::Request(r) => NetworkBody::Request(r.clone()),
            NetworkBodyMut::Response(r) => NetworkBody::Response(r.clone()),
            NetworkBodyMut::ResponseFinal(r) => NetworkBody::ResponseFinal(r.clone()),
            NetworkBodyMut::Interest(i) => NetworkBody::Interest(i.clone()),
            NetworkBodyMut::Declare(d) => NetworkBody::Declare(d.clone()),
            NetworkBodyMut::OAM(o) => NetworkBody::OAM(o.clone()),
        };
        (self.on_schedule)(NetworkMessage {
            body,
            reliability: msg.reliability,
        });
        Ok(true)
    }

    async fn close(&self, _reason: u8) -> ZResult<()> {
        Ok(())
    }

    fn add_debug_fields<'a, 'b: 'a, 'c>(
        &self,
        s: &'c mut DebugStruct<'a, 'b>,
    ) -> &'c mut DebugStruct<'a, 'b> {
        s
    }
}

/// Creates a [`TransportUnicast`] backed by a [`MockTransportUnicastInner`].
///
/// Every message that the routing layer sends to this face is converted to an owned
/// [`NetworkMessage`] and forwarded to `on_schedule`.  The caller can use this callback to
/// record or assert on outgoing messages without a real network connection.
///
/// The caller must keep the returned `Arc<MockTransportUnicastInner>` alive for the duration of
/// any operation that uses the `TransportUnicast`, because `TransportUnicast` only holds a `Weak`
/// reference.
pub fn mock_transport_unicast(
    zid: ZenohIdProto,
    whatami: WhatAmI,
    on_schedule: Arc<dyn Fn(NetworkMessage) + Send + Sync>,
) -> (TransportUnicast, Arc<MockTransportUnicastInner>) {
    let inner = Arc::new(MockTransportUnicastInner {
        zid,
        whatami,
        on_schedule,
    });
    let erased: Arc<dyn TransportUnicastTrait> = inner.clone();
    let transport = TransportUnicast::from(&erased);
    (transport, inner)
}

pub fn make_transport_manager_builder(
    #[cfg(feature = "transport_multilink")] max_links: usize,
    lowlatency_transport: bool,
) -> TransportManagerBuilderUnicast {
    let transport = make_basic_transport_manager_builder(lowlatency_transport);

    zcondfeat!(
        "transport_multilink",
        {
            println!("...with max links: {max_links}...");
            transport.max_links(max_links)
        },
        transport
    )
}

pub fn make_basic_transport_manager_builder(
    lowlatency_transport: bool,
) -> TransportManagerBuilderUnicast {
    println!("Create transport manager builder...");
    let config = TransportManager::config_unicast();
    if lowlatency_transport {
        println!("...with LowLatency transport...");
    }
    match lowlatency_transport {
        true => config.lowlatency(true).qos(false),
        false => config,
    }
}
