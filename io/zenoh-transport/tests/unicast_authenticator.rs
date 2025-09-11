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
use std::{any::Any, sync::Arc, time::Duration};

use zenoh_core::{zasyncwrite, ztimeout};
use zenoh_link::Link;
use zenoh_protocol::{
    core::{EndPoint, WhatAmI, ZenohIdProto},
    network::NetworkMessageMut,
};
use zenoh_result::ZResult;
use zenoh_transport::{
    multicast::TransportMulticast,
    unicast::{establishment::ext::auth::Auth, TransportUnicast},
    DummyTransportPeerEventHandler, TransportEventHandler, TransportMulticastEventHandler,
    TransportPeer, TransportPeerEventHandler,
};

const TIMEOUT: Duration = Duration::from_secs(60);
const SLEEP: Duration = Duration::from_millis(100);

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
    fn handle_message(&self, _msg: NetworkMessageMut) -> ZResult<()> {
        Ok(())
    }
    fn new_link(&self, _link: Link) {}
    fn del_link(&self, _link: Link) {}
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
        Ok(Arc::new(DummyTransportPeerEventHandler))
    }

    fn new_multicast(
        &self,
        _transport: TransportMulticast,
    ) -> ZResult<Arc<dyn TransportMulticastEventHandler>> {
        panic!();
    }
}

#[cfg(feature = "auth_pubkey")]
async fn auth_pubkey(endpoint: &EndPoint, lowlatency_transport: bool) {
    use rsa::{BigUint, RsaPrivateKey, RsaPublicKey};
    use zenoh_transport::{
        unicast::{
            establishment::ext::auth::AuthPubKey,
            test_helpers::make_basic_transport_manager_builder,
        },
        TransportManager,
    };

    // Create the transport transport manager for the client 01
    let client01_id = ZenohIdProto::try_from([2]).unwrap();

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
    let client01_pri_key = RsaPrivateKey::from_components(n, e, d, primes).unwrap();
    let mut auth = Auth::empty();
    auth.set_pubkey(Some(AuthPubKey::new(
        client01_pub_key.clone().into(),
        client01_pri_key.into(),
    )));
    let unicast = make_basic_transport_manager_builder(lowlatency_transport).authenticator(auth);
    let client01_manager = TransportManager::builder()
        .whatami(WhatAmI::Client)
        .zid(client01_id)
        .unicast(unicast)
        .build(Arc::new(SHClientAuthenticator))
        .unwrap();

    // Create the transport transport manager for the client 02
    let client02_id = ZenohIdProto::try_from([3]).unwrap();

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
    let client02_pri_key = RsaPrivateKey::from_components(n, e, d, primes).unwrap();
    let mut auth = Auth::empty();
    auth.set_pubkey(Some(AuthPubKey::new(
        client02_pub_key.clone().into(),
        client02_pri_key.clone().into(),
    )));
    let unicast = make_basic_transport_manager_builder(lowlatency_transport).authenticator(auth);
    let client02_manager = TransportManager::builder()
        .whatami(WhatAmI::Client)
        .zid(client02_id)
        .unicast(unicast)
        .build(Arc::new(SHClientAuthenticator))
        .unwrap();

    // Create the transport transport manager for the client 03 with the same key as client 02
    let client03_id = ZenohIdProto::try_from([4]).unwrap();
    let mut auth = Auth::empty();
    auth.set_pubkey(Some(AuthPubKey::new(
        client02_pub_key.clone().into(),
        client02_pri_key.clone().into(),
    )));
    let unicast = make_basic_transport_manager_builder(lowlatency_transport).authenticator(auth);
    let client03_manager = TransportManager::builder()
        .whatami(WhatAmI::Client)
        .zid(client03_id)
        .unicast(unicast)
        .build(Arc::new(SHClientAuthenticator))
        .unwrap();

    // Create the router transport manager
    let router_id = ZenohIdProto::try_from([1]).unwrap();
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
    let router_pri_key = RsaPrivateKey::from_components(n, e, d, primes).unwrap();
    let mut auth_pubkey = AuthPubKey::new(router_pub_key.into(), router_pri_key.into());
    ztimeout!(auth_pubkey.add_pubkey(client01_pub_key.into())).unwrap();
    let mut auth = Auth::empty();
    auth.set_pubkey(Some(auth_pubkey));
    let unicast = make_basic_transport_manager_builder(lowlatency_transport).authenticator(auth);
    let router_manager = TransportManager::builder()
        .whatami(WhatAmI::Router)
        .zid(router_id)
        .unicast(unicast)
        .build(router_handler.clone())
        .unwrap();

    /* [1] */
    println!("\nTransport Authenticator PubKey [1a1]");
    // Add the locator on the router
    ztimeout!(router_manager.add_listener(endpoint.clone())).unwrap();
    println!("Transport Authenticator PubKey [1a2]");
    let locators = ztimeout!(router_manager.get_listeners());
    println!("Transport Authenticator PubKey [1a2]: {locators:?}");
    assert_eq!(locators.len(), 1);

    /* [2a] */
    // Open a first transport from client01 to the router
    // -> This should be accepted
    println!("Transport Authenticator PubKey [2a1]");
    let c_ses1 = ztimeout!(client01_manager.open_transport_unicast(endpoint.clone())).unwrap();
    assert_eq!(c_ses1.get_links().unwrap().len(), 1);

    /* [2b] */
    // Open a first transport from client02 to the router
    // -> This should be rejected
    println!("Transport Authenticator PubKey [2b1]");
    let res = ztimeout!(client02_manager.open_transport_unicast(endpoint.clone()));
    println!("Transport Authenticator PubKey [2b2]: {res:?}");
    assert!(res.is_err());

    /* [2c] */
    // Open a first transport from client03 to the router
    // -> This should be rejected
    println!("Transport Authenticator PubKey [2c1]");
    let res = ztimeout!(client03_manager.open_transport_unicast(endpoint.clone()));
    println!("Transport Authenticator PubKey [2c2]: {res:?}");
    assert!(res.is_err());

    // Add client02 pubkey to the router
    let router_auth_handle = router_manager.get_auth_handle_unicast();
    ztimeout!(
        zasyncwrite!(router_auth_handle.get_pubkey().unwrap()).add_pubkey(client02_pub_key.into())
    )
    .unwrap();

    /* [3b] */
    // Open a first transport from client02 to the router
    // -> This should be accepted
    println!("Transport Authenticator PubKey [3a1]");
    let c_ses2 = ztimeout!(client02_manager.open_transport_unicast(endpoint.clone())).unwrap();
    assert_eq!(c_ses2.get_links().unwrap().len(), 1);

    // Open a first transport from client03 to the router
    // -> This should be accepted
    println!("Transport Authenticator PubKey [3b1]");
    let c_ses3 = ztimeout!(client03_manager.open_transport_unicast(endpoint.clone())).unwrap();
    assert_eq!(c_ses3.get_links().unwrap().len(), 1);

    // /* [4a] */
    // Close the sessions
    println!("Transport Authenticator PubKey [4a1]");
    let res = ztimeout!(c_ses1.close());
    println!("Transport Authenticator PubKey [4a2]: {res:?}");
    assert!(res.is_ok());

    println!("Transport Authenticator PubKey [4b1]");
    let res = ztimeout!(c_ses2.close());
    println!("Transport Authenticator PubKey [4b2]: {res:?}");
    assert!(res.is_ok());

    println!("Transport Authenticator PubKey [4c1]");
    let res = ztimeout!(c_ses3.close());
    println!("Transport Authenticator PubKey [4c2]: {res:?}");
    assert!(res.is_ok());

    ztimeout!(async {
        while !router_manager.get_transports_unicast().await.is_empty() {
            tokio::time::sleep(SLEEP).await;
        }
    });

    /* [5] */
    // Perform clean up of the open locators
    println!("Transport Authenticator PubKey [5a1]");
    let res = ztimeout!(router_manager.del_listener(endpoint));
    println!("Transport Authenticator PubKey [5a2]: {res:?}");
    assert!(res.is_ok());

    ztimeout!(async {
        while !router_manager.get_listeners().await.is_empty() {
            tokio::time::sleep(SLEEP).await;
        }
    });

    ztimeout!(client01_manager.close());
    ztimeout!(client02_manager.close());
    ztimeout!(client03_manager.close());
    ztimeout!(router_manager.close());

    // Wait a little bit
    tokio::time::sleep(SLEEP).await;
}

#[cfg(feature = "auth_usrpwd")]
async fn auth_usrpwd(endpoint: &EndPoint, lowlatency_transport: bool) {
    use zenoh_transport::{
        unicast::{
            establishment::ext::auth::AuthUsrPwd,
            test_helpers::make_basic_transport_manager_builder,
        },
        TransportManager,
    };

    /* [CLIENT] */
    let client01_id = ZenohIdProto::try_from([2]).unwrap();
    let user01 = "user01".to_string();
    let password01 = "password01".to_string();

    let client02_id = ZenohIdProto::try_from([3]).unwrap();
    let user02 = "invalid".to_string();
    let password02 = "invalid".to_string();

    let client03_id = client01_id;
    let user03 = "user03".to_string();
    let password03 = "password03".to_string();

    /* [ROUTER] */
    let router_id = ZenohIdProto::try_from([1]).unwrap();
    let router_handler = Arc::new(SHRouterAuthenticator::new());
    // Create the router transport manager
    let mut auth_usrpwd_router = AuthUsrPwd::new(None);
    ztimeout!(auth_usrpwd_router.add_user(user01.clone().into(), password01.clone().into()))
        .unwrap();
    ztimeout!(auth_usrpwd_router.add_user(user03.clone().into(), password03.clone().into()))
        .unwrap();
    let mut auth_router = Auth::empty();
    auth_router.set_usrpwd(Some(auth_usrpwd_router));

    let unicast =
        make_basic_transport_manager_builder(lowlatency_transport).authenticator(auth_router);
    let router_manager = TransportManager::builder()
        .whatami(WhatAmI::Router)
        .zid(router_id)
        .unicast(unicast)
        .build(router_handler.clone())
        .unwrap();

    // Create the transport transport manager for the first client
    let auth_usrpwdr_client01 =
        AuthUsrPwd::new(Some((user01.clone().into(), password01.clone().into())));
    let mut auth_client01 = Auth::empty();
    auth_client01.set_usrpwd(Some(auth_usrpwdr_client01));
    let unicast =
        make_basic_transport_manager_builder(lowlatency_transport).authenticator(auth_client01);
    let client01_manager = TransportManager::builder()
        .whatami(WhatAmI::Client)
        .zid(client01_id)
        .unicast(unicast)
        .build(Arc::new(SHClientAuthenticator))
        .unwrap();

    // Create the transport transport manager for the second client
    let auth_usrpwdr_client02 =
        AuthUsrPwd::new(Some((user02.clone().into(), password02.clone().into())));
    let mut auth_client02 = Auth::empty();
    auth_client02.set_usrpwd(Some(auth_usrpwdr_client02));
    let unicast =
        make_basic_transport_manager_builder(lowlatency_transport).authenticator(auth_client02);
    let client02_manager = TransportManager::builder()
        .whatami(WhatAmI::Client)
        .zid(client02_id)
        .unicast(unicast)
        .build(Arc::new(SHClientAuthenticator))
        .unwrap();

    // Create the transport transport manager for the third client
    let auth_usrpwdr_client03 =
        AuthUsrPwd::new(Some((user03.clone().into(), password03.clone().into())));
    let mut auth_client03 = Auth::empty();
    auth_client03.set_usrpwd(Some(auth_usrpwdr_client03));
    let unicast =
        make_basic_transport_manager_builder(lowlatency_transport).authenticator(auth_client03);
    let client03_manager = TransportManager::builder()
        .whatami(WhatAmI::Client)
        .zid(client03_id)
        .unicast(unicast)
        .build(Arc::new(SHClientAuthenticator))
        .unwrap();

    /* [1] */
    println!("\nTransport Authenticator UserPassword [1a1]");
    // Add the locator on the router
    let res = ztimeout!(router_manager.add_listener(endpoint.clone()));
    println!("Transport Authenticator UserPassword [1a1]: {res:?}");
    assert!(res.is_ok());
    println!("Transport Authenticator UserPassword [1a2]");
    let locators = ztimeout!(router_manager.get_listeners());
    println!("Transport Authenticator UserPassword [1a2]: {locators:?}");
    assert_eq!(locators.len(), 1);

    /* [2] */
    // Open a first transport from the client to the router
    // -> This should be accepted
    println!("Transport Authenticator UserPassword [2a1]");
    let res = ztimeout!(client01_manager.open_transport_unicast(endpoint.clone()));
    println!("Transport Authenticator UserPassword [2a1]: {res:?}");
    assert!(res.is_ok());
    let c_ses1 = res.unwrap();

    /* [3] */
    println!("Transport Authenticator UserPassword [3a1]");
    let res = ztimeout!(c_ses1.close());
    println!("Transport Authenticator UserPassword [3a1]: {res:?}");
    assert!(res.is_ok());

    ztimeout!(async {
        while !router_manager.get_transports_unicast().await.is_empty() {
            tokio::time::sleep(SLEEP).await;
        }
    });

    /* [4] */
    // Open a second transport from the client to the router
    // -> This should be rejected
    println!("Transport Authenticator UserPassword [4a1]");
    let res = ztimeout!(client02_manager.open_transport_unicast(endpoint.clone()));
    println!("Transport Authenticator UserPassword [4a1]: {res:?}");
    assert!(res.is_err());

    /* [5] */
    // Open a third transport from the client to the router
    // -> This should be accepted
    println!("Transport Authenticator UserPassword [5a1]");
    let res = ztimeout!(client01_manager.open_transport_unicast(endpoint.clone()));
    println!("Transport Authenticator UserPassword [5a1]: {res:?}");
    assert!(res.is_ok());
    let c_ses1 = res.unwrap();

    /* [6] */
    // Add client02 credentials on the router
    let auth_router = router_manager.get_auth_handle_unicast();
    ztimeout!(
        zasyncwrite!(auth_router.get_usrpwd().unwrap()).add_user(user02.into(), password02.into())
    )
    .unwrap();

    // Open a fourth transport from the client to the router
    // -> This should be accepted
    println!("Transport Authenticator UserPassword [6a1]");
    let res = ztimeout!(client02_manager.open_transport_unicast(endpoint.clone()));
    println!("Transport Authenticator UserPassword [6a1]: {res:?}");
    assert!(res.is_ok());
    let c_ses2 = res.unwrap();

    /* [7] */
    // Open a fourth transport from the client to the router
    // -> This should be rejected
    println!("Transport Authenticator UserPassword [7a1]");
    let res = ztimeout!(client03_manager.open_transport_unicast(endpoint.clone()));
    println!("Transport Authenticator UserPassword [7a1]: {res:?}");
    assert!(res.is_err());

    /* [8] */
    println!("Transport Authenticator UserPassword [8a1]");
    let res = ztimeout!(c_ses1.close());
    println!("Transport Authenticator UserPassword [8a1]: {res:?}");
    assert!(res.is_ok());
    println!("Transport Authenticator UserPassword [8a2]");
    let res = ztimeout!(c_ses2.close());
    println!("Transport Authenticator UserPassword [8a2]: {res:?}");
    assert!(res.is_ok());

    ztimeout!(async {
        while !router_manager.get_transports_unicast().await.is_empty() {
            tokio::time::sleep(SLEEP).await;
        }
    });

    /* [9] */
    // Perform clean up of the open locators
    println!("Transport Authenticator UserPassword [9a1]");
    let res = ztimeout!(router_manager.del_listener(endpoint));
    println!("Transport Authenticator UserPassword [9a2]: {res:?}");
    assert!(res.is_ok());

    ztimeout!(async {
        while !router_manager.get_listeners().await.is_empty() {
            tokio::time::sleep(SLEEP).await;
        }
    });

    // Wait a little bit
    tokio::time::sleep(SLEEP).await;
}

async fn run(endpoint: &EndPoint, lowlatency_transport: bool) {
    #[cfg(feature = "auth_pubkey")]
    auth_pubkey(endpoint, lowlatency_transport).await;
    #[cfg(feature = "auth_usrpwd")]
    auth_usrpwd(endpoint, lowlatency_transport).await;
}

async fn run_with_universal_transport(endpoint: &EndPoint) {
    run(endpoint, false).await
}

async fn run_with_lowlatency_transport(endpoint: &EndPoint) {
    run(endpoint, true).await
}

#[cfg(feature = "transport_tcp")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn authenticator_tcp() {
    zenoh_util::init_log_from_env_or("error");
    let endpoint: EndPoint = format!("tcp/127.0.0.1:{}", 8000).parse().unwrap();
    run_with_universal_transport(&endpoint).await;
}

#[cfg(feature = "transport_tcp")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn authenticator_tcp_with_lowlatency_transport() {
    zenoh_util::init_log_from_env_or("error");
    let endpoint: EndPoint = format!("tcp/127.0.0.1:{}", 8100).parse().unwrap();
    run_with_lowlatency_transport(&endpoint).await;
}

#[cfg(feature = "transport_udp")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn authenticator_udp() {
    zenoh_util::init_log_from_env_or("error");
    let endpoint: EndPoint = format!("udp/127.0.0.1:{}", 8010).parse().unwrap();
    run_with_universal_transport(&endpoint).await;
}

#[cfg(feature = "transport_udp")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn authenticator_udp_with_lowlatency_transport() {
    zenoh_util::init_log_from_env_or("error");
    let endpoint: EndPoint = format!("udp/127.0.0.1:{}", 8110).parse().unwrap();
    run_with_lowlatency_transport(&endpoint).await;
}

#[cfg(feature = "transport_unixpipe")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn authenticator_unixpipe() {
    zenoh_util::init_log_from_env_or("error");
    let endpoint: EndPoint = "unixpipe/authenticator_unixpipe_test".parse().unwrap();
    run_with_universal_transport(&endpoint).await;
}

#[cfg(feature = "transport_unixpipe")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn authenticator_unixpipe_with_lowlatency_transport() {
    zenoh_util::init_log_from_env_or("error");
    let endpoint: EndPoint = "unixpipe/authenticator_unixpipe_with_lowlatency_transport"
        .parse()
        .unwrap();
    run_with_lowlatency_transport(&endpoint).await;
}

#[cfg(feature = "transport_ws")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn authenticator_ws() {
    zenoh_util::init_log_from_env_or("error");
    let endpoint: EndPoint = format!("ws/127.0.0.1:{}", 8020).parse().unwrap();
    run_with_universal_transport(&endpoint).await;
}

#[cfg(feature = "transport_ws")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn authenticator_ws_with_lowlatency_transport() {
    zenoh_util::init_log_from_env_or("error");
    let endpoint: EndPoint = format!("ws/127.0.0.1:{}", 8120).parse().unwrap();
    run_with_lowlatency_transport(&endpoint).await;
}

#[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn authenticator_unix() {
    zenoh_util::init_log_from_env_or("error");
    let f1 = "zenoh-test-unix-socket-10.sock";
    let _ = std::fs::remove_file(f1);
    let endpoint: EndPoint = format!("unixsock-stream/{f1}").parse().unwrap();
    run_with_universal_transport(&endpoint).await;
    let _ = std::fs::remove_file(f1);
    let _ = std::fs::remove_file(format!("{f1}.lock"));
}

#[cfg(feature = "transport_tls")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn authenticator_tls() {
    use zenoh_link_commons::tls::config::*;

    zenoh_util::init_log_from_env_or("error");

    // NOTE: this an auto-generated pair of certificate and key.
    //       The target domain is localhost, so it has no real
    //       mapping to any existing domain. The certificate and key
    //       have been generated using: https://github.com/jsha/minica
    let key = "-----BEGIN RSA PRIVATE KEY-----
MIIEpAIBAAKCAQEAsfqAuhElN4HnyeqLovSd4Qe+nNv5AwCjSO+HFiF30x3vQ1Hi
qRA0UmyFlSqBnFH3TUHm4Jcad40QfrX8f11NKGZdpvKHsMYqYjZnYkRFGS2s4fQy
aDbV5M06s3UDX8ETPgY41Y8fCKTSVdi9iHkwcVrXMxUu4IBBx0C1r2GSo3gkIBnU
cELdFdaUOSbdCipJhbnkwixEr2h7PXxwba7SIZgZtRaQWak1VE9b716qe3iMuMha
Efo/UoFmeZCPu5spfwaOZsnCsxRPk2IjbzlsHTJ09lM9wmbEFHBMVAXejLTk++Sr
Xt8jASZhNen/2GzyLQNAquGn98lCMQ6SsE9vLQIDAQABAoIBAGQkKggHm6Q20L+4
2+bNsoOqguLplpvM4RMpyx11qWE9h6GeUmWD+5yg+SysJQ9aw0ZSHWEjRD4ePji9
lxvm2IIxzuIftp+NcM2gBN2ywhpfq9XbO/2NVR6PJ0dQQJzBG12bzKDFDdYkP0EU
WdiPL+WoEkvo0F57bAd77n6G7SZSgxYekBF+5S6rjbu5I1cEKW+r2vLehD4uFCVX
Q0Tu7TyIOE1KJ2anRb7ZXVUaguNj0/Er7EDT1+wN8KJKvQ1tYGIq/UUBtkP9nkOI
9XJd25k6m5AQPDddzd4W6/5+M7kjyVPi3CsQcpBPss6ueyecZOMaKqdWAHeEyaak
r67TofUCgYEA6GBa+YkRvp0Ept8cd5mh4gCRM8wUuhtzTQnhubCPivy/QqMWScdn
qD0OiARLAsqeoIfkAVgyqebVnxwTrKTvWe0JwpGylEVWQtpGz3oHgjST47yZxIiY
CSAaimi2CYnJZ+QB2oBkFVwNCuXdPEGX6LgnOGva19UKrm6ONsy6V9MCgYEAxBJu
fu4dGXZreARKEHa/7SQjI9ayAFuACFlON/EgSlICzQyG/pumv1FsMEiFrv6w7PRj
4AGqzyzGKXWVDRMrUNVeGPSKJSmlPGNqXfPaXRpVEeB7UQhAs5wyMrWDl8jEW7Ih
XcWhMLn1f/NOAKyrSDSEaEM+Nuu+xTifoAghvP8CgYEAlta9Fw+nihDIjT10cBo0
38w4dOP7bFcXQCGy+WMnujOYPzw34opiue1wOlB3FIfL8i5jjY/fyzPA5PhHuSCT
Ec9xL3B9+AsOFHU108XFi/pvKTwqoE1+SyYgtEmGKKjdKOfzYA9JaCgJe1J8inmV
jwXCx7gTJVjwBwxSmjXIm+sCgYBQF8NhQD1M0G3YCdCDZy7BXRippCL0OGxVfL2R
5oKtOVEBl9NxH/3+evE5y/Yn5Mw7Dx3ZPHUcygpslyZ6v9Da5T3Z7dKcmaVwxJ+H
n3wcugv0EIHvOPLNK8npovINR6rGVj6BAqD0uZHKYYYEioQxK5rGyGkaoDQ+dgHm
qku12wKBgQDem5FvNp5iW7mufkPZMqf3sEGtu612QeqejIPFM1z7VkUgetsgPBXD
tYsqC2FtWzY51VOEKNpnfH7zH5n+bjoI9nAEAW63TK9ZKkr2hRGsDhJdGzmLfQ7v
F6/CuIw9EsAq6qIB8O88FXQqald+BZOx6AzB8Oedsz/WtMmIEmr/+Q==
-----END RSA PRIVATE KEY-----";

    let cert = "-----BEGIN CERTIFICATE-----
MIIDLjCCAhagAwIBAgIIeUtmIdFQznMwDQYJKoZIhvcNAQELBQAwIDEeMBwGA1UE
AxMVbWluaWNhIHJvb3QgY2EgMDc4ZGE3MCAXDTIzMDMwNjE2MDMxOFoYDzIxMjMw
MzA2MTYwMzE4WjAUMRIwEAYDVQQDEwlsb2NhbGhvc3QwggEiMA0GCSqGSIb3DQEB
AQUAA4IBDwAwggEKAoIBAQCx+oC6ESU3gefJ6oui9J3hB76c2/kDAKNI74cWIXfT
He9DUeKpEDRSbIWVKoGcUfdNQebglxp3jRB+tfx/XU0oZl2m8oewxipiNmdiREUZ
Lazh9DJoNtXkzTqzdQNfwRM+BjjVjx8IpNJV2L2IeTBxWtczFS7ggEHHQLWvYZKj
eCQgGdRwQt0V1pQ5Jt0KKkmFueTCLESvaHs9fHBtrtIhmBm1FpBZqTVUT1vvXqp7
eIy4yFoR+j9SgWZ5kI+7myl/Bo5mycKzFE+TYiNvOWwdMnT2Uz3CZsQUcExUBd6M
tOT75Kte3yMBJmE16f/YbPItA0Cq4af3yUIxDpKwT28tAgMBAAGjdjB0MA4GA1Ud
DwEB/wQEAwIFoDAdBgNVHSUEFjAUBggrBgEFBQcDAQYIKwYBBQUHAwIwDAYDVR0T
AQH/BAIwADAfBgNVHSMEGDAWgBTWfAmQ/BUIQm/9/llJJs2jUMWzGzAUBgNVHREE
DTALgglsb2NhbGhvc3QwDQYJKoZIhvcNAQELBQADggEBAG/POnBob0S7iYwsbtI2
3LTTbRnmseIErtJuJmI9yYzgVIm6sUSKhlIUfAIm4rfRuzE94KFeWR2w9RabxOJD
wjYLLKvQ6rFY5g2AV/J0TwDjYuq0absdaDPZ8MKJ+/lpGYK3Te+CTOfq5FJRFt1q
GOkXAxnNpGg0obeRWRKFiAMHbcw6a8LIMfRjCooo3+uSQGsbVzGxSB4CYo720KcC
9vB1K9XALwzoqCewP4aiQsMY1GWpAmzXJftY3w+lka0e9dBYcdEdOqxSoZb5OBBZ
p5e60QweRuJsb60aUaCG8HoICevXYK2fFqCQdlb5sIqQqXyN2K6HuKAFywsjsGyJ
abY=
-----END CERTIFICATE-----";

    // Configure the client
    let ca = "-----BEGIN CERTIFICATE-----
MIIDSzCCAjOgAwIBAgIIB42n1ZIkOakwDQYJKoZIhvcNAQELBQAwIDEeMBwGA1UE
AxMVbWluaWNhIHJvb3QgY2EgMDc4ZGE3MCAXDTIzMDMwNjE2MDMwN1oYDzIxMjMw
MzA2MTYwMzA3WjAgMR4wHAYDVQQDExVtaW5pY2Egcm9vdCBjYSAwNzhkYTcwggEi
MA0GCSqGSIb3DQEBAQUAA4IBDwAwggEKAoIBAQDIuCq24O4P4Aep5vAVlrIQ7P8+
uWWgcHIFYa02TmhBUB/hjo0JANCQvAtpVNuQ8NyKPlqnnq1cttePbSYVeA0rrnOs
DcfySAiyGBEY9zMjFfHJtH1wtrPcJEU8XIEY3xUlrAJE2CEuV9dVYgfEEydnvgLc
8Ug0WXSiARjqbnMW3l8jh6bYCp/UpL/gSM4mxdKrgpfyPoweGhlOWXc3RTS7cqM9
T25acURGOSI6/g8GF0sNE4VZmUvHggSTmsbLeXMJzxDWO+xVehRmbQx3IkG7u++b
QdRwGIJcDNn7zHlDMHtQ0Z1DBV94fZNBwCULhCBB5g20XTGw//S7Fj2FPwyhAgMB
AAGjgYYwgYMwDgYDVR0PAQH/BAQDAgKEMB0GA1UdJQQWMBQGCCsGAQUFBwMBBggr
BgEFBQcDAjASBgNVHRMBAf8ECDAGAQH/AgEAMB0GA1UdDgQWBBTWfAmQ/BUIQm/9
/llJJs2jUMWzGzAfBgNVHSMEGDAWgBTWfAmQ/BUIQm/9/llJJs2jUMWzGzANBgkq
hkiG9w0BAQsFAAOCAQEAvtcZFAELKiTuOiAeYts6zeKxc+nnHCzayDeD/BDCbxGJ
e1n+xdHjLtWGd+/Anc+fvftSYBPTFQqCi84lPiUIln5z/rUxE+ke81hNPIfw2obc
yIg87xCabQpVyEh8s+MV+7YPQ1+fH4FuSi2Fck1FejxkVqN2uOZPvOYUmSTsaVr1
8SfRnwJNZ9UMRPM2bD4Jkvj0VcL42JM3QkOClOzYW4j/vll2cSs4kx7er27cIoo1
Ck0v2xSPAiVjg6w65rUQeW6uB5m0T2wyj+wm0At8vzhZPlgS1fKhcmT2dzOq3+oN
R+IdLiXcyIkg0m9N8I17p0ljCSkbrgGMD3bbePRTfg==
-----END CERTIFICATE-----";

    // Define the locator
    let mut endpoint: EndPoint = format!("tls/localhost:{}", 8030).parse().unwrap();
    endpoint
        .config_mut()
        .extend_from_iter(
            [
                (TLS_ROOT_CA_CERTIFICATE_RAW, ca),
                (TLS_LISTEN_CERTIFICATE_RAW, cert),
                (TLS_LISTEN_PRIVATE_KEY_RAW, key),
            ]
            .iter()
            .copied(),
        )
        .unwrap();

    run_with_universal_transport(&endpoint).await;
}

#[cfg(feature = "transport_quic")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn authenticator_quic() {
    use zenoh_link_commons::tls::config::*;

    zenoh_util::init_log_from_env_or("error");

    // NOTE: this an auto-generated pair of certificate and key.
    //       The target domain is localhost, so it has no real
    //       mapping to any existing domain. The certificate and key
    //       have been generated using: https://github.com/jsha/minica
    let key = "-----BEGIN RSA PRIVATE KEY-----
MIIEpAIBAAKCAQEAsfqAuhElN4HnyeqLovSd4Qe+nNv5AwCjSO+HFiF30x3vQ1Hi
qRA0UmyFlSqBnFH3TUHm4Jcad40QfrX8f11NKGZdpvKHsMYqYjZnYkRFGS2s4fQy
aDbV5M06s3UDX8ETPgY41Y8fCKTSVdi9iHkwcVrXMxUu4IBBx0C1r2GSo3gkIBnU
cELdFdaUOSbdCipJhbnkwixEr2h7PXxwba7SIZgZtRaQWak1VE9b716qe3iMuMha
Efo/UoFmeZCPu5spfwaOZsnCsxRPk2IjbzlsHTJ09lM9wmbEFHBMVAXejLTk++Sr
Xt8jASZhNen/2GzyLQNAquGn98lCMQ6SsE9vLQIDAQABAoIBAGQkKggHm6Q20L+4
2+bNsoOqguLplpvM4RMpyx11qWE9h6GeUmWD+5yg+SysJQ9aw0ZSHWEjRD4ePji9
lxvm2IIxzuIftp+NcM2gBN2ywhpfq9XbO/2NVR6PJ0dQQJzBG12bzKDFDdYkP0EU
WdiPL+WoEkvo0F57bAd77n6G7SZSgxYekBF+5S6rjbu5I1cEKW+r2vLehD4uFCVX
Q0Tu7TyIOE1KJ2anRb7ZXVUaguNj0/Er7EDT1+wN8KJKvQ1tYGIq/UUBtkP9nkOI
9XJd25k6m5AQPDddzd4W6/5+M7kjyVPi3CsQcpBPss6ueyecZOMaKqdWAHeEyaak
r67TofUCgYEA6GBa+YkRvp0Ept8cd5mh4gCRM8wUuhtzTQnhubCPivy/QqMWScdn
qD0OiARLAsqeoIfkAVgyqebVnxwTrKTvWe0JwpGylEVWQtpGz3oHgjST47yZxIiY
CSAaimi2CYnJZ+QB2oBkFVwNCuXdPEGX6LgnOGva19UKrm6ONsy6V9MCgYEAxBJu
fu4dGXZreARKEHa/7SQjI9ayAFuACFlON/EgSlICzQyG/pumv1FsMEiFrv6w7PRj
4AGqzyzGKXWVDRMrUNVeGPSKJSmlPGNqXfPaXRpVEeB7UQhAs5wyMrWDl8jEW7Ih
XcWhMLn1f/NOAKyrSDSEaEM+Nuu+xTifoAghvP8CgYEAlta9Fw+nihDIjT10cBo0
38w4dOP7bFcXQCGy+WMnujOYPzw34opiue1wOlB3FIfL8i5jjY/fyzPA5PhHuSCT
Ec9xL3B9+AsOFHU108XFi/pvKTwqoE1+SyYgtEmGKKjdKOfzYA9JaCgJe1J8inmV
jwXCx7gTJVjwBwxSmjXIm+sCgYBQF8NhQD1M0G3YCdCDZy7BXRippCL0OGxVfL2R
5oKtOVEBl9NxH/3+evE5y/Yn5Mw7Dx3ZPHUcygpslyZ6v9Da5T3Z7dKcmaVwxJ+H
n3wcugv0EIHvOPLNK8npovINR6rGVj6BAqD0uZHKYYYEioQxK5rGyGkaoDQ+dgHm
qku12wKBgQDem5FvNp5iW7mufkPZMqf3sEGtu612QeqejIPFM1z7VkUgetsgPBXD
tYsqC2FtWzY51VOEKNpnfH7zH5n+bjoI9nAEAW63TK9ZKkr2hRGsDhJdGzmLfQ7v
F6/CuIw9EsAq6qIB8O88FXQqald+BZOx6AzB8Oedsz/WtMmIEmr/+Q==
-----END RSA PRIVATE KEY-----";

    let cert = "-----BEGIN CERTIFICATE-----
MIIDLjCCAhagAwIBAgIIeUtmIdFQznMwDQYJKoZIhvcNAQELBQAwIDEeMBwGA1UE
AxMVbWluaWNhIHJvb3QgY2EgMDc4ZGE3MCAXDTIzMDMwNjE2MDMxOFoYDzIxMjMw
MzA2MTYwMzE4WjAUMRIwEAYDVQQDEwlsb2NhbGhvc3QwggEiMA0GCSqGSIb3DQEB
AQUAA4IBDwAwggEKAoIBAQCx+oC6ESU3gefJ6oui9J3hB76c2/kDAKNI74cWIXfT
He9DUeKpEDRSbIWVKoGcUfdNQebglxp3jRB+tfx/XU0oZl2m8oewxipiNmdiREUZ
Lazh9DJoNtXkzTqzdQNfwRM+BjjVjx8IpNJV2L2IeTBxWtczFS7ggEHHQLWvYZKj
eCQgGdRwQt0V1pQ5Jt0KKkmFueTCLESvaHs9fHBtrtIhmBm1FpBZqTVUT1vvXqp7
eIy4yFoR+j9SgWZ5kI+7myl/Bo5mycKzFE+TYiNvOWwdMnT2Uz3CZsQUcExUBd6M
tOT75Kte3yMBJmE16f/YbPItA0Cq4af3yUIxDpKwT28tAgMBAAGjdjB0MA4GA1Ud
DwEB/wQEAwIFoDAdBgNVHSUEFjAUBggrBgEFBQcDAQYIKwYBBQUHAwIwDAYDVR0T
AQH/BAIwADAfBgNVHSMEGDAWgBTWfAmQ/BUIQm/9/llJJs2jUMWzGzAUBgNVHREE
DTALgglsb2NhbGhvc3QwDQYJKoZIhvcNAQELBQADggEBAG/POnBob0S7iYwsbtI2
3LTTbRnmseIErtJuJmI9yYzgVIm6sUSKhlIUfAIm4rfRuzE94KFeWR2w9RabxOJD
wjYLLKvQ6rFY5g2AV/J0TwDjYuq0absdaDPZ8MKJ+/lpGYK3Te+CTOfq5FJRFt1q
GOkXAxnNpGg0obeRWRKFiAMHbcw6a8LIMfRjCooo3+uSQGsbVzGxSB4CYo720KcC
9vB1K9XALwzoqCewP4aiQsMY1GWpAmzXJftY3w+lka0e9dBYcdEdOqxSoZb5OBBZ
p5e60QweRuJsb60aUaCG8HoICevXYK2fFqCQdlb5sIqQqXyN2K6HuKAFywsjsGyJ
abY=
-----END CERTIFICATE-----";

    // Configure the client
    let ca = "-----BEGIN CERTIFICATE-----
MIIDSzCCAjOgAwIBAgIIB42n1ZIkOakwDQYJKoZIhvcNAQELBQAwIDEeMBwGA1UE
AxMVbWluaWNhIHJvb3QgY2EgMDc4ZGE3MCAXDTIzMDMwNjE2MDMwN1oYDzIxMjMw
MzA2MTYwMzA3WjAgMR4wHAYDVQQDExVtaW5pY2Egcm9vdCBjYSAwNzhkYTcwggEi
MA0GCSqGSIb3DQEBAQUAA4IBDwAwggEKAoIBAQDIuCq24O4P4Aep5vAVlrIQ7P8+
uWWgcHIFYa02TmhBUB/hjo0JANCQvAtpVNuQ8NyKPlqnnq1cttePbSYVeA0rrnOs
DcfySAiyGBEY9zMjFfHJtH1wtrPcJEU8XIEY3xUlrAJE2CEuV9dVYgfEEydnvgLc
8Ug0WXSiARjqbnMW3l8jh6bYCp/UpL/gSM4mxdKrgpfyPoweGhlOWXc3RTS7cqM9
T25acURGOSI6/g8GF0sNE4VZmUvHggSTmsbLeXMJzxDWO+xVehRmbQx3IkG7u++b
QdRwGIJcDNn7zHlDMHtQ0Z1DBV94fZNBwCULhCBB5g20XTGw//S7Fj2FPwyhAgMB
AAGjgYYwgYMwDgYDVR0PAQH/BAQDAgKEMB0GA1UdJQQWMBQGCCsGAQUFBwMBBggr
BgEFBQcDAjASBgNVHRMBAf8ECDAGAQH/AgEAMB0GA1UdDgQWBBTWfAmQ/BUIQm/9
/llJJs2jUMWzGzAfBgNVHSMEGDAWgBTWfAmQ/BUIQm/9/llJJs2jUMWzGzANBgkq
hkiG9w0BAQsFAAOCAQEAvtcZFAELKiTuOiAeYts6zeKxc+nnHCzayDeD/BDCbxGJ
e1n+xdHjLtWGd+/Anc+fvftSYBPTFQqCi84lPiUIln5z/rUxE+ke81hNPIfw2obc
yIg87xCabQpVyEh8s+MV+7YPQ1+fH4FuSi2Fck1FejxkVqN2uOZPvOYUmSTsaVr1
8SfRnwJNZ9UMRPM2bD4Jkvj0VcL42JM3QkOClOzYW4j/vll2cSs4kx7er27cIoo1
Ck0v2xSPAiVjg6w65rUQeW6uB5m0T2wyj+wm0At8vzhZPlgS1fKhcmT2dzOq3+oN
R+IdLiXcyIkg0m9N8I17p0ljCSkbrgGMD3bbePRTfg==
-----END CERTIFICATE-----";

    // Define the locator
    let mut endpoint: EndPoint = format!("quic/localhost:{}", 8041).parse().unwrap();
    endpoint
        .config_mut()
        .extend_from_iter(
            [
                (TLS_ROOT_CA_CERTIFICATE_RAW, ca),
                (TLS_LISTEN_CERTIFICATE_RAW, cert),
                (TLS_LISTEN_PRIVATE_KEY_RAW, key),
            ]
            .iter()
            .copied(),
        )
        .unwrap();

    run_with_universal_transport(&endpoint).await;
}
