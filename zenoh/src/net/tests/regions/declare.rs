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

//! Tests involving [`zenoh_protocol::network::declare`].

use zenoh_protocol::{
    core::{Bound, Region, WhatAmI, WireExpr},
    network::Mapping,
};

use super::{
    try_init_tracing_subscriber, Connection, EstablishedConnection, FaceDef, HarnessBuilder,
};

/// Tests that the gateway doesn't propagate tokens to the source router region.
#[test]
fn test_against_invalid_token_propagation_to_source_south_router_region() {
    try_init_tracing_subscriber();

    let n = HarnessBuilder::new()
        .mode(WhatAmI::Router)
        .subregions([Region::default_south(WhatAmI::Router), Region::Local])
        .build();

    let s = HarnessBuilder::new()
        .mode(WhatAmI::Router)
        .subregions([Region::Local])
        .build();

    let s_n = Connection {
        a: &s,
        b: &n,
        a2b: FaceDef::default()
            .mode(WhatAmI::Router)
            .remote_bound(Bound::South),
        b2a: FaceDef::default()
            .mode(WhatAmI::Router)
            .region(Region::default_south(WhatAmI::Router)),
    };

    let mut s_n = s_n.establish();
    s_n.bi_fwd();

    let ss = s.new_session();
    ss.declare_token(None, 1, "k");
    s_n.bi_fwd();

    assert!(s_n.b2a.recorder().tokens().is_empty());
}

/// Same as [`test_against_invalid_token_propagation_to_source_south_router_region`] but for north
/// router regions.
#[test]
fn test_against_invalid_token_propagation_to_source_north_router_region() {
    try_init_tracing_subscriber();

    let r0 = HarnessBuilder::new()
        .mode(WhatAmI::Router)
        .subregions([Region::Local])
        .build();
    let r1 = HarnessBuilder::new()
        .mode(WhatAmI::Router)
        .subregions([Region::Local])
        .build();

    let r0_r1 = Connection {
        a: &r0,
        b: &r1,
        a2b: FaceDef::default().mode(WhatAmI::Router),
        b2a: FaceDef::default().mode(WhatAmI::Router),
    };

    let mut r0_r1 = r0_r1.establish();
    r0_r1.bi_fwd();

    let r0s = r0.new_session();
    r0s.declare_token(None, 1, "k");
    r0_r1.bi_fwd();

    assert!(r0_r1.b2a.recorder().tokens().is_empty());
}

#[test]
fn test_multiple_gateways_r2r_token_propagation_upstream() {
    try_init_tracing_subscriber();

    let g1 = HarnessBuilder::new()
        .mode(WhatAmI::Router)
        .subregions([Region::default_south(WhatAmI::Router)])
        .build();
    let g0 = HarnessBuilder::new()
        .mode(WhatAmI::Router)
        .subregions([Region::default_south(WhatAmI::Router)])
        .build();
    let n = HarnessBuilder::new()
        .mode(WhatAmI::Router)
        .subregions([Region::Local])
        .build();
    let s = HarnessBuilder::new()
        .mode(WhatAmI::Router)
        .subregions([Region::Local])
        .build();

    let ss = s.new_session();

    let mut n_g1 = Connection {
        a: &n,
        b: &g1,
        a2b: FaceDef::default().mode(WhatAmI::Router),
        b2a: FaceDef::default().mode(WhatAmI::Router),
    }
    .establish();

    let mut n_g0 = Connection {
        a: &n,
        b: &g0,
        a2b: FaceDef::default().mode(WhatAmI::Router),
        b2a: FaceDef::default().mode(WhatAmI::Router),
    }
    .establish();

    let mut s_g1 = Connection {
        a: &s,
        b: &g1,
        a2b: FaceDef::default()
            .mode(WhatAmI::Router)
            .remote_bound(Bound::South),
        b2a: FaceDef::default()
            .mode(WhatAmI::Router)
            .region(Region::default_south(WhatAmI::Router)),
    }
    .establish();

    let mut s_g0 = Connection {
        a: &s,
        a2b: FaceDef::default()
            .mode(WhatAmI::Router)
            .remote_bound(Bound::South),
        b: &g0,
        b2a: FaceDef::default()
            .mode(WhatAmI::Router)
            .region(Region::default_south(WhatAmI::Router)),
    }
    .establish();

    let mut bi_fwd_all = || {
        EstablishedConnection::bi_fwd_many_unbounded([&mut n_g1, &mut n_g0, &mut s_g1, &mut s_g0])
    };

    bi_fwd_all();

    ss.declare_token(None, 1, "k");
    bi_fwd_all();

    // Only one gateway should forward the token
    assert!((n_g0.b2a.recorder().tokens().len() == 1) ^ (n_g1.b2a.recorder().tokens().len() == 1));

    // The token should not be re-propagated downstream
    assert!(s_g0.b2a.recorder().tokens().is_empty());
    assert!(s_g1.b2a.recorder().tokens().is_empty());

    assert!(n_g0.is_bi_complete());
    assert!(n_g1.is_bi_complete());
    assert!(s_g0.is_bi_complete());
    assert!(s_g1.is_bi_complete());
}

/// **Northbound subscriber aggregation (deterministic).** A router configured with
/// `aggregation.upstream.subscribers = ["example/**"]` folds K downstream subscribers included by
/// the prefix into a SINGLE `example/**` declaration toward its north-bound peer, suppressing the
/// per-key children upstream. Asserts the upstream face records exactly one `DeclareSubscriber`
/// (not K), and that undeclaring every child withdraws exactly one aggregate.
#[test]
fn test_northbound_subscriber_aggregation() {
    try_init_tracing_subscriber();

    let g = HarnessBuilder::new()
        .mode(WhatAmI::Router)
        .subregions([Region::default_south(WhatAmI::Router)])
        .aggregation_upstream_subscribers(["example/**"])
        .build();
    let s = HarnessBuilder::new()
        .mode(WhatAmI::Router)
        .subregions([Region::Local])
        .build();
    let n = HarnessBuilder::new()
        .mode(WhatAmI::Router)
        .subregions([Region::Local])
        .build();

    let ss = s.new_session();

    // s is downstream (south) of g; n is upstream (north) of g.
    let mut s_g = Connection {
        a: &s,
        b: &g,
        a2b: FaceDef::default()
            .mode(WhatAmI::Router)
            .remote_bound(Bound::South),
        b2a: FaceDef::default()
            .mode(WhatAmI::Router)
            .region(Region::default_south(WhatAmI::Router)),
    }
    .establish();
    let mut n_g = Connection {
        a: &n,
        b: &g,
        a2b: FaceDef::default().mode(WhatAmI::Router),
        b2a: FaceDef::default().mode(WhatAmI::Router),
    }
    .establish();

    // Inline (not a closure) so the `&mut` forwards are statement-scoped and the `recorder()` reads
    // between forwarding rounds don't conflict with the mutable borrows.
    EstablishedConnection::bi_fwd_many_unbounded([&mut s_g, &mut n_g]);

    // K downstream subscribers, all included by the configured `example/**` prefix.
    let k = 8u32;
    for j in 0..k {
        ss.declare_subscriber(None, j, format!("example/sensor/{j:04}"));
    }
    EstablishedConnection::bi_fwd_many_unbounded([&mut s_g, &mut n_g]);

    // The upstream face (g -> n) must carry exactly ONE aggregate, not K per-key children.
    let up = n_g.b2a.recorder().subscribers();
    assert_eq!(
        up.len(),
        1,
        "northbound fold must collapse {k} children into one aggregate upstream, got {}",
        up.len()
    );

    // Undeclaring every child withdraws exactly one aggregate upstream.
    for j in 0..k {
        ss.undeclare_subscriber(j);
    }
    EstablishedConnection::bi_fwd_many_unbounded([&mut s_g, &mut n_g]);
    let down = n_g.b2a.recorder().undeclared_subscribers();
    assert_eq!(
        down.len(),
        1,
        "withdrawing all children must withdraw exactly one aggregate upstream, got {}",
        down.len()
    );
}

#[test]
fn test_multiple_gateways_r2r_token_propagation_downstream() {
    try_init_tracing_subscriber();

    let g1 = HarnessBuilder::new()
        .mode(WhatAmI::Router)
        .subregions([Region::default_south(WhatAmI::Router)])
        .build();
    let g0 = HarnessBuilder::new()
        .mode(WhatAmI::Router)
        .subregions([Region::default_south(WhatAmI::Router)])
        .build();
    let n = HarnessBuilder::new()
        .mode(WhatAmI::Router)
        .subregions([Region::Local])
        .build();
    let s = HarnessBuilder::new()
        .mode(WhatAmI::Router)
        .subregions([Region::Local])
        .build();

    let ns = n.new_session();

    let mut n_g1 = Connection {
        a: &n,
        b: &g1,
        a2b: FaceDef::default().mode(WhatAmI::Router),
        b2a: FaceDef::default().mode(WhatAmI::Router),
    }
    .establish();

    let mut n_g0 = Connection {
        a: &n,
        b: &g0,
        a2b: FaceDef::default().mode(WhatAmI::Router),
        b2a: FaceDef::default().mode(WhatAmI::Router),
    }
    .establish();

    let mut s_g1 = Connection {
        a: &s,
        b: &g1,
        a2b: FaceDef::default()
            .mode(WhatAmI::Router)
            .remote_bound(Bound::South),
        b2a: FaceDef::default()
            .mode(WhatAmI::Router)
            .region(Region::default_south(WhatAmI::Router)),
    }
    .establish();

    let mut s_g0 = Connection {
        a: &s,
        a2b: FaceDef::default()
            .mode(WhatAmI::Router)
            .remote_bound(Bound::South),
        b: &g0,
        b2a: FaceDef::default()
            .mode(WhatAmI::Router)
            .region(Region::default_south(WhatAmI::Router)),
    }
    .establish();

    let mut bi_fwd_all = || {
        EstablishedConnection::bi_fwd_many_unbounded([&mut n_g1, &mut n_g0, &mut s_g1, &mut s_g0])
    };

    bi_fwd_all();

    ns.declare_token(None, 1, "k");
    bi_fwd_all();

    // Only one gateway should forward the token
    assert!((s_g0.b2a.recorder().tokens().len() == 1) ^ (s_g1.b2a.recorder().tokens().len() == 1));

    // The token should not be re-propagated upstream
    assert!(n_g0.b2a.recorder().tokens().is_empty());
    assert!(n_g1.b2a.recorder().tokens().is_empty());

    assert!(n_g0.is_bi_complete());
    assert!(n_g1.is_bi_complete());
    assert!(s_g0.is_bi_complete());
    assert!(s_g1.is_bi_complete());
}

#[test]
fn test_client_token_repropagation() {
    let g = HarnessBuilder::new()
        .mode(WhatAmI::Client)
        .subregions([Region::Local, Region::default_south(WhatAmI::Client)])
        .start_runtime(false)
        .build();

    let s0 = g.new_session();

    let s1 = g.new_face(
        FaceDef::default()
            .region(Region::default_south(WhatAmI::Client))
            .mode(WhatAmI::Client),
    );

    s0.declare_token(None, 1, "k/a");
    s0.declare_token(None, 2, "k/b");

    s1.declare_token(None, 1, "k/b");
    s1.declare_token(None, 2, "k/c");

    let n = g.new_face(FaceDef::default().remote_bound(Bound::South));

    assert_eq!(n.recorder().tokens().len(), 3);
}

/// Tests that queryable declaration doesn't result in undeclaration of its duplicates.
#[test]
fn test_duplicate_queryable_undeclaration() {
    try_init_tracing_subscriber();

    let g = HarnessBuilder::new()
        .mode(WhatAmI::Client)
        .subregions([Region::default_south(WhatAmI::Client)])
        .start_runtime(false)
        .build();

    let c0 = g.new_face(
        FaceDef::default()
            .mode(WhatAmI::Client)
            .region(Region::default_south(WhatAmI::Client)),
    );

    c0.declare_keyexpr(None, 5, "k");

    c0.declare_queryable(
        None,
        1,
        WireExpr {
            scope: 5,
            suffix: "".into(),
            mapping: Mapping::Sender,
        },
    );

    c0.declare_queryable(None, 2, "k");

    c0.undeclare_queryable(1);

    let c1 = g.new_face(
        FaceDef::default()
            .mode(WhatAmI::Client)
            .region(Region::default_south(WhatAmI::Client)),
    );

    c1.query(1, "k");

    assert_eq!(c0.recorder().requests().len(), 1);
}
