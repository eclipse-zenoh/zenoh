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

use zenoh_protocol::core::{Bound, Region, WhatAmI};

use super::{
    try_init_tracing_subscriber, Connection, EstablishedConnection, FaceDef, HarnessBuilder,
};
use crate::key_expr::KeyExpr;

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
    let ke = "k".parse::<KeyExpr>().unwrap();
    ss.declare_token(None, 1, &ke);
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
    let ke = "k".parse::<KeyExpr>().unwrap();
    r0s.declare_token(None, 1, &ke);
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

    let ke = "k".parse::<KeyExpr>().unwrap();

    ss.declare_token(None, 1, &ke);
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

    let ke = "k".parse::<KeyExpr>().unwrap();

    ns.declare_token(None, 1, &ke);
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
