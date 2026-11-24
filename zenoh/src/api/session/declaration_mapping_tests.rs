//
// Copyright (c) 2026 Dexory
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   Richard Williams, <richardw347@gmail.com>
//

use zenoh_protocol::{
    core::{Region, WhatAmI},
    network::{
        declare::queryable::ext::QueryableInfoType,
        interest::{InterestMode, InterestOptions},
    },
};

use super::*;
use crate::net::tests::regions::{FaceDef, HarnessBuilder, Message};

#[test_case::test_case(WhatAmI::Client; "client")]
#[test_case::test_case(WhatAmI::Peer; "peer")]
#[test_case::test_case(WhatAmI::Router; "router")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn queued_declarations_survive_receiver_mapping_retirement(mode: WhatAmI) {
    let mut config = crate::Config::default();
    config.scouting.multicast.set_enabled(Some(false)).unwrap();
    config.scouting.gossip.set_enabled(Some(false)).unwrap();
    config.listen.endpoints.set(vec![]).unwrap();
    config.connect.endpoints.set(vec![]).unwrap();
    let receiver = crate::open(config).await.unwrap();
    let gateway = HarnessBuilder::new()
        .mode(mode)
        .subregions([Region::Local, Region::default_south(WhatAmI::Client)])
        .build();
    let destination = gateway.new_face(
        FaceDef::default()
            .mode(WhatAmI::Client)
            .region(Region::default_south(WhatAmI::Client)),
    );
    // Replace only the transport: API declarations go into a real remote router
    // face, while its outgoing declarations stay queued until explicitly delivered
    // to the real Session decoder. No wall-clock delays or network timing required.
    let original = zwrite!(receiver.0.state)
        .primitives
        .replace(destination.face.clone());
    let liveliness = receiver
        .liveliness()
        .declare_subscriber("declaration-mapping/**")
        .await
        .unwrap();
    destination.interest(
        100,
        InterestMode::Future,
        InterestOptions::SUBSCRIBERS + InterestOptions::QUERYABLES,
        "declaration-mapping/**",
    );
    let key = receiver
        .declare_keyexpr("declaration-mapping/topic")
        .await
        .unwrap();
    let retired_id = key.to_wire(&receiver).scope;
    assert_ne!(retired_id, 0);
    assert!(destination
        .face
        .state
        .remote_mappings
        .contains_key(&retired_id));

    let source = gateway.new_session();
    source.declare_token(None, 1, "declaration-mapping/topic");
    source.declare_subscriber(None, 2, "declaration-mapping/topic");
    source.declare_queryable(
        None,
        3,
        "declaration-mapping/topic",
        QueryableInfoType::DEFAULT,
    );
    assert_eq!(destination.recorder().tokens().len(), 1);
    assert_eq!(destination.recorder().subscribers().len(), 1);
    assert_eq!(destination.recorder().queryables().len(), 1);
    let queued = destination
        .recorder()
        .with_messages(|messages| messages.to_vec());

    drop(key);
    assert!(!zread!(receiver.0.state)
        .local_resources
        .contains_key(&retired_id));
    for message in &queued {
        if let Message::Declare(declaration) = message {
            Primitives::send_declare(&receiver.downgrade(), &mut declaration.clone());
        }
    }
    let discovered = {
        let state = zread!(receiver.0.state);
        (
            state.remote_tokens.len(),
            state.remote_subscribers.len(),
            state.remote_queryables.len(),
        )
    };
    let put = liveliness.try_recv().unwrap();

    // Teardown must also find the declarations that were delivered after retirement.
    source.undeclare_token(1);
    source.undeclare_subscriber(2);
    source.undeclare_queryable(3);
    let deleted = destination
        .recorder()
        .with_messages(|messages| messages[queued.len()..].to_vec());
    for message in deleted {
        if let Message::Declare(mut declaration) = message {
            Primitives::send_declare(&receiver.downgrade(), &mut declaration);
        }
    }
    let remaining = {
        let state = zread!(receiver.0.state);
        (
            state.remote_tokens.len(),
            state.remote_subscribers.len(),
            state.remote_queryables.len(),
        )
    };
    let delete = liveliness.try_recv().unwrap();
    drop(liveliness);
    zwrite!(receiver.0.state).primitives = original;
    destination.face.send_close();
    source.face.send_close();
    receiver.close().await.unwrap();

    assert_eq!(
        discovered,
        (1, 1, 1),
        "queued declarations used a retired receiver mapping"
    );
    assert_eq!(put.unwrap().kind(), SampleKind::Put);
    assert_eq!(remaining, (0, 0, 0));
    assert_eq!(delete.unwrap().kind(), SampleKind::Delete);
}
