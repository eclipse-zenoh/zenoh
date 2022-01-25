//
// Copyright (c) 2017, 2020 ADLINK Technology Inc.
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ADLINK zenoh team, <zenoh@adlink-labs.tech>
//
use async_std::prelude::*;
use async_std::sync::Arc;
use async_std::task;
#[cfg(feature = "auth_pubkey")]
use rsa::{BigUint, RsaPrivateKey, RsaPublicKey};
use std::any::Any;
#[cfg(feature = "auth_usrpwd")]
use std::collections::HashMap;
use std::collections::HashSet;
use std::iter::FromIterator;
use std::time::Duration;
use zenoh::net::link::{EndPoint, Link};
use zenoh::net::protocol::core::{WhatAmI, ZenohId};
use zenoh::net::protocol::message::ZenohMessage;
#[cfg(feature = "auth_pubkey")]
use zenoh::net::transport::unicast::establishment::authenticator::PubKeyAuthenticator;
#[cfg(feature = "shared-memory")]
use zenoh::net::transport::unicast::establishment::authenticator::SharedMemoryAuthenticator;
#[cfg(feature = "auth_usrpwd")]
use zenoh::net::transport::unicast::establishment::authenticator::UserPasswordAuthenticator;
use zenoh::net::transport::{
    DummyTransportPeerEventHandler, TransportEventHandler, TransportManager, TransportMulticast,
    TransportMulticastEventHandler, TransportPeer, TransportPeerEventHandler, TransportUnicast,
};
use zenoh_util::core::Result as ZResult;
use zenoh_util::properties::Properties;
use zenoh_util::zasync_executor_init;

const TIMEOUT: Duration = Duration::from_secs(60);
const SLEEP: Duration = Duration::from_millis(100);

macro_rules! ztimeout {
    ($f:expr) => {
        $f.timeout(TIMEOUT).await.unwrap()
    };
}

#[cfg(test)]
struct SHRouterAuthenticator;

impl SHRouterAuthenticator {
    fn new() -> Self {
        Self
    }
}

impl TransportEventHandler for SHRouterAuthenticator {
    fn new_unicast(
        &self,
        _peer: TransportPeer,
        _transport: TransportUnicast,
    ) -> ZResult<Arc<dyn TransportPeerEventHandler>> {
        Ok(Arc::new(MHRouterAuthenticator::new()))
    }

    fn new_multicast(
        &self,
        _transport: TransportMulticast,
    ) -> ZResult<Arc<dyn TransportMulticastEventHandler>> {
        panic!();
    }
}

struct MHRouterAuthenticator;

impl MHRouterAuthenticator {
    fn new() -> Self {
        Self
    }
}

impl TransportPeerEventHandler for MHRouterAuthenticator {
    fn handle_message(&self, _msg: ZenohMessage) -> ZResult<()> {
        Ok(())
    }
    fn new_link(&self, _link: Link) {}
    fn del_link(&self, _link: Link) {}
    fn closing(&self) {}
    fn closed(&self) {}

    fn as_any(&self) -> &dyn Any {
        self
    }
}

// Transport Handler for the client
#[derive(Default)]
struct SHClientAuthenticator;

impl TransportEventHandler for SHClientAuthenticator {
    fn new_unicast(
        &self,
        _peer: TransportPeer,
        _transport: TransportUnicast,
    ) -> ZResult<Arc<dyn TransportPeerEventHandler>> {
        Ok(Arc::new(DummyTransportPeerEventHandler::default()))
    }

    fn new_multicast(
        &self,
        _transport: TransportMulticast,
    ) -> ZResult<Arc<dyn TransportMulticastEventHandler>> {
        panic!();
    }
}

#[cfg(feature = "auth_pubkey")]
async fn authenticator_multilink(endpoint: &EndPoint) {
    // Create the router transport manager
    let router_id = ZenohId::new(1, [0u8; ZenohId::MAX_SIZE]);
    let router_handler = Arc::new(SHRouterAuthenticator::new());
    let n = BigUint::from_bytes_le(&[
        0x31, 0xd1, 0xfc, 0x7e, 0x70, 0x5f, 0xd7, 0xe3, 0xcc, 0xa4, 0xca, 0xcb, 0x38, 0x84, 0x2f,
        0xf5, 0x88, 0xaa, 0x4b, 0xbc, 0x2f, 0x74, 0x59, 0x49, 0xa9, 0xb9, 0x1a, 0x4d, 0x1c, 0x93,
        0xbc, 0xc7, 0x02, 0xd0, 0xe0, 0x0f, 0xa7, 0x68, 0xeb, 0xef, 0x9b, 0xf9, 0x4f, 0xdc, 0xe3,
        0x40, 0x5a, 0x3c, 0x8f, 0x20, 0xf4, 0x2c, 0x90, 0x1c, 0x70, 0x56, 0x9b, 0xae, 0x44, 0x17,
        0xca, 0x85, 0x60, 0xb5,
    ]);
    let e = BigUint::from_bytes_le(&[0x01, 0x00, 0x01]);
    let router_pub_key = RsaPublicKey::new(n, e).unwrap();

    let n = BigUint::from_bytes_le(&[
        0x31, 0xd1, 0xfc, 0x7e, 0x70, 0x5f, 0xd7, 0xe3, 0xcc, 0xa4, 0xca, 0xcb, 0x38, 0x84, 0x2f,
        0xf5, 0x88, 0xaa, 0x4b, 0xbc, 0x2f, 0x74, 0x59, 0x49, 0xa9, 0xb9, 0x1a, 0x4d, 0x1c, 0x93,
        0xbc, 0xc7, 0x02, 0xd0, 0xe0, 0x0f, 0xa7, 0x68, 0xeb, 0xef, 0x9b, 0xf9, 0x4f, 0xdc, 0xe3,
        0x40, 0x5a, 0x3c, 0x8f, 0x20, 0xf4, 0x2c, 0x90, 0x1c, 0x70, 0x56, 0x9b, 0xae, 0x44, 0x17,
        0xca, 0x85, 0x60, 0xb5,
    ]);
    let e = BigUint::from_bytes_le(&[0x01, 0x00, 0x01]);
    let d = BigUint::from_bytes_le(&[
        0xc1, 0xd1, 0xc1, 0x0f, 0xbe, 0xa7, 0xe6, 0x18, 0x98, 0x3c, 0xf8, 0x26, 0x74, 0xc0, 0xc7,
        0xef, 0xf9, 0x38, 0x95, 0x75, 0x40, 0x45, 0xd4, 0x0d, 0x27, 0xec, 0x4c, 0xcd, 0x81, 0xf9,
        0xf4, 0x69, 0x36, 0x99, 0x95, 0x97, 0xd0, 0xc8, 0x43, 0xac, 0xbb, 0x3e, 0x8f, 0xfb, 0x97,
        0x53, 0xdb, 0x92, 0x12, 0xc5, 0xc0, 0x50, 0x83, 0xb2, 0x04, 0x25, 0x79, 0xeb, 0xa7, 0x32,
        0x84, 0xbb, 0xc6, 0x35,
    ]);
    let primes = vec![
        BigUint::from_bytes_le(&[
            0xb9, 0x17, 0xd3, 0x45, 0x0a, 0x8e, 0xf7, 0x41, 0xaf, 0x75, 0xe3, 0x7f, 0xe9, 0x3c,
            0x10, 0x28, 0x24, 0x0a, 0x95, 0x32, 0xc0, 0xcb, 0x23, 0x60, 0x6e, 0x2d, 0xb8, 0x2e,
            0x96, 0x78, 0x21, 0xdf,
        ]),
        BigUint::from_bytes_le(&[
            0x39, 0x51, 0xd3, 0xf3, 0xfe, 0xd1, 0x81, 0xd3, 0xc3, 0x2b, 0x49, 0x65, 0x3a, 0x44,
            0x41, 0x31, 0xa7, 0x38, 0x8b, 0xd9, 0x18, 0xc7, 0x41, 0x8c, 0x86, 0x0b, 0x65, 0x2d,
            0x18, 0x78, 0x18, 0xd0,
        ]),
    ];
    let router_pri_key = RsaPrivateKey::from_components(n, e, d, primes);
    let peer_auth_router = Arc::new(PubKeyAuthenticator::new(router_pub_key, router_pri_key));
    let unicast = TransportManager::config_unicast()
        .max_links(2)
        .peer_authenticator(HashSet::from_iter(vec![peer_auth_router.clone().into()]));
    let router_manager = TransportManager::builder()
        .whatami(WhatAmI::Router)
        .zid(router_id)
        .unicast(unicast)
        .build(router_handler.clone())
        .unwrap();

    // Create the transport transport manager for the client 01
    let client01_id = ZenohId::new(1, [1_u8; ZenohId::MAX_SIZE]);

    let n = BigUint::from_bytes_le(&[
        0x41, 0x74, 0xc6, 0x40, 0x18, 0x63, 0xbd, 0x59, 0xe6, 0x0d, 0xe9, 0x23, 0x3e, 0x95, 0xca,
        0xb4, 0x5d, 0x17, 0x3d, 0x14, 0xdd, 0xbb, 0x16, 0x4a, 0x49, 0xeb, 0x43, 0x27, 0x79, 0x3e,
        0x75, 0x67, 0xd6, 0xf6, 0x7f, 0xe7, 0xbf, 0xb5, 0x1d, 0xf6, 0x27, 0x80, 0xca, 0x26, 0x35,
        0xa2, 0xc5, 0x4c, 0x96, 0x50, 0xaa, 0x9f, 0xf4, 0x47, 0xbe, 0x06, 0x9c, 0xd1, 0xec, 0xfd,
        0x1e, 0x81, 0xe9, 0xc4,
    ]);
    let e = BigUint::from_bytes_le(&[0x01, 0x00, 0x01]);
    let client01_pub_key = RsaPublicKey::new(n, e).unwrap();

    let n = BigUint::from_bytes_le(&[
        0x41, 0x74, 0xc6, 0x40, 0x18, 0x63, 0xbd, 0x59, 0xe6, 0x0d, 0xe9, 0x23, 0x3e, 0x95, 0xca,
        0xb4, 0x5d, 0x17, 0x3d, 0x14, 0xdd, 0xbb, 0x16, 0x4a, 0x49, 0xeb, 0x43, 0x27, 0x79, 0x3e,
        0x75, 0x67, 0xd6, 0xf6, 0x7f, 0xe7, 0xbf, 0xb5, 0x1d, 0xf6, 0x27, 0x80, 0xca, 0x26, 0x35,
        0xa2, 0xc5, 0x4c, 0x96, 0x50, 0xaa, 0x9f, 0xf4, 0x47, 0xbe, 0x06, 0x9c, 0xd1, 0xec, 0xfd,
        0x1e, 0x81, 0xe9, 0xc4,
    ]);
    let e = BigUint::from_bytes_le(&[0x01, 0x00, 0x01]);
    let d = BigUint::from_bytes_le(&[
        0x15, 0xe1, 0x93, 0xda, 0x75, 0xcb, 0x76, 0x40, 0xce, 0x70, 0x6f, 0x0f, 0x62, 0xe1, 0x58,
        0xa5, 0x53, 0x7b, 0x17, 0x63, 0x71, 0x70, 0x2d, 0x0d, 0xc5, 0xce, 0xcd, 0xb4, 0x26, 0xe0,
        0x22, 0x3d, 0xd4, 0x04, 0x88, 0x51, 0x24, 0x34, 0x01, 0x9c, 0x94, 0x01, 0x47, 0x49, 0x86,
        0xe3, 0x2f, 0x3b, 0x65, 0x5c, 0xcc, 0x0b, 0x8a, 0x00, 0x93, 0x26, 0x79, 0xbb, 0x18, 0xab,
        0x94, 0x4b, 0x52, 0x99,
    ]);
    let primes = vec![
        BigUint::from_bytes_le(&[
            0x87, 0x9c, 0xbd, 0x9c, 0xbf, 0xd5, 0xb7, 0xc2, 0x73, 0x16, 0x44, 0x3f, 0x67, 0x90,
            0xaa, 0xab, 0xfe, 0x20, 0xac, 0x7d, 0xe9, 0xc4, 0xb9, 0xfb, 0x12, 0xab, 0x09, 0x35,
            0xec, 0xf5, 0x9f, 0xe1,
        ]),
        BigUint::from_bytes_le(&[
            0xf7, 0xa2, 0xc1, 0x81, 0x63, 0xe1, 0x1c, 0x39, 0xe4, 0x7b, 0xbf, 0x56, 0xd5, 0x35,
            0xc3, 0xd1, 0x11, 0xf9, 0x1f, 0x42, 0x4e, 0x3e, 0xe7, 0xc9, 0xa2, 0x3c, 0x98, 0x08,
            0xaa, 0xf9, 0x6b, 0xdf,
        ]),
    ];
    let client01_pri_key = RsaPrivateKey::from_components(n, e, d, primes);
    let peer_auth_client01 = PubKeyAuthenticator::new(client01_pub_key, client01_pri_key);
    let unicast = TransportManager::config_unicast()
        .max_links(2)
        .peer_authenticator(HashSet::from_iter(vec![peer_auth_client01.into()]));
    let client01_manager = TransportManager::builder()
        .whatami(WhatAmI::Client)
        .zid(client01_id)
        .unicast(unicast)
        .build(Arc::new(SHClientAuthenticator::default()))
        .unwrap();

    // Create the transport transport manager for the client 02
    let client02_id = ZenohId::new(1, [2_u8; ZenohId::MAX_SIZE]);

    let n = BigUint::from_bytes_le(&[
        0xd1, 0x36, 0xcf, 0x94, 0xda, 0x04, 0x7e, 0x9f, 0x53, 0x39, 0xb8, 0x7b, 0x53, 0x3a, 0xe6,
        0xa4, 0x0e, 0x6c, 0xf0, 0x92, 0x5d, 0xd9, 0x1d, 0x84, 0xc3, 0x10, 0xab, 0x8f, 0x7d, 0xe8,
        0xf4, 0xff, 0x79, 0xae, 0x00, 0x25, 0xfc, 0xaf, 0x0c, 0x0f, 0x05, 0xc7, 0xa3, 0xfd, 0x31,
        0x9a, 0xd3, 0x79, 0x0f, 0x44, 0xa6, 0x1c, 0x19, 0x61, 0xed, 0xb0, 0x27, 0x99, 0x53, 0x23,
        0x50, 0xad, 0x67, 0xcf,
    ]);
    let e = BigUint::from_bytes_le(&[0x01, 0x00, 0x01]);
    let client02_pub_key = RsaPublicKey::new(n, e).unwrap();

    let n = BigUint::from_bytes_le(&[
        0xd1, 0x36, 0xcf, 0x94, 0xda, 0x04, 0x7e, 0x9f, 0x53, 0x39, 0xb8, 0x7b, 0x53, 0x3a, 0xe6,
        0xa4, 0x0e, 0x6c, 0xf0, 0x92, 0x5d, 0xd9, 0x1d, 0x84, 0xc3, 0x10, 0xab, 0x8f, 0x7d, 0xe8,
        0xf4, 0xff, 0x79, 0xae, 0x00, 0x25, 0xfc, 0xaf, 0x0c, 0x0f, 0x05, 0xc7, 0xa3, 0xfd, 0x31,
        0x9a, 0xd3, 0x79, 0x0f, 0x44, 0xa6, 0x1c, 0x19, 0x61, 0xed, 0xb0, 0x27, 0x99, 0x53, 0x23,
        0x50, 0xad, 0x67, 0xcf,
    ]);
    let e = BigUint::from_bytes_le(&[0x01, 0x00, 0x01]);
    let d = BigUint::from_bytes_le(&[
        0x01, 0xe4, 0xe9, 0x20, 0x20, 0x8c, 0x17, 0xd3, 0xea, 0xd0, 0x1f, 0xfa, 0x25, 0x5c, 0xaf,
        0x5d, 0x19, 0xa4, 0x2a, 0xbc, 0x62, 0x5e, 0x2c, 0x63, 0x4f, 0x6e, 0x30, 0x07, 0x7c, 0x04,
        0x72, 0xc9, 0x57, 0x3d, 0xe0, 0x59, 0x33, 0x8a, 0x36, 0x02, 0x5d, 0xa6, 0x81, 0x4e, 0x27,
        0x82, 0xce, 0x95, 0x85, 0xd4, 0xa3, 0x9b, 0x5e, 0x2a, 0x04, 0xa8, 0x9d, 0x74, 0x25, 0x70,
        0xf4, 0x37, 0x7d, 0x27,
    ]);
    let primes = vec![
        BigUint::from_bytes_le(&[
            0x31, 0x55, 0x19, 0x90, 0xf4, 0xb5, 0x76, 0xed, 0xa4, 0x2e, 0x52, 0x37, 0x16, 0xd5,
            0xef, 0x0b, 0xcb, 0x00, 0x10, 0xea, 0xff, 0x4f, 0xfe, 0x04, 0xf4, 0x44, 0xac, 0x24,
            0xfc, 0x68, 0x02, 0xe4,
        ]),
        BigUint::from_bytes_le(&[
            0xa1, 0x13, 0xee, 0xe0, 0xe2, 0x98, 0x4e, 0x0b, 0x90, 0x11, 0x73, 0x87, 0xa2, 0x54,
            0x8c, 0x5c, 0xe7, 0x03, 0x4b, 0xbf, 0x26, 0xfc, 0xb4, 0xba, 0xf9, 0xf9, 0x03, 0x84,
            0xb9, 0xbc, 0xdd, 0xe8,
        ]),
    ];
    let client02_pri_key = RsaPrivateKey::from_components(n, e, d, primes);

    let peer_auth_client02 = PubKeyAuthenticator::new(client02_pub_key, client02_pri_key);
    let unicast = TransportManager::config_unicast()
        .max_links(2)
        .peer_authenticator(HashSet::from_iter(vec![peer_auth_client02.into()]));
    let client02_manager = TransportManager::builder()
        .whatami(WhatAmI::Client)
        .zid(client02_id)
        .unicast(unicast)
        .build(Arc::new(SHClientAuthenticator::default()))
        .unwrap();

    // Create the transport transport manager for the third client
    let client01_spoof_id = client01_id;

    let n = BigUint::from_bytes_le(&[
        0x19, 0x01, 0x60, 0xf9, 0x1b, 0xd5, 0x2f, 0x1d, 0xb9, 0x16, 0xb5, 0xe4, 0xe3, 0x89, 0x7d,
        0x94, 0xa7, 0x82, 0x2f, 0xa7, 0xab, 0xec, 0xf2, 0xc2, 0x31, 0xf4, 0xf9, 0x66, 0x97, 0x6a,
        0x98, 0xf7, 0x02, 0x00, 0x0f, 0x51, 0xe4, 0xe4, 0x19, 0x29, 0xb6, 0x95, 0x1b, 0xb1, 0x2e,
        0x86, 0x2b, 0x99, 0x7f, 0x4e, 0x5a, 0x84, 0x44, 0xf3, 0xaa, 0xb3, 0xbf, 0x6c, 0xb5, 0xd4,
        0x05, 0x37, 0x33, 0xc2,
    ]);
    let e = BigUint::from_bytes_le(&[0x01, 0x00, 0x01]);
    let client01_spoof_pub_key = RsaPublicKey::new(n, e).unwrap();

    let n = BigUint::from_bytes_le(&[
        0x19, 0x01, 0x60, 0xf9, 0x1b, 0xd5, 0x2f, 0x1d, 0xb9, 0x16, 0xb5, 0xe4, 0xe3, 0x89, 0x7d,
        0x94, 0xa7, 0x82, 0x2f, 0xa7, 0xab, 0xec, 0xf2, 0xc2, 0x31, 0xf4, 0xf9, 0x66, 0x97, 0x6a,
        0x98, 0xf7, 0x02, 0x00, 0x0f, 0x51, 0xe4, 0xe4, 0x19, 0x29, 0xb6, 0x95, 0x1b, 0xb1, 0x2e,
        0x86, 0x2b, 0x99, 0x7f, 0x4e, 0x5a, 0x84, 0x44, 0xf3, 0xaa, 0xb3, 0xbf, 0x6c, 0xb5, 0xd4,
        0x05, 0x37, 0x33, 0xc2,
    ]);
    let e = BigUint::from_bytes_le(&[0x01, 0x00, 0x01]);
    let d = BigUint::from_bytes_le(&[
        0x01, 0xef, 0x77, 0x79, 0x99, 0xf7, 0xa7, 0x14, 0x0f, 0x61, 0xc6, 0xca, 0x3e, 0x14, 0xfa,
        0x52, 0x6a, 0xbc, 0x94, 0x2a, 0xb1, 0x7e, 0x3a, 0x57, 0xd1, 0x85, 0x62, 0xa4, 0xd0, 0xf5,
        0x40, 0x6a, 0x0f, 0xb3, 0x59, 0xec, 0x7d, 0xbd, 0xac, 0xcf, 0x28, 0x5c, 0xa1, 0x11, 0x40,
        0xfa, 0x84, 0x4a, 0x54, 0xd1, 0x7f, 0x44, 0xc8, 0xce, 0xb2, 0x21, 0x63, 0xd0, 0xc4, 0x02,
        0x55, 0x0a, 0x3a, 0x12,
    ]);
    let primes = vec![
        BigUint::from_bytes_le(&[
            0xe1, 0x93, 0xce, 0x13, 0x6e, 0xf4, 0xb9, 0xb8, 0xea, 0xdc, 0xc9, 0x83, 0xcb, 0xe5,
            0x7d, 0x2b, 0x2f, 0x4e, 0xef, 0x75, 0x1f, 0x10, 0x4b, 0x6e, 0xbe, 0xf1, 0xc3, 0x61,
            0x33, 0x71, 0x65, 0xce,
        ]),
        BigUint::from_bytes_le(&[
            0x39, 0x94, 0x43, 0x4f, 0xd3, 0x74, 0x27, 0x94, 0xc7, 0x1b, 0x0f, 0xbb, 0x2b, 0xd4,
            0x9b, 0xf9, 0xe7, 0xfc, 0x47, 0x63, 0x44, 0x28, 0xa7, 0x81, 0x20, 0x91, 0x8e, 0xcf,
            0x5d, 0x66, 0xdf, 0xf0,
        ]),
    ];
    let client01_spoof_pri_key = RsaPrivateKey::from_components(n, e, d, primes);

    let peer_auth_client01_spoof =
        PubKeyAuthenticator::new(client01_spoof_pub_key, client01_spoof_pri_key);
    let unicast = TransportManager::config_unicast()
        .max_links(2)
        .peer_authenticator(HashSet::from_iter(vec![peer_auth_client01_spoof.into()]));
    let client01_spoof_manager = TransportManager::builder()
        .whatami(WhatAmI::Client)
        .zid(client01_spoof_id)
        .unicast(unicast)
        .build(Arc::new(SHClientAuthenticator::default()))
        .unwrap();

    /* [1] */
    println!("\nTransport Authenticator PubKey [1a1]");
    // Add the locator on the router
    let res = ztimeout!(router_manager.add_listener(endpoint.clone()));
    println!("Transport Authenticator PubKey [1a1]: {:?}", res);
    assert!(res.is_ok());
    println!("Transport Authenticator PubKey [1a2]");
    let locators = router_manager.get_listeners();
    println!("Transport Authenticator PubKey [1a2]: {:?}", locators);
    assert_eq!(locators.len(), 1);

    /* [2a] */
    // Open a first transport from client01 to the router
    // -> This should be accepted
    println!("Transport Authenticator PubKey [2a1]");
    let res = ztimeout!(client01_manager.open_transport(endpoint.clone()));
    println!("Transport Authenticator PubKey [2a2]: {:?}", res);
    assert!(res.is_ok());
    let c_ses1 = res.unwrap();
    assert_eq!(c_ses1.get_links().unwrap().len(), 1);

    /* [2b] */
    // Open a second transport from client01 to the router
    // -> This should be accepted
    println!("Transport Authenticator PubKey [2b1]");
    let res = ztimeout!(client01_manager.open_transport(endpoint.clone()));
    println!("Transport Authenticator PubKey [2b2]: {:?}", res);
    assert!(res.is_ok());
    assert_eq!(c_ses1.get_links().unwrap().len(), 2);

    /* [2c] */
    // Open a third transport from client01 to the router
    // -> This should be rejected
    println!("Transport Authenticator PubKey [2c1]");
    let res = ztimeout!(client01_manager.open_transport(endpoint.clone()));
    println!("Transport Authenticator PubKey [2c2]: {:?}", res);
    assert!(res.is_err());
    assert_eq!(c_ses1.get_links().unwrap().len(), 2);

    /* [2d] */
    // Close the session
    println!("Transport Authenticator PubKey [2d1]");
    let res = ztimeout!(c_ses1.close());
    println!("Transport Authenticator PubKey [2d2]: {:?}", res);
    assert!(res.is_ok());

    ztimeout!(async {
        while !router_manager.get_transports().is_empty() {
            task::sleep(SLEEP).await;
        }
    });

    /* [3a] */
    // Open a first transport from client02 to the router
    // -> This should be accepted
    println!("Transport Authenticator PubKey [3a1]");
    let res = ztimeout!(client02_manager.open_transport(endpoint.clone()));
    println!("Transport Authenticator PubKey [3a2]: {:?}", res);
    assert!(res.is_ok());
    let c_ses2 = res.unwrap();
    assert_eq!(c_ses2.get_links().unwrap().len(), 1);

    /* [3b] */
    // Open a second transport from client02 to the router
    // -> This should be accepted
    println!("Transport Authenticator PubKey [3b1]");
    let res = ztimeout!(client02_manager.open_transport(endpoint.clone()));
    println!("Transport Authenticator PubKey [3b2]: {:?}", res);
    assert!(res.is_ok());
    assert_eq!(c_ses2.get_links().unwrap().len(), 2);

    /* [3c] */
    // Open a third transport from client02 to the router
    // -> This should be rejected
    println!("Transport Authenticator PubKey [3c1]");
    let res = ztimeout!(client02_manager.open_transport(endpoint.clone()));
    println!("Transport Authenticator PubKey [3c2]: {:?}", res);
    assert!(res.is_err());
    assert_eq!(c_ses2.get_links().unwrap().len(), 2);

    /* [3d] */
    // Close the session
    println!("Transport Authenticator PubKey [3d1]");
    let res = ztimeout!(c_ses2.close());
    println!("Transport Authenticator PubKey [3d2]: {:?}", res);
    assert!(res.is_ok());

    ztimeout!(async {
        while !router_manager.get_transports().is_empty() {
            task::sleep(SLEEP).await;
        }
    });

    /* [4a] */
    // Open a first transport from client01_spoof to the router
    // -> This should be accepted
    println!("Transport Authenticator PubKey [4a1]");
    let res = ztimeout!(client01_spoof_manager.open_transport(endpoint.clone()));
    println!("Transport Authenticator PubKey [4a2]: {:?}", res);
    assert!(res.is_ok());
    let c_ses1_spoof = res.unwrap();
    assert_eq!(c_ses1_spoof.get_links().unwrap().len(), 1);

    /* [4b] */
    // Open a second transport from client01_spoof to the router
    // -> This should be accepted
    println!("Transport Authenticator PubKey [4b1]");
    let res = ztimeout!(client01_spoof_manager.open_transport(endpoint.clone()));
    println!("Transport Authenticator PubKey [4b2]: {:?}", res);
    assert!(res.is_ok());
    assert_eq!(c_ses1_spoof.get_links().unwrap().len(), 2);

    /* [4c] */
    // Open a third transport from client02 to the router
    // -> This should be rejected
    println!("Transport Authenticator PubKey [41]");
    let res = ztimeout!(client01_spoof_manager.open_transport(endpoint.clone()));
    println!("Transport Authenticator PubKey [4c2]: {:?}", res);
    assert!(res.is_err());
    assert_eq!(c_ses1_spoof.get_links().unwrap().len(), 2);

    /* [4d] */
    // Close the session
    println!("Transport Authenticator PubKey [4d1]");
    let res = ztimeout!(c_ses1_spoof.close());
    println!("Transport Authenticator PubKey [4d2]: {:?}", res);
    assert!(res.is_ok());

    ztimeout!(async {
        while !router_manager.get_transports().is_empty() {
            task::sleep(SLEEP).await;
        }
    });

    /* [5a] */
    // Open a first transport from client01 to the router
    // -> This should be accepted
    println!("Transport Authenticator PubKey [5a1]");
    let res = ztimeout!(client01_manager.open_transport(endpoint.clone()));
    println!("Transport Authenticator PubKey [5a2]: {:?}", res);
    assert!(res.is_ok());
    let c_ses1 = res.unwrap();
    assert_eq!(c_ses1.get_links().unwrap().len(), 1);

    /* [5b] */
    // Open a spoof transport from client01_spoof to the router
    // -> This should be rejected. Spoofing detected.
    println!("Transport Authenticator PubKey [5b1]");
    let res = ztimeout!(client01_spoof_manager.open_transport(endpoint.clone()));
    println!("Transport Authenticator PubKey [5b2]: {:?}", res);
    assert!(res.is_err());

    /* [5c] */
    // Open a second transport from client01 to the router
    // -> This should be accepted
    println!("Transport Authenticator PubKey [5a1]");
    let res = ztimeout!(client01_manager.open_transport(endpoint.clone()));
    println!("Transport Authenticator PubKey [5a2]: {:?}", res);
    assert!(res.is_ok());
    let c_ses1 = res.unwrap();
    assert_eq!(c_ses1.get_links().unwrap().len(), 2);

    /* [5d] */
    // Close the session
    println!("Transport Authenticator PubKey [5d1]");
    let res = ztimeout!(c_ses1.close());
    println!("Transport Authenticator PubKey [5d2]: {:?}", res);
    assert!(res.is_ok());

    ztimeout!(async {
        while !router_manager.get_transports().is_empty() {
            task::sleep(SLEEP).await;
        }
    });

    /* [6] */
    // Perform clean up of the open locators
    println!("Transport Authenticator UserPassword [6a1]");
    let res = ztimeout!(router_manager.del_listener(endpoint));
    println!("Transport Authenticator UserPassword [6a2]: {:?}", res);
    assert!(res.is_ok());

    ztimeout!(async {
        while !router_manager.get_listeners().is_empty() {
            task::sleep(SLEEP).await;
        }
    });

    ztimeout!(router_manager.close());
    ztimeout!(client01_manager.close());
    ztimeout!(client01_spoof_manager.close());

    // Wait a little bit
    task::sleep(SLEEP).await;
}

#[cfg(feature = "auth_usrpwd")]
async fn authenticator_user_password(endpoint: &EndPoint) {
    /* [CLIENT] */
    let client01_id = ZenohId::new(1, [1_u8; ZenohId::MAX_SIZE]);
    let user01 = "user01".to_string();
    let password01 = "password01".to_string();

    let client02_id = ZenohId::new(1, [2_u8; ZenohId::MAX_SIZE]);
    let user02 = "invalid".to_string();
    let password02 = "invalid".to_string();

    let client03_id = client01_id;
    let user03 = "user03".to_string();
    let password03 = "password03".to_string();

    /* [ROUTER] */
    let router_id = ZenohId::new(1, [0_u8; ZenohId::MAX_SIZE]);
    let router_handler = Arc::new(SHRouterAuthenticator::new());
    // Create the router transport manager
    let mut lookup: HashMap<Vec<u8>, Vec<u8>> = HashMap::new();
    lookup.insert(user01.clone().into(), password01.clone().into());
    lookup.insert(user03.clone().into(), password03.clone().into());

    let peer_auth_router = Arc::new(UserPasswordAuthenticator::new(lookup, None));
    let unicast = TransportManager::config_unicast()
        .peer_authenticator(HashSet::from_iter(vec![peer_auth_router.clone().into()]));
    let router_manager = TransportManager::builder()
        .whatami(WhatAmI::Router)
        .zid(router_id)
        .unicast(unicast)
        .build(router_handler.clone())
        .unwrap();

    // Create the transport transport manager for the first client
    let lookup: HashMap<Vec<u8>, Vec<u8>> = HashMap::new();
    let peer_auth_client01 = UserPasswordAuthenticator::new(
        lookup,
        Some((user01.clone().into(), password01.clone().into())),
    );
    let unicast = TransportManager::config_unicast()
        .peer_authenticator(HashSet::from_iter(vec![peer_auth_client01.into()]));
    let client01_manager = TransportManager::builder()
        .whatami(WhatAmI::Client)
        .zid(client01_id)
        .unicast(unicast)
        .build(Arc::new(SHClientAuthenticator::default()))
        .unwrap();

    // Create the transport transport manager for the second client
    let lookup: HashMap<Vec<u8>, Vec<u8>> = HashMap::new();
    let peer_auth_client02 = UserPasswordAuthenticator::new(
        lookup,
        Some((user02.clone().into(), password02.clone().into())),
    );
    let unicast = TransportManager::config_unicast()
        .peer_authenticator(HashSet::from_iter(vec![peer_auth_client02.into()]));
    let client02_manager = TransportManager::builder()
        .whatami(WhatAmI::Client)
        .zid(client02_id)
        .unicast(unicast)
        .build(Arc::new(SHClientAuthenticator::default()))
        .unwrap();

    // Create the transport transport manager for the third client
    let lookup: HashMap<Vec<u8>, Vec<u8>> = HashMap::new();
    let peer_auth_client03 = UserPasswordAuthenticator::new(
        lookup,
        Some((user03.clone().into(), password03.clone().into())),
    );
    let unicast = TransportManager::config_unicast()
        .peer_authenticator(HashSet::from_iter(vec![peer_auth_client03.into()]));
    let client03_manager = TransportManager::builder()
        .whatami(WhatAmI::Client)
        .zid(client03_id)
        .unicast(unicast)
        .build(Arc::new(SHClientAuthenticator::default()))
        .unwrap();

    /* [1] */
    println!("\nTransport Authenticator UserPassword [1a1]");
    // Add the locator on the router
    let res = ztimeout!(router_manager.add_listener(endpoint.clone()));
    println!("Transport Authenticator UserPassword [1a1]: {:?}", res);
    assert!(res.is_ok());
    println!("Transport Authenticator UserPassword [1a2]");
    let locators = router_manager.get_listeners();
    println!("Transport Authenticator UserPassword [1a2]: {:?}", locators);
    assert_eq!(locators.len(), 1);

    /* [2] */
    // Open a first transport from the client to the router
    // -> This should be accepted
    println!("Transport Authenticator UserPassword [2a1]");
    let res = ztimeout!(client01_manager.open_transport(endpoint.clone()));
    println!("Transport Authenticator UserPassword [2a1]: {:?}", res);
    assert!(res.is_ok());
    let c_ses1 = res.unwrap();

    /* [3] */
    println!("Transport Authenticator UserPassword [3a1]");
    let res = ztimeout!(c_ses1.close());
    println!("Transport Authenticator UserPassword [3a1]: {:?}", res);
    assert!(res.is_ok());

    ztimeout!(async {
        while !router_manager.get_transports().is_empty() {
            task::sleep(SLEEP).await;
        }
    });

    /* [4] */
    // Open a second transport from the client to the router
    // -> This should be rejected
    println!("Transport Authenticator UserPassword [4a1]");
    let res = ztimeout!(client02_manager.open_transport(endpoint.clone()));
    println!("Transport Authenticator UserPassword [4a1]: {:?}", res);
    assert!(res.is_err());

    /* [5] */
    // Open a third transport from the client to the router
    // -> This should be accepted
    println!("Transport Authenticator UserPassword [5a1]");
    let res = ztimeout!(client01_manager.open_transport(endpoint.clone()));
    println!("Transport Authenticator UserPassword [5a1]: {:?}", res);
    assert!(res.is_ok());
    let c_ses1 = res.unwrap();

    /* [6] */
    // Add client02 credentials on the router
    let res = ztimeout!(peer_auth_router.add_user(user02.into(), password02.into()));
    assert!(res.is_ok());
    // Open a fourth transport from the client to the router
    // -> This should be accepted
    println!("Transport Authenticator UserPassword [6a1]");
    let res = ztimeout!(client02_manager.open_transport(endpoint.clone()));
    println!("Transport Authenticator UserPassword [6a1]: {:?}", res);
    assert!(res.is_ok());
    let c_ses2 = res.unwrap();

    /* [7] */
    // Open a fourth transport from the client to the router
    // -> This should be rejected
    println!("Transport Authenticator UserPassword [7a1]");
    let res = ztimeout!(client03_manager.open_transport(endpoint.clone()));
    println!("Transport Authenticator UserPassword [7a1]: {:?}", res);
    assert!(res.is_err());

    /* [8] */
    println!("Transport Authenticator UserPassword [8a1]");
    let res = ztimeout!(c_ses1.close());
    println!("Transport Authenticator UserPassword [8a1]: {:?}", res);
    assert!(res.is_ok());
    println!("Transport Authenticator UserPassword [8a2]");
    let res = ztimeout!(c_ses2.close());
    println!("Transport Authenticator UserPassword [8a2]: {:?}", res);
    assert!(res.is_ok());

    ztimeout!(async {
        while !router_manager.get_transports().is_empty() {
            task::sleep(SLEEP).await;
        }
    });

    /* [9] */
    // Perform clean up of the open locators
    println!("Transport Authenticator UserPassword [9a1]");
    let res = ztimeout!(router_manager.del_listener(endpoint));
    println!("Transport Authenticator UserPassword [9a2]: {:?}", res);
    assert!(res.is_ok());

    ztimeout!(async {
        while !router_manager.get_listeners().is_empty() {
            task::sleep(SLEEP).await;
        }
    });

    // Wait a little bit
    task::sleep(SLEEP).await;
}

#[cfg(feature = "shared-memory")]
async fn authenticator_shared_memory(endpoint: &EndPoint) {
    /* [CLIENT] */
    let client_id = ZenohId::new(1, [1u8; ZenohId::MAX_SIZE]);

    /* [ROUTER] */
    let router_id = ZenohId::new(1, [0_u8; ZenohId::MAX_SIZE]);
    let router_handler = Arc::new(SHRouterAuthenticator::new());
    // Create the router transport manager
    let peer_auth_router = SharedMemoryAuthenticator::make().unwrap();
    let unicast = TransportManager::config_unicast()
        .peer_authenticator(HashSet::from_iter(vec![peer_auth_router.into()]));
    let router_manager = TransportManager::builder()
        .whatami(WhatAmI::Router)
        .zid(router_id)
        .unicast(unicast)
        .build(router_handler.clone())
        .unwrap();

    // Create the transport transport manager for the first client
    let peer_auth_client = SharedMemoryAuthenticator::make().unwrap();
    let unicast = TransportManager::config_unicast()
        .peer_authenticator(HashSet::from_iter(vec![peer_auth_client.into()]));
    let client_manager = TransportManager::builder()
        .whatami(WhatAmI::Router)
        .zid(client_id)
        .unicast(unicast)
        .build(Arc::new(SHClientAuthenticator::default()))
        .unwrap();

    /* [1] */
    println!("\nTransport Authenticator SharedMemory [1a1]");
    // Add the locator on the router
    let res = ztimeout!(router_manager.add_listener(endpoint.clone()));
    println!("Transport Authenticator SharedMemory [1a1]: {:?}", res);
    assert!(res.is_ok());
    println!("Transport Authenticator SharedMemory [1a2]");
    let locators = router_manager.get_listeners();
    println!("Transport Authenticator SharedMemory 1a2]: {:?}", locators);
    assert_eq!(locators.len(), 1);

    /* [2] */
    // Open a transport from the client to the router
    // -> This should be accepted
    println!("Transport Authenticator SharedMemory [2a1]");
    let res = ztimeout!(client_manager.open_transport(endpoint.clone()));
    println!("Transport Authenticator SharedMemory [2a1]: {:?}", res);
    assert!(res.is_ok());
    let c_ses1 = res.unwrap();
    assert!(c_ses1.is_shm().unwrap());

    /* [3] */
    println!("Transport Authenticator SharedMemory [3a1]");
    let res = ztimeout!(c_ses1.close());
    println!("Transport Authenticator SharedMemory [3a1]: {:?}", res);
    assert!(res.is_ok());

    ztimeout!(async {
        while !router_manager.get_transports().is_empty() {
            task::sleep(SLEEP).await;
        }
    });

    /* [4] */
    // Perform clean up of the open locators
    println!("Transport Authenticator SharedMemory [4a1]");
    let res = ztimeout!(router_manager.del_listener(endpoint));
    println!("Transport Authenticator SharedMemory [4a2]: {:?}", res);
    assert!(res.is_ok());

    ztimeout!(async {
        while !router_manager.get_listeners().is_empty() {
            task::sleep(SLEEP).await;
        }
    });

    task::sleep(SLEEP).await;
}

async fn run(endpoint: &EndPoint) {
    #[cfg(feature = "auth_pubkey")]
    authenticator_multilink(endpoint).await;
    #[cfg(feature = "auth_usrpwd")]
    authenticator_user_password(endpoint).await;
    #[cfg(feature = "shared-memory")]
    authenticator_shared_memory(endpoint).await;
}

#[cfg(feature = "transport_tcp")]
#[test]
fn authenticator_tcp() {
    task::block_on(async {
        zasync_executor_init!();
    });

    let endpoint: EndPoint = "tcp/127.0.0.1:11447".parse().unwrap();
    task::block_on(run(&endpoint));
}

#[cfg(feature = "transport_udp")]
#[test]
fn authenticator_udp() {
    task::block_on(async {
        zasync_executor_init!();
    });

    let endpoint: EndPoint = "udp/127.0.0.1:11447".parse().unwrap();
    task::block_on(run(&endpoint));
}

#[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
#[test]
fn authenticator_unix() {
    task::block_on(async {
        zasync_executor_init!();
    });

    let _ = std::fs::remove_file("zenoh-test-unix-socket-10.sock");
    let endpoint: EndPoint = "unixsock-stream/zenoh-test-unix-socket-10.sock"
        .parse()
        .unwrap();
    task::block_on(run(&endpoint));
    let _ = std::fs::remove_file("zenoh-test-unix-socket-10.sock");
    let _ = std::fs::remove_file("zenoh-test-unix-socket-10.sock.lock");
}

#[cfg(feature = "transport_tls")]
#[test]
fn authenticator_tls() {
    use zenoh::net::link::tls::config::*;

    task::block_on(async {
        zasync_executor_init!();
    });

    // NOTE: this an auto-generated pair of certificate and key.
    //       The target domain is localhost, so it has no real
    //       mapping to any existing domain. The certificate and key
    //       have been generated using: https://github.com/jsha/minica
    let key = "-----BEGIN RSA PRIVATE KEY-----
MIIEowIBAAKCAQEAz105EYUbOdW5uJ8o/TqtxtOtKJL7AQdy5yiXoslosAsulaew
4JSJetVa6Fa6Bq5BK6fsphGD9bpGGeiBZFBt75JRjOrkj4DwlLGa0CPLTgG5hul4
Ufe9B7VG3J5P8OwUqIYmPzj8uTbNtkgFRcYumHR28h4GkYdG5Y04AV4vIjgKE47j
AgV5ACRHkcmGrTzF2HOes2wT73l4yLSkKR4GlIWu5cLRdI8PTUmjMFAh/GIh1ahd
+VqXz051V3jok0n1klVNjc6DnWuH3j/MSOg/52C3YfcUjCeIJGVfcqDnPTJKSNEF
yVTYCUjWy+B0B4fMz3MpU17dDWpvS5hfc4VrgQIDAQABAoIBAQCq+i208XBqdnwk
6y7r5Tcl6qErBE3sIk0upjypX7Ju/TlS8iqYckENQ+AqFGBcY8+ehF5O68BHm2hz
sk8F/H84+wc8zuzYGjPEFtEUb38RecCUqeqog0Gcmm6sN+ioOLAr6DifBojy2mox
sx6N0oPW9qigp/s4gTcGzTLxhcwNRHWuoWjQwq6y6qwt2PJXnllii5B5iIJhKAxE
EOmcVCmFbPavQ1Xr9F5jd5rRc1TYq28hXX8dZN2JhdVUbLlHzaiUfTnA/8yI4lyq
bEmqu29Oqe+CmDtB6jRnrLiIwyZxzXKuxXaO6NqgxqtaVjLcdISEgZMeHEftuOtf
C1xxodaVAoGBAOb1Y1SvUGx+VADSt1d30h3bBm1kU/1LhLKZOAQrnFMrEfyOfYbz
AZ4FJgXE6ZsB1BA7hC0eJDVHz8gTgDJQrOOO8WJWDGRe4TbZkCi5IizYg5UH/6az
I/WKlfdA4j1tftbQhycHL+9bGzdoRzrwIK489PG4oVAJJCaK2CVtx+l3AoGBAOXY
75sHOiMaIvDA7qlqFbaBkdi1NzH7bCgy8IntNfLxlOCmGjxeNZzKrkode3JWY9SI
Mo/nuWj8EZBEHj5omCapzOtkW/Nhnzc4C6U3BCspdrQ4mzbmzEGTdhqvxepa7U7K
iRcoD1iU7kINCEwg2PsB/BvCSrkn6lpIJlYXlJDHAoGAY7QjgXd9fJi8ou5Uf8oW
RxU6nRbmuz5Sttc2O3aoMa8yQJkyz4Mwe4s1cuAjCOutJKTM1r1gXC/4HyNsAEyb
llErG4ySJPJgv1EEzs+9VSbTBw9A6jIDoAiH3QmBoYsXapzy+4I6y1XFVhIKTgND
2HQwOfm+idKobIsb7GyMFNkCgYBIsixWZBrHL2UNsHfLrXngl2qBmA81B8hVjob1
mMkPZckopGB353Qdex1U464/o4M/nTQgv7GsuszzTBgktQAqeloNuVg7ygyJcnh8
cMIoxJx+s8ijvKutse4Q0rdOQCP+X6CsakcwRSp2SZjuOxVljmMmhHUNysocc+Vs
JVkf0QKBgHiCVLU60EoPketADvhRJTZGAtyCMSb3q57Nb0VIJwxdTB5KShwpul1k
LPA8Z7Y2i9+IEXcPT0r3M+hTwD7noyHXNlNuzwXot4B8PvbgKkMLyOpcwBjppJd7
ns4PifoQbhDFnZPSfnrpr+ZXSEzxtiyv7Ql69jznl/vB8b75hBL4
-----END RSA PRIVATE KEY-----";

    let cert = "-----BEGIN CERTIFICATE-----
MIIDLDCCAhSgAwIBAgIIIXlwQVKrtaAwDQYJKoZIhvcNAQELBQAwIDEeMBwGA1UE
AxMVbWluaWNhIHJvb3QgY2EgMmJiOTlkMB4XDTIxMDIwMjE0NDYzNFoXDTIzMDMw
NDE0NDYzNFowFDESMBAGA1UEAxMJbG9jYWxob3N0MIIBIjANBgkqhkiG9w0BAQEF
AAOCAQ8AMIIBCgKCAQEAz105EYUbOdW5uJ8o/TqtxtOtKJL7AQdy5yiXoslosAsu
laew4JSJetVa6Fa6Bq5BK6fsphGD9bpGGeiBZFBt75JRjOrkj4DwlLGa0CPLTgG5
hul4Ufe9B7VG3J5P8OwUqIYmPzj8uTbNtkgFRcYumHR28h4GkYdG5Y04AV4vIjgK
E47jAgV5ACRHkcmGrTzF2HOes2wT73l4yLSkKR4GlIWu5cLRdI8PTUmjMFAh/GIh
1ahd+VqXz051V3jok0n1klVNjc6DnWuH3j/MSOg/52C3YfcUjCeIJGVfcqDnPTJK
SNEFyVTYCUjWy+B0B4fMz3MpU17dDWpvS5hfc4VrgQIDAQABo3YwdDAOBgNVHQ8B
Af8EBAMCBaAwHQYDVR0lBBYwFAYIKwYBBQUHAwEGCCsGAQUFBwMCMAwGA1UdEwEB
/wQCMAAwHwYDVR0jBBgwFoAULXa6lBiO7OLL5Z6XuF5uF5wR9PQwFAYDVR0RBA0w
C4IJbG9jYWxob3N0MA0GCSqGSIb3DQEBCwUAA4IBAQBOMkNXfzPEDU475zbiSi3v
JOhpZLyuoaYY62RzZc9VF8YRybJlWKUWdR3szAiUd1xCJe/beNX7b9lPg6wNadKq
DGTWFmVxSfpVMO9GQYBXLDcNaAUXzsDLC5sbAFST7jkAJELiRn6KtQYxZ2kEzo7G
QmzNMfNMc1KeL8Qr4nfEHZx642yscSWj9edGevvx4o48j5KXcVo9+pxQQFao9T2O
F5QxyGdov+uNATWoYl92Gj8ERi7ovHimU3H7HLIwNPqMJEaX4hH/E/Oz56314E9b
AXVFFIgCSluyrolaD6CWD9MqOex4YOfJR2bNxI7lFvuK4AwjyUJzT1U1HXib17mM
-----END CERTIFICATE-----";

    // Configure the client
    let ca = "-----BEGIN CERTIFICATE-----
MIIDSzCCAjOgAwIBAgIIK7mduKtTVxkwDQYJKoZIhvcNAQELBQAwIDEeMBwGA1UE
AxMVbWluaWNhIHJvb3QgY2EgMmJiOTlkMCAXDTIxMDIwMjEzMTc0NVoYDzIxMjEw
MjAyMTMxNzQ1WjAgMR4wHAYDVQQDExVtaW5pY2Egcm9vdCBjYSAyYmI5OWQwggEi
MA0GCSqGSIb3DQEBAQUAA4IBDwAwggEKAoIBAQCoBZOxIfVq7LoEpVCMlQzuDnFy
d+yuk5pFasEQvZ3IvWVta4rPFJ3WGl4UNF6v9bZegNHp+oo70guZ8ps9ez34qrwB
rrNtZ0YJLDvR0ygloinZZeiclrZcu+x9vRdnyfWqrAulJBMlJIbbHcNx2OCkq7MM
HdpLJMXxKVbIlQQYGUzRkNTAaK2PiFX5BaqmnZZyo7zNbz7L2asg+0K/FpiS2IRA
coHPTa9BtsLUJUPRHPr08pgTjM1MQwa+Xxg1+wtMh85xdrqMi6Oe0cxefS+0L04F
KVfMD3bW8AyuugvcTEpGnea2EvMoPfLWpnPGU3XO8lRZyotZDQzrPvNyYKM3AgMB
AAGjgYYwgYMwDgYDVR0PAQH/BAQDAgKEMB0GA1UdJQQWMBQGCCsGAQUFBwMBBggr
BgEFBQcDAjASBgNVHRMBAf8ECDAGAQH/AgEAMB0GA1UdDgQWBBQtdrqUGI7s4svl
npe4Xm4XnBH09DAfBgNVHSMEGDAWgBQtdrqUGI7s4svlnpe4Xm4XnBH09DANBgkq
hkiG9w0BAQsFAAOCAQEAJliEt607VUOSDsUeabhG8MIhYDhxe+mjJ4i7N/0xk9JU
piCUdQr26HyYCzN+bNdjw663rxuVGtTTdHSw2CJHsPSOEDinbYkLMSyDeomsnr0S
4e0hKUeqXXYg0iC/O2283ZEvvQK5SE+cjm0La0EmqO0mj3Mkc4Fsg8hExYuOur4M
M0AufDKUhroksKKiCmjsFj1x55VcU45Ag8069lzBk7ntcGQpHUUkwZzvD4FXf8IR
pVVHiH6WC99p77T9Di99dE5ufjsprfbzkuafgTo2Rz03HgPq64L4po/idP8uBMd6
tOzot3pwe+3SJtpk90xAQrABEO0Zh2unrC8i83ySfg==
-----END CERTIFICATE-----";

    // Define the locator
    let mut endpoint: EndPoint = "tls/localhost:11448".parse().unwrap();
    let mut config = Properties::default();
    config.insert(TLS_ROOT_CA_CERTIFICATE_RAW.to_string(), ca.to_string());
    config.insert(TLS_SERVER_PRIVATE_KEY_RAW.to_string(), key.to_string());
    config.insert(TLS_SERVER_CERTIFICATE_RAW.to_string(), cert.to_string());
    endpoint.config = Some(Arc::new(config));

    task::block_on(run(&endpoint));
}

#[cfg(feature = "transport_quic")]
#[test]
fn authenticator_quic() {
    use zenoh::net::link::quic::config::*;

    task::block_on(async {
        zasync_executor_init!();
    });

    // NOTE: this an auto-generated pair of certificate and key.
    //       The target domain is localhost, so it has no real
    //       mapping to any existing domain. The certificate and key
    //       have been generated using: https://github.com/jsha/minica
    let key = "-----BEGIN RSA PRIVATE KEY-----
MIIEowIBAAKCAQEAz105EYUbOdW5uJ8o/TqtxtOtKJL7AQdy5yiXoslosAsulaew
4JSJetVa6Fa6Bq5BK6fsphGD9bpGGeiBZFBt75JRjOrkj4DwlLGa0CPLTgG5hul4
Ufe9B7VG3J5P8OwUqIYmPzj8uTbNtkgFRcYumHR28h4GkYdG5Y04AV4vIjgKE47j
AgV5ACRHkcmGrTzF2HOes2wT73l4yLSkKR4GlIWu5cLRdI8PTUmjMFAh/GIh1ahd
+VqXz051V3jok0n1klVNjc6DnWuH3j/MSOg/52C3YfcUjCeIJGVfcqDnPTJKSNEF
yVTYCUjWy+B0B4fMz3MpU17dDWpvS5hfc4VrgQIDAQABAoIBAQCq+i208XBqdnwk
6y7r5Tcl6qErBE3sIk0upjypX7Ju/TlS8iqYckENQ+AqFGBcY8+ehF5O68BHm2hz
sk8F/H84+wc8zuzYGjPEFtEUb38RecCUqeqog0Gcmm6sN+ioOLAr6DifBojy2mox
sx6N0oPW9qigp/s4gTcGzTLxhcwNRHWuoWjQwq6y6qwt2PJXnllii5B5iIJhKAxE
EOmcVCmFbPavQ1Xr9F5jd5rRc1TYq28hXX8dZN2JhdVUbLlHzaiUfTnA/8yI4lyq
bEmqu29Oqe+CmDtB6jRnrLiIwyZxzXKuxXaO6NqgxqtaVjLcdISEgZMeHEftuOtf
C1xxodaVAoGBAOb1Y1SvUGx+VADSt1d30h3bBm1kU/1LhLKZOAQrnFMrEfyOfYbz
AZ4FJgXE6ZsB1BA7hC0eJDVHz8gTgDJQrOOO8WJWDGRe4TbZkCi5IizYg5UH/6az
I/WKlfdA4j1tftbQhycHL+9bGzdoRzrwIK489PG4oVAJJCaK2CVtx+l3AoGBAOXY
75sHOiMaIvDA7qlqFbaBkdi1NzH7bCgy8IntNfLxlOCmGjxeNZzKrkode3JWY9SI
Mo/nuWj8EZBEHj5omCapzOtkW/Nhnzc4C6U3BCspdrQ4mzbmzEGTdhqvxepa7U7K
iRcoD1iU7kINCEwg2PsB/BvCSrkn6lpIJlYXlJDHAoGAY7QjgXd9fJi8ou5Uf8oW
RxU6nRbmuz5Sttc2O3aoMa8yQJkyz4Mwe4s1cuAjCOutJKTM1r1gXC/4HyNsAEyb
llErG4ySJPJgv1EEzs+9VSbTBw9A6jIDoAiH3QmBoYsXapzy+4I6y1XFVhIKTgND
2HQwOfm+idKobIsb7GyMFNkCgYBIsixWZBrHL2UNsHfLrXngl2qBmA81B8hVjob1
mMkPZckopGB353Qdex1U464/o4M/nTQgv7GsuszzTBgktQAqeloNuVg7ygyJcnh8
cMIoxJx+s8ijvKutse4Q0rdOQCP+X6CsakcwRSp2SZjuOxVljmMmhHUNysocc+Vs
JVkf0QKBgHiCVLU60EoPketADvhRJTZGAtyCMSb3q57Nb0VIJwxdTB5KShwpul1k
LPA8Z7Y2i9+IEXcPT0r3M+hTwD7noyHXNlNuzwXot4B8PvbgKkMLyOpcwBjppJd7
ns4PifoQbhDFnZPSfnrpr+ZXSEzxtiyv7Ql69jznl/vB8b75hBL4
-----END RSA PRIVATE KEY-----";

    let cert = "-----BEGIN CERTIFICATE-----
MIIDLDCCAhSgAwIBAgIIIXlwQVKrtaAwDQYJKoZIhvcNAQELBQAwIDEeMBwGA1UE
AxMVbWluaWNhIHJvb3QgY2EgMmJiOTlkMB4XDTIxMDIwMjE0NDYzNFoXDTIzMDMw
NDE0NDYzNFowFDESMBAGA1UEAxMJbG9jYWxob3N0MIIBIjANBgkqhkiG9w0BAQEF
AAOCAQ8AMIIBCgKCAQEAz105EYUbOdW5uJ8o/TqtxtOtKJL7AQdy5yiXoslosAsu
laew4JSJetVa6Fa6Bq5BK6fsphGD9bpGGeiBZFBt75JRjOrkj4DwlLGa0CPLTgG5
hul4Ufe9B7VG3J5P8OwUqIYmPzj8uTbNtkgFRcYumHR28h4GkYdG5Y04AV4vIjgK
E47jAgV5ACRHkcmGrTzF2HOes2wT73l4yLSkKR4GlIWu5cLRdI8PTUmjMFAh/GIh
1ahd+VqXz051V3jok0n1klVNjc6DnWuH3j/MSOg/52C3YfcUjCeIJGVfcqDnPTJK
SNEFyVTYCUjWy+B0B4fMz3MpU17dDWpvS5hfc4VrgQIDAQABo3YwdDAOBgNVHQ8B
Af8EBAMCBaAwHQYDVR0lBBYwFAYIKwYBBQUHAwEGCCsGAQUFBwMCMAwGA1UdEwEB
/wQCMAAwHwYDVR0jBBgwFoAULXa6lBiO7OLL5Z6XuF5uF5wR9PQwFAYDVR0RBA0w
C4IJbG9jYWxob3N0MA0GCSqGSIb3DQEBCwUAA4IBAQBOMkNXfzPEDU475zbiSi3v
JOhpZLyuoaYY62RzZc9VF8YRybJlWKUWdR3szAiUd1xCJe/beNX7b9lPg6wNadKq
DGTWFmVxSfpVMO9GQYBXLDcNaAUXzsDLC5sbAFST7jkAJELiRn6KtQYxZ2kEzo7G
QmzNMfNMc1KeL8Qr4nfEHZx642yscSWj9edGevvx4o48j5KXcVo9+pxQQFao9T2O
F5QxyGdov+uNATWoYl92Gj8ERi7ovHimU3H7HLIwNPqMJEaX4hH/E/Oz56314E9b
AXVFFIgCSluyrolaD6CWD9MqOex4YOfJR2bNxI7lFvuK4AwjyUJzT1U1HXib17mM
-----END CERTIFICATE-----";

    // Configure the client
    let ca = "-----BEGIN CERTIFICATE-----
MIIDSzCCAjOgAwIBAgIIK7mduKtTVxkwDQYJKoZIhvcNAQELBQAwIDEeMBwGA1UE
AxMVbWluaWNhIHJvb3QgY2EgMmJiOTlkMCAXDTIxMDIwMjEzMTc0NVoYDzIxMjEw
MjAyMTMxNzQ1WjAgMR4wHAYDVQQDExVtaW5pY2Egcm9vdCBjYSAyYmI5OWQwggEi
MA0GCSqGSIb3DQEBAQUAA4IBDwAwggEKAoIBAQCoBZOxIfVq7LoEpVCMlQzuDnFy
d+yuk5pFasEQvZ3IvWVta4rPFJ3WGl4UNF6v9bZegNHp+oo70guZ8ps9ez34qrwB
rrNtZ0YJLDvR0ygloinZZeiclrZcu+x9vRdnyfWqrAulJBMlJIbbHcNx2OCkq7MM
HdpLJMXxKVbIlQQYGUzRkNTAaK2PiFX5BaqmnZZyo7zNbz7L2asg+0K/FpiS2IRA
coHPTa9BtsLUJUPRHPr08pgTjM1MQwa+Xxg1+wtMh85xdrqMi6Oe0cxefS+0L04F
KVfMD3bW8AyuugvcTEpGnea2EvMoPfLWpnPGU3XO8lRZyotZDQzrPvNyYKM3AgMB
AAGjgYYwgYMwDgYDVR0PAQH/BAQDAgKEMB0GA1UdJQQWMBQGCCsGAQUFBwMBBggr
BgEFBQcDAjASBgNVHRMBAf8ECDAGAQH/AgEAMB0GA1UdDgQWBBQtdrqUGI7s4svl
npe4Xm4XnBH09DAfBgNVHSMEGDAWgBQtdrqUGI7s4svlnpe4Xm4XnBH09DANBgkq
hkiG9w0BAQsFAAOCAQEAJliEt607VUOSDsUeabhG8MIhYDhxe+mjJ4i7N/0xk9JU
piCUdQr26HyYCzN+bNdjw663rxuVGtTTdHSw2CJHsPSOEDinbYkLMSyDeomsnr0S
4e0hKUeqXXYg0iC/O2283ZEvvQK5SE+cjm0La0EmqO0mj3Mkc4Fsg8hExYuOur4M
M0AufDKUhroksKKiCmjsFj1x55VcU45Ag8069lzBk7ntcGQpHUUkwZzvD4FXf8IR
pVVHiH6WC99p77T9Di99dE5ufjsprfbzkuafgTo2Rz03HgPq64L4po/idP8uBMd6
tOzot3pwe+3SJtpk90xAQrABEO0Zh2unrC8i83ySfg==
-----END CERTIFICATE-----";

    // Define the locator
    let mut endpoint: EndPoint = "quic/localhost:11448".parse().unwrap();
    let mut config = Properties::default();
    config.insert(TLS_ROOT_CA_CERTIFICATE_RAW.to_string(), ca.to_string());
    config.insert(TLS_SERVER_PRIVATE_KEY_RAW.to_string(), key.to_string());
    config.insert(TLS_SERVER_CERTIFICATE_RAW.to_string(), cert.to_string());
    endpoint.config = Some(Arc::new(config));

    task::block_on(run(&endpoint));
}
