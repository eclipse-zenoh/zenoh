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
use async_std::sync::Arc;
use async_std::task;
use std::any::Any;
#[cfg(feature = "auth_usrpwd")]
use std::collections::HashMap;
use std::collections::HashSet;
use std::iter::FromIterator;
use std::time::Duration;
use zenoh::net::link::{EndPoint, Link};
use zenoh::net::protocol::core::{PeerId, WhatAmI};
use zenoh::net::protocol::proto::ZenohMessage;
// #[cfg(feature = "auth_pubkey")]
// use zenoh::net::transport::unicast::establishment::authenticator::PubKeyAuthenticator;
#[cfg(feature = "zero-copy")]
use zenoh::net::transport::unicast::establishment::authenticator::SharedMemoryAuthenticator;
#[cfg(feature = "auth_usrpwd")]
use zenoh::net::transport::unicast::establishment::authenticator::UserPasswordAuthenticator;
use zenoh::net::transport::{
    DummyTransportPeerEventHandler, TransportEventHandler, TransportManager,
    TransportManagerConfig, TransportManagerConfigUnicast, TransportMulticast,
    TransportMulticastEventHandler, TransportPeer, TransportPeerEventHandler, TransportUnicast,
};
use zenoh_util::core::ZResult;
use zenoh_util::properties::Properties;
use zenoh_util::zasync_executor_init;

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
async fn authenticator_public_key(_endpoint: &EndPoint) {
    // RsaPublicKey { n: BigUint { data: [13854569197451851669, 17694260563073856544, 7177042759546308301, 5448683402327741903, 11613381432655861382, 18176980596997491973, 1643934184321650053, 14058391388315639667] }, e: BigUint { data: [65537] } }
    // RsaPrivateKey { pubkey_components: RsaPublicKey { n: BigUint { data: [13854569197451851669, 17694260563073856544, 7177042759546308301, 5448683402327741903, 11613381432655861382, 18176980596997491973, 1643934184321650053, 14058391388315639667] }, e: BigUint { data: [65537] } }, d: BigUint { data: [7447105889854916609, 7706984235858323390, 543010913175397935, 7281429826252030630, 12421727492480610771, 4477469312556402117, 2280913078553678180, 4995955497411710146] }, primes: [BigUint { data: [17614414467766477461, 9408422114921960360, 16178501485145592533, 15900752384880489729] }, BigUint { data: [18016786655892160769, 9145900177685724685, 14952227153430670259, 16309388496288445361] }], precomputed: Some(PrecomputedValues { dp: BigUint { data: [10531729316280232829, 13306510626894743537, 6366538247360590135, 14546433453400518207] }, dq: BigUint { data: [16800401846487837953, 8325141234391518337, 16850505700991737475, 7656107803047347628] }, qinv: BigInt { sign: Plus, data: BigUint { data: [4923901714057849726, 9020674298733315867, 3753418685796228084, 10488744668634804796] } }, crt_values: [] }) }

    // RsaPublicKey { n: BigUint { data: [17473122527927429655, 16298871317093084869, 4940340869122128330, 2037748821152051721, 10824600817118707509, 14372565566066002801, 9302684737608554218, 13585081155721536499] }, e: BigUint { data: [65537] } }
    // RsaPrivateKey { pubkey_components: RsaPublicKey { n: BigUint { data: [17473122527927429655, 16298871317093084869, 4940340869122128330, 2037748821152051721, 10824600817118707509, 14372565566066002801, 9302684737608554218, 13585081155721536499] }, e: BigUint { data: [65537] } }, d: BigUint { data: [7456242084992586705, 18367601275490331694, 4117193244150050720, 3458731659176767144, 16862346202955676289, 3838642081251794606, 11708402587340966852, 12835732415652058884] }, primes: [BigUint { data: [17316650767328074763, 6012558276658106095, 16039307155668761347, 14079877292100419224] }, BigUint { data: [13821189690405627301, 6948065444946967995, 13329837469803163478, 17798487167268861554] }], precomputed: Some(PrecomputedValues { dp: BigUint { data: [5577060041868571123, 1089772660824360317, 17092006454431308196, 1616660460855023187] }, dq: BigUint { data: [18056483174682837301, 12519384622875629877, 1184699978395139147, 5511157057622213521] }, qinv: BigInt { sign: Plus, data: BigUint { data: [14761750939420702040, 12499151064593160523, 13006945694343633768, 1488704957604081267] } }, crt_values: [] }) }

    // RsaPublicKey { n: BigUint { data: [3686350342620908375, 8545614685518232883, 14215578824320109297, 16675002711059108518, 16155907951513455499, 11414564319314822353, 17175189137638047078, 15995100422626709276] }, e: BigUint { data: [65537] } }
    // RsaPrivateKey { pubkey_components: RsaPublicKey { n: BigUint { data: [3686350342620908375, 8545614685518232883, 14215578824320109297, 16675002711059108518, 16155907951513455499, 11414564319314822353, 17175189137638047078, 15995100422626709276] }, e: BigUint { data: [65537] } }, d: BigUint { data: [4556826134197859009, 3562961612395979568, 14586344940073008745, 16160937605978972348, 9093301098868660612, 14414478351979291262, 12038776970606293352, 3041502379827792712] }, primes: [BigUint { data: [12846697277240834759, 1500159518305670828, 3217625493990706223, 17581557268255462034] }, BigUint { data: [935854861169810161, 8446685943269626472, 10690214627302603441, 16782217833583043577] }], precomputed: Some(PrecomputedValues { dp: BigUint { data: [14187687900726169745, 154247825920773355, 18444916654681770889, 5294560542079890728] }, dq: BigUint { data: [12618408670020134257, 188422378886325779, 10863583501584294867, 3923285463605685500] }, qinv: BigInt { sign: Plus, data: BigUint { data: [17432429445919805588, 9736632911158649548, 13805301904022454805, 8934107280997551563] } }, crt_values: [] }) }

    // RsaPublicKey { n: BigUint { data: [16383961692702960071, 12438583860330527170, 1058179066964940137, 11677588414324440620, 15783156963962073405, 2595157166562914867, 3869258657212364853, 13529756005834192120] }, e: BigUint { data: [65537] } }
    // RsaPrivateKey { pubkey_components: RsaPublicKey { n: BigUint { data: [16383961692702960071, 12438583860330527170, 1058179066964940137, 11677588414324440620, 15783156963962073405, 2595157166562914867, 3869258657212364853, 13529756005834192120] }, e: BigUint { data: [65537] } }, d: BigUint { data: [3327746175160399905, 4754389458571766303, 18445880705045551384, 6483221579223935667, 3221262248421972271, 15396530501593367425, 9073637640712678817, 7529858256508481062] }, primes: [BigUint { data: [13268502174880840607, 11414023979431771042, 12088498362166724207, 15430067556460481771] }, BigUint { data: [9871421612426305753, 12157523347948669706, 14109055328688991186, 16174909507435072059] }], precomputed: Some(PrecomputedValues { dp: BigUint { data: [15588907300746335631, 11357825803605464791, 9138345614797710638, 15040177999820743031] }, dq: BigUint { data: [1760081535143795225, 17218762781349774384, 9017324801166990592, 10612894757605864376] }, qinv: BigInt { sign: Plus, data: BigUint { data: [9522232260123452820, 14787968413756624855, 436611369251140883, 10574616638065942359] } }, crt_values: [] }) }

    // /* [CLIENT] */
    // let client01_id = PeerId::new(1, [1u8; PeerId::MAX_SIZE]);
    // let client02_id = PeerId::new(1, [2u8; PeerId::MAX_SIZE]);

    // /* [ROUTER] */
    // let router_id = PeerId::new(1, [0u8; PeerId::MAX_SIZE]);
    // let router_handler = Arc::new(SHRouterAuthenticator::new());
    // // Create the router transport manager
    // let peer_auth_router = Arc::new(PubKeyAuthenticator::new());
    // let config = TransportManagerConfig::builder()
    //     .whatami(WhatAmI::Router)
    //     .pid(router_id)
    //     .unicast(
    //         TransportManagerConfigUnicast::builder()
    //             .peer_authenticator(HashSet::from_iter(vec![peer_auth_router.clone().into()]))
    //             .build(),
    //     )
    //     .build(router_handler.clone());
    // let router_manager = TransportManager::new(config);

    // // Create the transport transport manager for the first client
    // let peer_auth_client01 = PubKeyAuthenticator::new();
    // let config = TransportManagerConfig::builder()
    //     .whatami(WhatAmI::Client)
    //     .pid(client01_id)
    //     .unicast(
    //         TransportManagerConfigUnicast::builder()
    //             .peer_authenticator(HashSet::from_iter(vec![peer_auth_client01.into()]))
    //             .build(),
    //     )
    //     .build(Arc::new(SHClientAuthenticator::default()));
    // let client01_manager = TransportManager::new(config);

    // // Create the transport transport manager for the second client
    // let config = TransportManagerConfig::builder()
    //     .whatami(WhatAmI::Client)
    //     .pid(client02_id)
    //     .unicast(
    //         TransportManagerConfigUnicast::builder()
    //             .max_links(1)
    //             .build(),
    //     )
    //     .build(Arc::new(SHClientAuthenticator::default()));
    // let client02_manager = TransportManager::new(config);

    // // Create the transport transport manager for the third client
    // let peer_auth_client01_spoof = PubKeyAuthenticator::new();
    // let config = TransportManagerConfig::builder()
    //     .whatami(WhatAmI::Client)
    //     .pid(client01_id)
    //     .unicast(
    //         TransportManagerConfigUnicast::builder()
    //             .peer_authenticator(HashSet::from_iter(vec![peer_auth_client01_spoof.into()]))
    //             .build(),
    //     )
    //     .build(Arc::new(SHClientAuthenticator::default()));
    // let client01_spoof_manager = TransportManager::new(config);

    // /* [1] */
    // println!("\nTransport Authenticator PubKey [1a1]");
    // // Add the locator on the router
    // let res = router_manager.add_listener(endpoint.clone()).await;
    // println!("Transport Authenticator PubKey [1a1]: {:?}", res);
    // assert!(res.is_ok());
    // println!("Transport Authenticator PubKey [1a2]");
    // let locators = router_manager.get_listeners();
    // println!("Transport Authenticator PubKey [1a2]: {:?}", locators);
    // assert_eq!(locators.len(), 1);

    // /* [2] */
    // // Open a first transport from the client to the router
    // // -> This should be accepted
    // println!("Transport Authenticator PubKey [2a1]");
    // let res = client01_manager.open_transport(endpoint.clone()).await;
    // println!("Transport Authenticator PubKey [2a2]: {:?}", res);
    // assert!(res.is_ok());
    // let c_ses1 = res.unwrap();
    // assert_eq!(c_ses1.get_links().unwrap().len(), 1);

    // /* [3] */
    // // Open a second transport from the client to the router
    // // -> This should be accepted
    // println!("Transport Authenticator PubKey [3a1]");
    // let res = client01_manager.open_transport(endpoint.clone()).await;
    // println!("Transport Authenticator PubKey [3a2]: {:?}", res);
    // assert!(res.is_ok());
    // assert_eq!(c_ses1.get_links().unwrap().len(), 2);

    // /* [4] */
    // // Close the session
    // println!("Transport Authenticator PubKey [4a1]");
    // let res = c_ses1.close().await;
    // println!("Transport Authenticator PubKey [4a2]: {:?}", res);
    // assert!(res.is_ok());

    // task::sleep(SLEEP).await;

    // /* [5] */
    // // Open a first transport from the client to the router
    // // -> This should be accepted
    // println!("Transport Authenticator PubKey [5a1]");
    // let res = client02_manager.open_transport(endpoint.clone()).await;
    // println!("Transport Authenticator PubKey [5a2]: {:?}", res);
    // assert!(res.is_ok());
    // let c_ses2 = res.unwrap();
    // assert_eq!(c_ses2.get_links().unwrap().len(), 1);

    // /* [6] */
    // // Open a second transport from the client to the router
    // // -> This should be rejected
    // println!("Transport Authenticator PubKey [6a1]");
    // let res = client02_manager.open_transport(endpoint.clone()).await;
    // println!("Transport Authenticator PubKey [6a2]: {:?}", res);
    // assert!(res.is_err());
    // assert_eq!(c_ses2.get_links().unwrap().len(), 1);

    // /* [7] */
    // // Close the session
    // println!("Transport Authenticator PubKey [7a1]");
    // let res = c_ses2.close().await;
    // println!("Transport Authenticator PubKey [7a2]: {:?}", res);
    // assert!(res.is_ok());

    // task::sleep(SLEEP).await;

    // /* [8] */
    // // Open a first transport from the client to the router
    // // -> This should be accepted
    // println!("Transport Authenticator PubKey [8a1]");
    // let res = client01_spoof_manager
    //     .open_transport(endpoint.clone())
    //     .await;
    // println!("Transport Authenticator PubKey [8a2]: {:?}", res);
    // assert!(res.is_ok());
    // let c_ses1_spoof = res.unwrap();
    // assert_eq!(c_ses1_spoof.get_links().unwrap().len(), 1);

    // /* [9] */
    // // Open a second transport from the client to the router
    // // -> This should be accepted
    // println!("Transport Authenticator PubKey [9a1]");
    // let res = client01_spoof_manager
    //     .open_transport(endpoint.clone())
    //     .await;
    // println!("Transport Authenticator PubKey [9a2]: {:?}", res);
    // assert!(res.is_ok());
    // assert_eq!(c_ses1_spoof.get_links().unwrap().len(), 2);

    // /* [10] */
    // // Close the session
    // println!("Transport Authenticator PubKey [10a1]");
    // let res = c_ses1_spoof.close().await;
    // println!("Transport Authenticator PubKey [10a2]: {:?}", res);
    // assert!(res.is_ok());

    // task::sleep(SLEEP).await;

    // /* [11] */
    // // Open a first transport from the client to the router
    // // -> This should be accepted
    // println!("Transport Authenticator PubKey [11a1]");
    // let res = client01_manager.open_transport(endpoint.clone()).await;
    // println!("Transport Authenticator PubKey [11a2]: {:?}", res);
    // assert!(res.is_ok());
    // let c_ses1 = res.unwrap();
    // assert_eq!(c_ses1.get_links().unwrap().len(), 1);

    // /* [12] */
    // // Open a spoof transport from the client to the router
    // // -> This should be rejected
    // println!("Transport Authenticator PubKey [12a1]");
    // let res = client01_spoof_manager
    //     .open_transport(endpoint.clone())
    //     .await;
    // println!("Transport Authenticator PubKey [12a2]: {:?}", res);
    // assert!(res.is_err());

    // /* [13] */
    // // Close the session
    // println!("Transport Authenticator PubKey [13a1]");
    // let res = c_ses1.close().await;
    // println!("Transport Authenticator PubKey [13a2]: {:?}", res);
    // assert!(res.is_ok());

    // task::sleep(SLEEP).await;

    // /* [14] */
    // // Perform clean up of the open locators
    // println!("Transport Authenticator UserPassword [14a1]");
    // let res = router_manager.del_listener(endpoint).await;
    // println!("Transport Authenticator UserPassword [14a2]: {:?}", res);
    // assert!(res.is_ok());

    // task::sleep(SLEEP).await;
}

#[cfg(feature = "auth_usrpwd")]
async fn authenticator_user_password(endpoint: &EndPoint) {
    /* [CLIENT] */
    let client01_id = PeerId::new(1, [1u8; PeerId::MAX_SIZE]);
    let user01 = "user01".to_string();
    let password01 = "password01".to_string();

    let client02_id = PeerId::new(1, [2u8; PeerId::MAX_SIZE]);
    let user02 = "invalid".to_string();
    let password02 = "invalid".to_string();

    let client03_id = client01_id;
    let user03 = "user03".to_string();
    let password03 = "password03".to_string();

    /* [ROUTER] */
    let router_id = PeerId::new(1, [0u8; PeerId::MAX_SIZE]);
    let router_handler = Arc::new(SHRouterAuthenticator::new());
    // Create the router transport manager
    let mut lookup: HashMap<Vec<u8>, Vec<u8>> = HashMap::new();
    lookup.insert(user01.clone().into(), password01.clone().into());
    lookup.insert(user03.clone().into(), password03.clone().into());

    let peer_auth_router = Arc::new(UserPasswordAuthenticator::new(lookup, None));
    let config = TransportManagerConfig::builder()
        .whatami(WhatAmI::Router)
        .pid(router_id)
        .unicast(
            TransportManagerConfigUnicast::builder()
                .peer_authenticator(HashSet::from_iter(vec![peer_auth_router.clone().into()]))
                .build()
                .unwrap(),
        )
        .build(router_handler.clone())
        .unwrap();
    let router_manager = TransportManager::new(config);

    // Create the transport transport manager for the first client
    let lookup: HashMap<Vec<u8>, Vec<u8>> = HashMap::new();
    let peer_auth_client01 = UserPasswordAuthenticator::new(
        lookup,
        Some((user01.clone().into(), password01.clone().into())),
    );

    let config = TransportManagerConfig::builder()
        .whatami(WhatAmI::Client)
        .pid(client01_id)
        .unicast(
            TransportManagerConfigUnicast::builder()
                .peer_authenticator(HashSet::from_iter(vec![peer_auth_client01.into()]))
                .build()
                .unwrap(),
        )
        .build(Arc::new(SHClientAuthenticator::default()))
        .unwrap();
    let client01_manager = TransportManager::new(config);

    // Create the transport transport manager for the second client
    let lookup: HashMap<Vec<u8>, Vec<u8>> = HashMap::new();
    let peer_auth_client02 = UserPasswordAuthenticator::new(
        lookup,
        Some((user02.clone().into(), password02.clone().into())),
    );
    let config = TransportManagerConfig::builder()
        .whatami(WhatAmI::Client)
        .pid(client02_id)
        .unicast(
            TransportManagerConfigUnicast::builder()
                .peer_authenticator(HashSet::from_iter(vec![peer_auth_client02.into()]))
                .build()
                .unwrap(),
        )
        .build(Arc::new(SHClientAuthenticator::default()))
        .unwrap();
    let client02_manager = TransportManager::new(config);

    // Create the transport transport manager for the third client
    let lookup: HashMap<Vec<u8>, Vec<u8>> = HashMap::new();
    let peer_auth_client03 = UserPasswordAuthenticator::new(
        lookup,
        Some((user03.clone().into(), password03.clone().into())),
    );
    let config = TransportManagerConfig::builder()
        .whatami(WhatAmI::Client)
        .pid(client03_id)
        .unicast(
            TransportManagerConfigUnicast::builder()
                .peer_authenticator(HashSet::from_iter(vec![peer_auth_client03.into()]))
                .build()
                .unwrap(),
        )
        .build(Arc::new(SHClientAuthenticator::default()))
        .unwrap();
    let client03_manager = TransportManager::new(config);

    /* [1] */
    println!("\nTransport Authenticator UserPassword [1a1]");
    // Add the locator on the router
    let res = router_manager.add_listener(endpoint.clone()).await;
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
    let res = client01_manager.open_transport(endpoint.clone()).await;
    println!("Transport Authenticator UserPassword [2a1]: {:?}", res);
    assert!(res.is_ok());
    let c_ses1 = res.unwrap();

    /* [3] */
    println!("Transport Authenticator UserPassword [3a1]");
    let res = c_ses1.close().await;
    println!("Transport Authenticator UserPassword [3a1]: {:?}", res);
    assert!(res.is_ok());

    /* [4] */
    // Open a second transport from the client to the router
    // -> This should be rejected
    println!("Transport Authenticator UserPassword [4a1]");
    let res = client02_manager.open_transport(endpoint.clone()).await;
    println!("Transport Authenticator UserPassword [4a1]: {:?}", res);
    assert!(res.is_err());

    /* [5] */
    // Open a third transport from the client to the router
    // -> This should be accepted
    println!("Transport Authenticator UserPassword [5a1]");
    let res = client01_manager.open_transport(endpoint.clone()).await;
    println!("Transport Authenticator UserPassword [5a1]: {:?}", res);
    assert!(res.is_ok());
    let c_ses1 = res.unwrap();

    /* [6] */
    // Add client02 credentials on the router
    let res = peer_auth_router
        .add_user(user02.into(), password02.into())
        .await;
    assert!(res.is_ok());
    // Open a fourth transport from the client to the router
    // -> This should be accepted
    println!("Transport Authenticator UserPassword [6a1]");
    let res = client02_manager.open_transport(endpoint.clone()).await;
    println!("Transport Authenticator UserPassword [6a1]: {:?}", res);
    assert!(res.is_ok());
    let c_ses2 = res.unwrap();

    /* [7] */
    // Open a fourth transport from the client to the router
    // -> This should be rejected
    println!("Transport Authenticator UserPassword [7a1]");
    let res = client03_manager.open_transport(endpoint.clone()).await;
    println!("Transport Authenticator UserPassword [7a1]: {:?}", res);
    assert!(res.is_err());

    /* [8] */
    println!("Transport Authenticator UserPassword [8a1]");
    let res = c_ses1.close().await;
    println!("Transport Authenticator UserPassword [8a1]: {:?}", res);
    assert!(res.is_ok());
    println!("Transport Authenticator UserPassword [8a2]");
    let res = c_ses2.close().await;
    println!("Transport Authenticator UserPassword [8a2]: {:?}", res);
    assert!(res.is_ok());

    task::sleep(SLEEP).await;

    /* [9] */
    // Perform clean up of the open locators
    println!("Transport Authenticator UserPassword [9a1]");
    let res = router_manager.del_listener(endpoint).await;
    println!("Transport Authenticator UserPassword [9a2]: {:?}", res);
    assert!(res.is_ok());

    task::sleep(SLEEP).await;
}

#[cfg(feature = "zero-copy")]
async fn authenticator_shared_memory(endpoint: &EndPoint) {
    /* [CLIENT] */
    let client_id = PeerId::new(1, [1u8; PeerId::MAX_SIZE]);

    /* [ROUTER] */
    let router_id = PeerId::new(1, [0u8; PeerId::MAX_SIZE]);
    let router_handler = Arc::new(SHRouterAuthenticator::new());
    // Create the router transport manager
    let peer_auth_router = SharedMemoryAuthenticator::new();
    let config = TransportManagerConfig::builder()
        .whatami(WhatAmI::Router)
        .pid(router_id)
        .unicast(
            TransportManagerConfigUnicast::builder()
                .peer_authenticator(HashSet::from_iter(vec![peer_auth_router.into()]))
                .build()
                .unwrap(),
        )
        .build(router_handler.clone())
        .unwrap();
    let router_manager = TransportManager::new(config);

    // Create the transport transport manager for the first client
    let peer_auth_client = SharedMemoryAuthenticator::new();
    let config = TransportManagerConfig::builder()
        .whatami(WhatAmI::Router)
        .pid(client_id)
        .unicast(
            TransportManagerConfigUnicast::builder()
                .peer_authenticator(HashSet::from_iter(vec![peer_auth_client.into()]))
                .build()
                .unwrap(),
        )
        .build(Arc::new(SHClientAuthenticator::default()))
        .unwrap();
    let client_manager = TransportManager::new(config);

    /* [1] */
    println!("\nTransport Authenticator SharedMemory [1a1]");
    // Add the locator on the router
    let res = router_manager.add_listener(endpoint.clone()).await;
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
    let res = client_manager.open_transport(endpoint.clone()).await;
    println!("Transport Authenticator SharedMemory [2a1]: {:?}", res);
    assert!(res.is_ok());
    let c_ses1 = res.unwrap();
    assert!(c_ses1.is_shm().unwrap());

    /* [3] */
    println!("Transport Authenticator SharedMemory [3a1]");
    let res = c_ses1.close().await;
    println!("Transport Authenticator SharedMemory [3a1]: {:?}", res);
    assert!(res.is_ok());

    task::sleep(SLEEP).await;

    /* [4] */
    // Perform clean up of the open locators
    println!("Transport Authenticator SharedMemory [4a1]");
    let res = router_manager.del_listener(endpoint).await;
    println!("Transport Authenticator SharedMemory [4a2]: {:?}", res);
    assert!(res.is_ok());

    task::sleep(SLEEP).await;
}

async fn run(endpoint: &EndPoint) {
    #[cfg(feature = "auth_pubkey")]
    authenticator_public_key(endpoint).await;
    #[cfg(feature = "auth_usrpwd")]
    authenticator_user_password(endpoint).await;
    #[cfg(feature = "zero-copy")]
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
