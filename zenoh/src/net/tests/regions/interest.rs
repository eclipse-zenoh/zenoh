//
// Copyright (c) 2026 ZettaScale Technology
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

//! Tests involving [`zenoh_protocol::network::interest`].

use zenoh_protocol::{
    core::{Bound, Region, WhatAmI},
    network::{
        declare::{DeclareToken, TokenId},
        interest::{InterestMode, InterestOptions},
    },
};

use super::{try_init_tracing_subscriber, Connection, FaceDef, Harness};
use crate::key_expr::KeyExpr;

/// Test that current tokens are re-propagated even if they've already been propagated in future
/// mode.
///
/// ```d2
/// shape: sequence_diagram
///
/// C1 -> R: Interest id=1 mode=F
///
/// C2 -> R.1: DeclareToken iid=None
/// R.1 -> C1: DeclareToken iid=None
///
/// C1 -> R.2: Interest id=2 mode=C
/// R.2 -> C1: DeclareToken iid=2
/// R.2 -> C1: DeclareFinal iid=2
/// ```
#[test]
fn test_current_token_repropagation() {
    try_init_tracing_subscriber();

    let r = Harness::new_router();
    let c1 = Harness::new_client();
    let c2 = Harness::new_client();

    let s1 = c1.new_session();
    let s2 = c1.new_session();

    let r_face = FaceDef::default()
        .region(Region::North)
        .remote_bound(Bound::South)
        .mode(WhatAmI::Router);

    let c_face = FaceDef::default()
        .mode(WhatAmI::Client)
        .region(Region::default_south(WhatAmI::Client));

    let mut c1_r = Connection {
        a: &c1,
        ab: r_face,
        b: &r,
        ba: c_face,
    }
    .establish();
    c1_r.bi_fwd();

    let mut c2_r = Connection {
        a: &c2,
        ab: r_face,
        b: &r,
        ba: c_face,
    }
    .establish();
    c2_r.bi_fwd();

    s1.interest(
        1,
        InterestMode::Future,
        InterestOptions::TOKENS,
        Some("test".try_into().unwrap()),
    );
    c1_r.bi_fwd();

    s2.declare_token(None, 1, "test".try_into().unwrap());
    c2_r.bi_fwd();
    c1_r.bi_fwd();

    assert_eq!(
        s1.recorder().tokens().as_slice(),
        &[DeclareToken {
            id: 1,
            wire_expr: "test".into(),
        }]
    );

    s1.interest(
        1,
        InterestMode::Current,
        InterestOptions::TOKENS,
        Some("test".try_into().unwrap()),
    );
    c1_r.bi_fwd();

    assert_eq!(
        s1.recorder().tokens().as_slice(),
        &[
            DeclareToken {
                id: 1,
                wire_expr: "test".into(),
            },
            DeclareToken {
                id: TokenId::default(),
                wire_expr: "test".into(),
            }
        ]
    );
}

/// Test peer-to-peer interest routing in the presence of unfinalized initial interests.
///
/// This checks for a regression discovered in RMW Zenoh which uses peer mode and sends a
/// [liveliness GET] right after opening a session.
///
/// This issue occured because we did not check that the source of a current tokens interest is
/// south-bound before propagating it to peers with unfinalized initial interests.
///
/// [liveliness GET]:
///     https://github.com/ros2/rmw_zenoh/blob/944a8715f5af6f58e74e318d31510409f69a5e6e/rmw_zenoh_cpp/src/detail/rmw_context_impl_s.cpp#L250-L254
#[test]
fn test_p2p_interest_routing_with_unfinalized_initial_interests() {
    try_init_tracing_subscriber();

    let p = Harness::with_subregions(WhatAmI::Peer, []);

    let p0 = p.new_face(FaceDef::default().mode(WhatAmI::Peer));
    let p1 = p.new_face(FaceDef::default().mode(WhatAmI::Peer));

    p0.interest(42, InterestMode::Current, InterestOptions::TOKENS, None);

    assert!(p0.recorder().interests().is_empty());
    assert!(p1.recorder().interests().is_empty());
}

/// Same as [`test_p2p_interest_routing_with_unfinalized_initial_interests`] but finalizes initial
/// interests before sending the current tokens interest.
#[test]
fn test_p2p_interest_routing_with_finalized_initial_interests() {
    let p = Harness::with_subregions(WhatAmI::Peer, []);

    let p0 = p.new_face(FaceDef::default().mode(WhatAmI::Peer));
    let p1 = p.new_face(FaceDef::default().mode(WhatAmI::Peer));

    p0.declare_final(0);
    p1.declare_final(0);

    p0.interest(42, InterestMode::Current, InterestOptions::TOKENS, None);

    assert!(p0.recorder().interests().is_empty());
    assert!(p1.recorder().interests().is_empty());
}

/// Concurrent current-future interest in a two-region hierarchy.
#[test_case::test_matrix(
    [WhatAmI::Client, WhatAmI::Peer],
    [WhatAmI::Client, WhatAmI::Peer]
)]
fn test_concurrent_current_future_interests(north: WhatAmI, south: WhatAmI) {
    try_init_tracing_subscriber();

    let r = Region::default_south(south);
    let g = Harness::with_subregions(north, [r]);
    let n = g.new_face(FaceDef::default().remote_bound(Bound::South));
    let s0 = g.new_face(FaceDef::default().mode(south).region(r));
    let s1 = g.new_face(FaceDef::default().mode(south).region(r));

    assert_eq!(n.recorder().interests().len(), 0);

    s0.interest(
        42,
        InterestMode::CurrentFuture,
        InterestOptions::KEYEXPRS + InterestOptions::SUBSCRIBERS,
        None,
    );

    s1.interest(
        42,
        InterestMode::CurrentFuture,
        InterestOptions::KEYEXPRS + InterestOptions::SUBSCRIBERS,
        None,
    );

    assert_eq!(n.recorder().interests().len(), 2);

    n.declare_subscriber(Some(42), 1999, &"k".parse::<KeyExpr>().unwrap());

    assert_eq!(s0.recorder().subscribers().len(), 1);
    assert_eq!(s1.recorder().subscribers().len(), 1);
}

/// Re-propagated current-future interest in a two-region hierarchy.
#[test_case::test_matrix(
    [WhatAmI::Client, WhatAmI::Peer],
    [WhatAmI::Client, WhatAmI::Peer]
)]
/// Test that current-future interests are propagated upstresam on open and that downstream
/// declarations with interest id are accepted by the middle gateway even though there is no
/// breadcrumb.
fn test_current_future_interest_propagation_on_open(north: WhatAmI, south: WhatAmI) {
    try_init_tracing_subscriber();

    let r = Region::default_south(south);
    let g = Harness::with_subregions(north, [r]);
    let s = g.new_face(FaceDef::default().mode(south).region(r));

    s.interest(
        42,
        InterestMode::CurrentFuture,
        InterestOptions::QUERYABLES,
        None,
    );

    let n = g.new_face(FaceDef::default().remote_bound(Bound::South));

    assert_eq!(n.recorder().interests().len(), 1);
    assert_eq!(s.recorder().queryables().len(), 0);

    n.declare_queryable(Some(42), 1999, &"k".parse::<KeyExpr>().unwrap());

    assert_eq!(s.recorder().queryables().len(), 1);
}
