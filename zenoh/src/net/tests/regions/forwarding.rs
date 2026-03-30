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

//! Tests involving [`zenoh_protocol::network::push`] & [`zenoh_protocol::network::request`], i.e.
//! the data/forwarding plane.

use zenoh_protocol::{
    core::{Bound, Region, Reliability, WhatAmI, WireExpr},
    network::{
        interest::{InterestMode, InterestOptions},
        Mapping, Push,
    },
};

use super::{try_init_tracing_subscriber, FaceDef, HarnessBuilder};
use crate::net::{
    primitives::Primitives,
    tests::regions::{Connection, EstablishedConnection},
};

/// Tests data routing between peer subregions.
///
/// Here we validate that the dispatcher properly stores different routes per (sub-)region.
///
/// ```d2
/// shape: sequence_diagram
///
/// P0 -> G: Open
/// P1 -> G: Open
///
/// # P0 and P1 belong in two distinct sub-regions of G (S0 and S1 resp.)
///
/// P0 -> G: DeclareSubscriber expr=K
/// P1 -> G: DeclareSubscriber expr=K
///
/// P0 -> G.0: Push expr=K
/// G.0 -> P1: Push expr=K
/// # G caches an empty routes object for data originating in S0
///
/// P1 -> G.1: Push expr=K
/// G.1 -> P0: Push expr=K
/// # G caches an empty routes object for data originating in S1
/// ```
#[test]
fn test_p2p_inter_subregion_data_routing() {
    try_init_tracing_subscriber();

    const S1: Region = Region::South {
        id: 0,
        mode: WhatAmI::Peer,
    };

    const S2: Region = Region::South {
        id: 1,
        mode: WhatAmI::Peer,
    };

    let g = HarnessBuilder::new()
        .mode(WhatAmI::default())
        .subregions([S1, S2])
        .build();

    let p0 = g.new_face(FaceDef::default().mode(WhatAmI::Peer).region(S1));
    let p1 = g.new_face(FaceDef::default().mode(WhatAmI::Peer).region(S2));

    let ke = "k";

    p0.declare_subscriber(None, 1, ke);
    p1.declare_subscriber(None, 1, ke);

    p0.put(ke, vec![42]);
    p1.put(ke, vec![43]);

    assert_eq!(p0.recorder().pushes().len(), 1);
    assert_eq!(p0.recorder().pushes().len(), 1);
}

/// Same as [`test_p2p_inter_subregion_data_routing`] but for queries.
#[test]
fn test_p2p_inter_subregion_query_routing() {
    try_init_tracing_subscriber();

    const S1: Region = Region::South {
        id: 0,
        mode: WhatAmI::Peer,
    };

    const S2: Region = Region::South {
        id: 1,
        mode: WhatAmI::Peer,
    };

    let g = HarnessBuilder::new()
        .mode(WhatAmI::default())
        .subregions([S1, S2])
        .build();

    let p0 = g.new_face(FaceDef::default().mode(WhatAmI::Peer).region(S1));
    let p1 = g.new_face(FaceDef::default().mode(WhatAmI::Peer).region(S2));

    let ke = "k";

    p0.declare_queryable(None, 1, ke);
    p1.declare_queryable(None, 1, ke);

    p0.query(1, ke);
    p1.query(1, ke);

    assert_eq!(p0.recorder().requests().len(), 1);
    assert_eq!(p0.recorder().requests().len(), 1);
}

/// Tests data routing between router subregions.
#[test]
fn test_r2r_inter_subregion_data_routing() {
    try_init_tracing_subscriber();

    const S1: Region = Region::South {
        id: 0,
        mode: WhatAmI::Router,
    };

    const S2: Region = Region::South {
        id: 1,
        mode: WhatAmI::Router,
    };

    let g = HarnessBuilder::new()
        .mode(WhatAmI::default())
        .subregions([S1, S2])
        .build();
    let r0 = HarnessBuilder::new()
        .mode(WhatAmI::Router)
        .subregions([Region::Local])
        .build();
    let r1 = HarnessBuilder::new()
        .mode(WhatAmI::Router)
        .subregions([Region::Local])
        .build();

    let r0s = r0.new_session();
    let r1s = r1.new_session();

    let mut r0_g = Connection {
        a: &r0,
        a2b: FaceDef::default()
            .mode(WhatAmI::Router)
            .remote_bound(Bound::South),
        b: &g,
        b2a: FaceDef::default().mode(WhatAmI::Router).region(S1),
    }
    .establish();

    let mut r1_g = Connection {
        a: &r1,
        a2b: FaceDef::default()
            .mode(WhatAmI::Router)
            .remote_bound(Bound::South),
        b: &g,
        b2a: FaceDef::default().mode(WhatAmI::Router).region(S2),
    }
    .establish();

    let mut bi_fwd_all = || EstablishedConnection::bi_fwd_many_unbounded([&mut r0_g, &mut r1_g]);

    bi_fwd_all();

    let ke = "k";

    r0s.declare_subscriber(None, 1, ke);
    r1s.declare_subscriber(None, 1, ke);
    bi_fwd_all();

    r0s.put(ke, vec![42]);
    bi_fwd_all();

    r1s.put(ke, vec![43]);
    bi_fwd_all();

    assert_eq!(r1s.recorder().pushes().len(), 1);
    assert_eq!(r0s.recorder().pushes().len(), 1);
}

/// Tests query routing between router subregions.
#[test]
fn test_r2r_inter_subregion_query_routing() {
    try_init_tracing_subscriber();

    const S1: Region = Region::South {
        id: 0,
        mode: WhatAmI::Router,
    };

    const S2: Region = Region::South {
        id: 1,
        mode: WhatAmI::Router,
    };

    let g = HarnessBuilder::new()
        .mode(WhatAmI::default())
        .subregions([S1, S2])
        .build();
    let r0 = HarnessBuilder::new()
        .mode(WhatAmI::Router)
        .subregions([Region::Local])
        .build();
    let r1 = HarnessBuilder::new()
        .mode(WhatAmI::Router)
        .subregions([Region::Local])
        .build();

    let r0s = r0.new_session();
    let r1s = r1.new_session();

    let mut r0_g = Connection {
        a: &r0,
        a2b: FaceDef::default()
            .mode(WhatAmI::Router)
            .remote_bound(Bound::South),
        b: &g,
        b2a: FaceDef::default().mode(WhatAmI::Router).region(S1),
    }
    .establish();

    let mut r1_g = Connection {
        a: &r1,
        a2b: FaceDef::default()
            .mode(WhatAmI::Router)
            .remote_bound(Bound::South),
        b: &g,
        b2a: FaceDef::default().mode(WhatAmI::Router).region(S2),
    }
    .establish();

    let mut bi_fwd_all = || EstablishedConnection::bi_fwd_many_unbounded([&mut r0_g, &mut r1_g]);

    bi_fwd_all();

    let ke = "k";

    r0s.declare_queryable(None, 1, ke);
    r1s.declare_queryable(None, 1, ke);
    bi_fwd_all();

    r0s.query(1, ke);
    bi_fwd_all();

    r1s.query(1, ke);
    bi_fwd_all();

    assert_eq!(r1s.recorder().requests().len(), 1);
    assert_eq!(r0s.recorder().requests().len(), 1);
}

/// Tests data routing between client subregions.
#[test]
fn test_c2c_inter_subregion_data_routing() {
    try_init_tracing_subscriber();

    const S1: Region = Region::South {
        id: 0,
        mode: WhatAmI::Client,
    };

    const S2: Region = Region::South {
        id: 1,
        mode: WhatAmI::Client,
    };

    let g = HarnessBuilder::new()
        .mode(WhatAmI::default())
        .subregions([S1, S2])
        .build();

    let c0 = g.new_face(FaceDef::default().mode(WhatAmI::Client).region(S1));
    let c1 = g.new_face(FaceDef::default().mode(WhatAmI::Client).region(S2));

    let ke = "k";

    c0.declare_subscriber(None, 1, ke);
    c1.declare_subscriber(None, 1, ke);

    c0.put(ke, vec![42]);
    c1.put(ke, vec![43]);

    assert_eq!(c0.recorder().pushes().len(), 1);
    assert_eq!(c1.recorder().pushes().len(), 1);
}

/// Tests query routing between client subregions.
#[test]
fn test_c2c_inter_subregion_query_routing() {
    try_init_tracing_subscriber();

    const S1: Region = Region::South {
        id: 0,
        mode: WhatAmI::Client,
    };

    const S2: Region = Region::South {
        id: 1,
        mode: WhatAmI::Client,
    };

    let g = HarnessBuilder::new()
        .mode(WhatAmI::default())
        .subregions([S1, S2])
        .build();

    let c0 = g.new_face(FaceDef::default().mode(WhatAmI::Client).region(S1));
    let c1 = g.new_face(FaceDef::default().mode(WhatAmI::Client).region(S2));

    let ke = "k";

    c0.declare_queryable(None, 1, ke);
    c1.declare_queryable(None, 1, ke);

    c0.query(1, ke);
    c1.query(1, ke);

    assert_eq!(c0.recorder().requests().len(), 1);
    assert_eq!(c1.recorder().requests().len(), 1);
}

#[test]
fn test_multiple_gateways_data_routing_r2p_downstream() {
    try_init_tracing_subscriber();

    let g0 = HarnessBuilder::new()
        .mode(WhatAmI::Router)
        .subregions([Region::default_south(WhatAmI::Peer)])
        .build();
    let g1 = HarnessBuilder::new()
        .mode(WhatAmI::Router)
        .subregions([Region::default_south(WhatAmI::Peer)])
        .build();
    let r = HarnessBuilder::new()
        .mode(WhatAmI::Router)
        .subregions([Region::Local])
        .build();
    let p = HarnessBuilder::new()
        .mode(WhatAmI::Peer)
        .subregions([Region::Local])
        .build();

    let ps = p.new_session();
    let rs = r.new_session();

    let mut r_g0 = Connection {
        a: &r,
        a2b: FaceDef::default().mode(WhatAmI::Router),
        b: &g0,
        b2a: FaceDef::default().mode(WhatAmI::Router),
    }
    .establish();

    let mut r_g1 = Connection {
        a: &r,
        a2b: FaceDef::default().mode(WhatAmI::Router),
        b: &g1,
        b2a: FaceDef::default().mode(WhatAmI::Router),
    }
    .establish();

    let mut p_g0 = Connection {
        a: &p,
        a2b: FaceDef::default()
            .mode(WhatAmI::Router)
            .remote_bound(Bound::South),
        b: &g0,
        b2a: FaceDef::default()
            .mode(WhatAmI::Peer)
            .region(Region::default_south(WhatAmI::Peer)),
    }
    .establish();

    let mut p_g1 = Connection {
        a: &p,
        a2b: FaceDef::default()
            .mode(WhatAmI::Router)
            .remote_bound(Bound::South),
        b: &g1,
        b2a: FaceDef::default()
            .mode(WhatAmI::Peer)
            .region(Region::default_south(WhatAmI::Peer)),
    }
    .establish();

    let mut bi_fwd_all = || {
        EstablishedConnection::bi_fwd_many_unbounded([&mut r_g0, &mut r_g1, &mut p_g0, &mut p_g1])
    };

    bi_fwd_all();

    let ke = "k";

    ps.declare_subscriber(None, 1, ke);
    bi_fwd_all();

    rs.put(ke, vec![42]);
    bi_fwd_all();

    assert_eq!(ps.recorder().pushes().len(), 1);

    assert!(r_g0.is_bi_complete());
    assert!(r_g1.is_bi_complete());
    assert!(p_g0.is_bi_complete());
    assert!(p_g1.is_bi_complete());
}

#[test]
fn test_multiple_gateways_query_routing_r2p_downstream() {
    try_init_tracing_subscriber();

    let g0 = HarnessBuilder::new()
        .mode(WhatAmI::Router)
        .subregions([Region::default_south(WhatAmI::Peer)])
        .build();
    let g1 = HarnessBuilder::new()
        .mode(WhatAmI::Router)
        .subregions([Region::default_south(WhatAmI::Peer)])
        .build();
    let r = HarnessBuilder::new()
        .mode(WhatAmI::Router)
        .subregions([Region::Local])
        .build();
    let p = HarnessBuilder::new()
        .mode(WhatAmI::Peer)
        .subregions([Region::Local])
        .build();

    let ps = p.new_session();
    let rs = r.new_session();

    let mut r_g0 = Connection {
        a: &r,
        a2b: FaceDef::default().mode(WhatAmI::Router),
        b: &g0,
        b2a: FaceDef::default().mode(WhatAmI::Router),
    }
    .establish();

    let mut r_g1 = Connection {
        a: &r,
        a2b: FaceDef::default().mode(WhatAmI::Router),
        b: &g1,
        b2a: FaceDef::default().mode(WhatAmI::Router),
    }
    .establish();

    let mut p_g0 = Connection {
        a: &p,
        a2b: FaceDef::default()
            .mode(WhatAmI::Router)
            .remote_bound(Bound::South),
        b: &g0,
        b2a: FaceDef::default()
            .mode(WhatAmI::Peer)
            .region(Region::default_south(WhatAmI::Peer)),
    }
    .establish();

    let mut p_g1 = Connection {
        a: &p,
        a2b: FaceDef::default()
            .mode(WhatAmI::Router)
            .remote_bound(Bound::South),
        b: &g1,
        b2a: FaceDef::default()
            .mode(WhatAmI::Peer)
            .region(Region::default_south(WhatAmI::Peer)),
    }
    .establish();

    let mut bi_fwd_all = || {
        EstablishedConnection::bi_fwd_many_unbounded([&mut r_g0, &mut r_g1, &mut p_g0, &mut p_g1])
    };

    bi_fwd_all();

    let ke = "k";

    ps.declare_queryable(None, 1, ke);
    bi_fwd_all();

    rs.query(1, ke);
    bi_fwd_all();

    assert_eq!(ps.recorder().requests().len(), 1);

    assert!(r_g0.is_bi_complete());
    assert!(r_g1.is_bi_complete());
    assert!(p_g0.is_bi_complete());
    assert!(p_g1.is_bi_complete());
}

#[test]
fn test_multiple_gateways_data_routing_r2r_downstream() {
    try_init_tracing_subscriber();

    let g0 = HarnessBuilder::new()
        .zid("a0".parse().unwrap())
        .mode(WhatAmI::Router)
        .subregions([Region::default_south(WhatAmI::Router)])
        .build();
    let g1 = HarnessBuilder::new()
        .zid("a1".parse().unwrap())
        .mode(WhatAmI::Router)
        .subregions([Region::default_south(WhatAmI::Router)])
        .build();
    let n = HarnessBuilder::new()
        .zid("a".parse().unwrap())
        .mode(WhatAmI::Router)
        .subregions([Region::Local])
        .build();
    let s = HarnessBuilder::new()
        .zid("b".parse().unwrap())
        .mode(WhatAmI::Router)
        .subregions([Region::Local])
        .build();

    let ss = s.new_session();
    let ns = n.new_session();

    let mut n_g0 = Connection {
        a: &n,
        a2b: FaceDef::default().mode(WhatAmI::Router),
        b: &g0,
        b2a: FaceDef::default().mode(WhatAmI::Router),
    }
    .establish();

    let mut n_g1 = Connection {
        a: &n,
        a2b: FaceDef::default().mode(WhatAmI::Router),
        b: &g1,
        b2a: FaceDef::default().mode(WhatAmI::Router),
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

    let mut s_g1 = Connection {
        a: &s,
        a2b: FaceDef::default()
            .mode(WhatAmI::Router)
            .remote_bound(Bound::South),
        b: &g1,
        b2a: FaceDef::default()
            .mode(WhatAmI::Router)
            .region(Region::default_south(WhatAmI::Router)),
    }
    .establish();

    let mut bi_fwd_all = || {
        EstablishedConnection::bi_fwd_many_unbounded([&mut n_g0, &mut n_g1, &mut s_g0, &mut s_g1])
    };

    bi_fwd_all();

    let ke = "k";

    ss.declare_subscriber(None, 1, ke);
    bi_fwd_all();

    ns.put(ke, vec![0x42]);
    bi_fwd_all();

    assert_eq!(ss.recorder().pushes().len(), 1);

    assert!(n_g0.is_bi_complete());
    assert!(n_g1.is_bi_complete());
    assert!(s_g0.is_bi_complete());
    assert!(s_g1.is_bi_complete());
}

#[test]
fn test_multiple_gateways_query_routing_r2r_downstream() {
    try_init_tracing_subscriber();

    let g0 = HarnessBuilder::new()
        .mode(WhatAmI::Router)
        .subregions([Region::default_south(WhatAmI::Router)])
        .build();
    let g1 = HarnessBuilder::new()
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
    let ns = n.new_session();

    let mut n_g0 = Connection {
        a: &n,
        a2b: FaceDef::default().mode(WhatAmI::Router),
        b: &g0,
        b2a: FaceDef::default().mode(WhatAmI::Router),
    }
    .establish();

    let mut n_g1 = Connection {
        a: &n,
        a2b: FaceDef::default().mode(WhatAmI::Router),
        b: &g1,
        b2a: FaceDef::default().mode(WhatAmI::Router),
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

    let mut s_g1 = Connection {
        a: &s,
        a2b: FaceDef::default()
            .mode(WhatAmI::Router)
            .remote_bound(Bound::South),
        b: &g1,
        b2a: FaceDef::default()
            .mode(WhatAmI::Router)
            .region(Region::default_south(WhatAmI::Router)),
    }
    .establish();

    let mut bi_fwd_all = || {
        EstablishedConnection::bi_fwd_many_unbounded([&mut n_g0, &mut n_g1, &mut s_g0, &mut s_g1])
    };

    bi_fwd_all();

    let ke = "k";

    ss.declare_queryable(None, 1, ke);
    bi_fwd_all();

    ns.query(1, ke);
    bi_fwd_all();

    assert_eq!(ss.recorder().requests().len(), 1);

    assert!(n_g0.is_bi_complete());
    assert!(n_g1.is_bi_complete());
    assert!(s_g0.is_bi_complete());
    assert!(s_g1.is_bi_complete());
}

#[test]
fn test_multiple_gateways_data_routing_p2r_upstream() {
    try_init_tracing_subscriber();

    let g0 = HarnessBuilder::new()
        .mode(WhatAmI::Router)
        .subregions([Region::default_south(WhatAmI::Peer)])
        .build();
    let g1 = HarnessBuilder::new()
        .mode(WhatAmI::Router)
        .subregions([Region::default_south(WhatAmI::Peer)])
        .build();
    let r = HarnessBuilder::new()
        .mode(WhatAmI::Router)
        .subregions([Region::Local])
        .build();
    let p = HarnessBuilder::new()
        .mode(WhatAmI::Peer)
        .subregions([Region::Local])
        .build();

    let ps = p.new_session();
    let rs = r.new_session();

    let mut r_g0 = Connection {
        a: &r,
        a2b: FaceDef::default().mode(WhatAmI::Router),
        b: &g0,
        b2a: FaceDef::default().mode(WhatAmI::Router),
    }
    .establish();

    let mut r_g1 = Connection {
        a: &r,
        a2b: FaceDef::default().mode(WhatAmI::Router),
        b: &g1,
        b2a: FaceDef::default().mode(WhatAmI::Router),
    }
    .establish();

    let mut p_g0 = Connection {
        a: &p,
        a2b: FaceDef::default()
            .mode(WhatAmI::Router)
            .remote_bound(Bound::South),
        b: &g0,
        b2a: FaceDef::default()
            .mode(WhatAmI::Peer)
            .region(Region::default_south(WhatAmI::Peer)),
    }
    .establish();

    let mut p_g1 = Connection {
        a: &p,
        a2b: FaceDef::default()
            .mode(WhatAmI::Router)
            .remote_bound(Bound::South),
        b: &g1,
        b2a: FaceDef::default()
            .mode(WhatAmI::Peer)
            .region(Region::default_south(WhatAmI::Peer)),
    }
    .establish();

    let mut bi_fwd_all = || {
        EstablishedConnection::bi_fwd_many_unbounded([&mut r_g0, &mut r_g1, &mut p_g0, &mut p_g1])
    };

    bi_fwd_all();

    let ke = "k";

    rs.declare_subscriber(None, 1, ke);
    bi_fwd_all();

    // The peer routes upstream unconditionally without having sent an interest.
    ps.put(ke, vec![42]);
    bi_fwd_all();

    assert_eq!(rs.recorder().pushes().len(), 1);

    assert!(r_g0.is_bi_complete());
    assert!(r_g1.is_bi_complete());
    assert!(p_g0.is_bi_complete());
    assert!(p_g1.is_bi_complete());
}

#[test]
fn test_multiple_gateways_data_routing_p2r_upstream_with_interest() {
    try_init_tracing_subscriber();

    let g0 = HarnessBuilder::new()
        .mode(WhatAmI::Router)
        .subregions([Region::default_south(WhatAmI::Peer)])
        .build();
    let g1 = HarnessBuilder::new()
        .mode(WhatAmI::Router)
        .subregions([Region::default_south(WhatAmI::Peer)])
        .build();
    let r = HarnessBuilder::new()
        .mode(WhatAmI::Router)
        .subregions([Region::Local])
        .build();
    let p = HarnessBuilder::new()
        .mode(WhatAmI::Peer)
        .subregions([Region::Local])
        .build();

    let ps = p.new_session();
    let rs = r.new_session();

    let mut r_g0 = Connection {
        a: &r,
        a2b: FaceDef::default().mode(WhatAmI::Router),
        b: &g0,
        b2a: FaceDef::default().mode(WhatAmI::Router),
    }
    .establish();

    let mut r_g1 = Connection {
        a: &r,
        a2b: FaceDef::default().mode(WhatAmI::Router),
        b: &g1,
        b2a: FaceDef::default().mode(WhatAmI::Router),
    }
    .establish();

    let mut p_g0 = Connection {
        a: &p,
        a2b: FaceDef::default()
            .mode(WhatAmI::Router)
            .remote_bound(Bound::South),
        b: &g0,
        b2a: FaceDef::default()
            .mode(WhatAmI::Peer)
            .region(Region::default_south(WhatAmI::Peer)),
    }
    .establish();

    let mut p_g1 = Connection {
        a: &p,
        a2b: FaceDef::default()
            .mode(WhatAmI::Router)
            .remote_bound(Bound::South),
        b: &g1,
        b2a: FaceDef::default()
            .mode(WhatAmI::Peer)
            .region(Region::default_south(WhatAmI::Peer)),
    }
    .establish();

    let mut bi_fwd_all = || {
        EstablishedConnection::bi_fwd_many_unbounded([&mut r_g0, &mut r_g1, &mut p_g0, &mut p_g1])
    };

    bi_fwd_all();

    let ke = "k";

    rs.declare_subscriber(None, 1, ke);
    bi_fwd_all();

    ps.interest(
        1,
        InterestMode::CurrentFuture,
        InterestOptions::SUBSCRIBERS,
        ke,
    );
    bi_fwd_all();

    ps.put(ke, vec![42]);
    bi_fwd_all();

    assert_eq!(rs.recorder().pushes().len(), 1);

    assert!(r_g0.is_bi_complete());
    assert!(r_g1.is_bi_complete());
    assert!(p_g0.is_bi_complete());
    assert!(p_g1.is_bi_complete());
}

#[test]
fn test_multiple_gateways_query_routing_p2r_upstream() {
    try_init_tracing_subscriber();

    let g0 = HarnessBuilder::new()
        .mode(WhatAmI::Router)
        .subregions([Region::default_south(WhatAmI::Peer)])
        .build();
    let g1 = HarnessBuilder::new()
        .mode(WhatAmI::Router)
        .subregions([Region::default_south(WhatAmI::Peer)])
        .build();
    let r = HarnessBuilder::new()
        .mode(WhatAmI::Router)
        .subregions([Region::Local])
        .build();
    let p = HarnessBuilder::new()
        .mode(WhatAmI::Peer)
        .subregions([Region::Local])
        .build();

    let ps = p.new_session();
    let rs = r.new_session();

    let mut r_g0 = Connection {
        a: &r,
        a2b: FaceDef::default().mode(WhatAmI::Router),
        b: &g0,
        b2a: FaceDef::default().mode(WhatAmI::Router),
    }
    .establish();

    let mut r_g1 = Connection {
        a: &r,
        a2b: FaceDef::default().mode(WhatAmI::Router),
        b: &g1,
        b2a: FaceDef::default().mode(WhatAmI::Router),
    }
    .establish();

    let mut p_g0 = Connection {
        a: &p,
        a2b: FaceDef::default()
            .mode(WhatAmI::Router)
            .remote_bound(Bound::South),
        b: &g0,
        b2a: FaceDef::default()
            .mode(WhatAmI::Peer)
            .region(Region::default_south(WhatAmI::Peer)),
    }
    .establish();

    let mut p_g1 = Connection {
        a: &p,
        a2b: FaceDef::default()
            .mode(WhatAmI::Router)
            .remote_bound(Bound::South),
        b: &g1,
        b2a: FaceDef::default()
            .mode(WhatAmI::Peer)
            .region(Region::default_south(WhatAmI::Peer)),
    }
    .establish();

    let mut bi_fwd_all = || {
        EstablishedConnection::bi_fwd_many_unbounded([&mut r_g0, &mut r_g1, &mut p_g0, &mut p_g1])
    };

    bi_fwd_all();

    let ke = "k";

    rs.declare_queryable(None, 1, ke);
    bi_fwd_all();

    // The peer routes the query upstream unconditionally without having sent an interest.
    ps.query(1, ke);
    bi_fwd_all();

    assert!(r_g0.is_bi_complete());
    assert!(r_g1.is_bi_complete());
    assert!(p_g0.is_bi_complete());
    assert!(p_g1.is_bi_complete());

    assert_eq!(rs.recorder().requests().len(), 1);
}

#[test]
fn test_multiple_gateways_query_routing_p2r_upstream_with_interest() {
    try_init_tracing_subscriber();

    let g0 = HarnessBuilder::new()
        .mode(WhatAmI::Router)
        .subregions([Region::default_south(WhatAmI::Peer)])
        .build();
    let g1 = HarnessBuilder::new()
        .mode(WhatAmI::Router)
        .subregions([Region::default_south(WhatAmI::Peer)])
        .build();
    let r = HarnessBuilder::new()
        .mode(WhatAmI::Router)
        .subregions([Region::Local])
        .build();
    let p = HarnessBuilder::new()
        .mode(WhatAmI::Peer)
        .subregions([Region::Local])
        .build();

    let ps = p.new_session();
    let rs = r.new_session();

    let mut r_g0 = Connection {
        a: &r,
        a2b: FaceDef::default().mode(WhatAmI::Router),
        b: &g0,
        b2a: FaceDef::default().mode(WhatAmI::Router),
    }
    .establish();

    let mut r_g1 = Connection {
        a: &r,
        a2b: FaceDef::default().mode(WhatAmI::Router),
        b: &g1,
        b2a: FaceDef::default().mode(WhatAmI::Router),
    }
    .establish();

    let mut p_g0 = Connection {
        a: &p,
        a2b: FaceDef::default()
            .mode(WhatAmI::Router)
            .remote_bound(Bound::South),
        b: &g0,
        b2a: FaceDef::default()
            .mode(WhatAmI::Peer)
            .region(Region::default_south(WhatAmI::Peer)),
    }
    .establish();

    let mut p_g1 = Connection {
        a: &p,
        a2b: FaceDef::default()
            .mode(WhatAmI::Router)
            .remote_bound(Bound::South),
        b: &g1,
        b2a: FaceDef::default()
            .mode(WhatAmI::Peer)
            .region(Region::default_south(WhatAmI::Peer)),
    }
    .establish();

    let mut bi_fwd_all = || {
        EstablishedConnection::bi_fwd_many_unbounded([&mut r_g0, &mut r_g1, &mut p_g0, &mut p_g1])
    };

    bi_fwd_all();

    let ke = "k";

    rs.declare_queryable(None, 1, ke);
    bi_fwd_all();

    ps.interest(
        1,
        InterestMode::CurrentFuture,
        InterestOptions::QUERYABLES,
        ke,
    );
    bi_fwd_all();

    ps.query(1, ke);
    bi_fwd_all();

    assert_eq!(rs.recorder().requests().len(), 1);

    assert!(r_g0.is_bi_complete());
    assert!(r_g1.is_bi_complete());
    assert!(p_g0.is_bi_complete());
    assert!(p_g1.is_bi_complete());
}

#[test]
fn test_multiple_gateways_data_routing_r2r_upstream() {
    try_init_tracing_subscriber();

    let g0 = HarnessBuilder::new()
        .mode(WhatAmI::Router)
        .subregions([Region::default_south(WhatAmI::Router)])
        .build();
    let g1 = HarnessBuilder::new()
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
    let ss = s.new_session();

    let mut n_g0 = Connection {
        a: &n,
        a2b: FaceDef::default().mode(WhatAmI::Router),
        b: &g0,
        b2a: FaceDef::default().mode(WhatAmI::Router),
    }
    .establish();

    let mut n_g1 = Connection {
        a: &n,
        a2b: FaceDef::default().mode(WhatAmI::Router),
        b: &g1,
        b2a: FaceDef::default().mode(WhatAmI::Router),
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

    let mut s_g1 = Connection {
        a: &s,
        a2b: FaceDef::default()
            .mode(WhatAmI::Router)
            .remote_bound(Bound::South),
        b: &g1,
        b2a: FaceDef::default()
            .mode(WhatAmI::Router)
            .region(Region::default_south(WhatAmI::Router)),
    }
    .establish();

    let mut bi_fwd_all = || {
        EstablishedConnection::bi_fwd_many_unbounded([&mut n_g0, &mut n_g1, &mut s_g0, &mut s_g1])
    };

    bi_fwd_all();

    let ke = "k";

    ns.declare_subscriber(None, 1, ke);
    bi_fwd_all();

    ss.put(ke, vec![0x42]);
    bi_fwd_all();

    assert_eq!(ns.recorder().pushes().len(), 1);

    assert!(n_g0.is_bi_complete());
    assert!(n_g1.is_bi_complete());
    assert!(s_g0.is_bi_complete());
    assert!(s_g1.is_bi_complete());
}

#[test]
fn test_multiple_gateways_query_routing_r2r_upstream() {
    try_init_tracing_subscriber();

    let g0 = HarnessBuilder::new()
        .mode(WhatAmI::Router)
        .subregions([Region::default_south(WhatAmI::Router)])
        .build();
    let g1 = HarnessBuilder::new()
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
    let ss = s.new_session();

    let mut n_g0 = Connection {
        a: &n,
        a2b: FaceDef::default().mode(WhatAmI::Router),
        b: &g0,
        b2a: FaceDef::default().mode(WhatAmI::Router),
    }
    .establish();

    let mut n_g1 = Connection {
        a: &n,
        a2b: FaceDef::default().mode(WhatAmI::Router),
        b: &g1,
        b2a: FaceDef::default().mode(WhatAmI::Router),
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

    let mut s_g1 = Connection {
        a: &s,
        a2b: FaceDef::default()
            .mode(WhatAmI::Router)
            .remote_bound(Bound::South),
        b: &g1,
        b2a: FaceDef::default()
            .mode(WhatAmI::Router)
            .region(Region::default_south(WhatAmI::Router)),
    }
    .establish();

    let mut bi_fwd_all = || {
        EstablishedConnection::bi_fwd_many_unbounded([&mut n_g0, &mut n_g1, &mut s_g0, &mut s_g1])
    };

    bi_fwd_all();

    let ke = "k";

    ns.declare_queryable(None, 1, ke);
    bi_fwd_all();

    ss.query(1, ke);
    bi_fwd_all();

    assert!(n_g0.is_bi_complete());
    assert!(n_g1.is_bi_complete());
    assert!(s_g0.is_bi_complete());
    assert!(s_g1.is_bi_complete());

    assert_eq!(ns.recorder().requests().len(), 1);
}

#[test]
fn test_multiple_gateways_data_routing_p2p_downstream() {
    try_init_tracing_subscriber();

    let g0 = HarnessBuilder::new()
        .mode(WhatAmI::Peer)
        .subregions([Region::default_south(WhatAmI::Peer)])
        .build();
    let g1 = HarnessBuilder::new()
        .mode(WhatAmI::Peer)
        .subregions([Region::default_south(WhatAmI::Peer)])
        .build();
    let n = HarnessBuilder::new()
        .mode(WhatAmI::Peer)
        .subregions([Region::Local])
        .build();
    let s = HarnessBuilder::new()
        .mode(WhatAmI::Peer)
        .subregions([Region::Local])
        .build();

    let ss = s.new_session();
    let ns = n.new_session();

    let mut n_g0 = Connection {
        a: &n,
        a2b: FaceDef::default().mode(WhatAmI::Peer),
        b: &g0,
        b2a: FaceDef::default().mode(WhatAmI::Peer),
    }
    .establish();

    let mut n_g1 = Connection {
        a: &n,
        a2b: FaceDef::default().mode(WhatAmI::Peer),
        b: &g1,
        b2a: FaceDef::default().mode(WhatAmI::Peer),
    }
    .establish();

    let mut s_g0 = Connection {
        a: &s,
        a2b: FaceDef::default()
            .mode(WhatAmI::Peer)
            .remote_bound(Bound::South),
        b: &g0,
        b2a: FaceDef::default()
            .mode(WhatAmI::Peer)
            .region(Region::default_south(WhatAmI::Peer)),
    }
    .establish();

    let mut s_g1 = Connection {
        a: &s,
        a2b: FaceDef::default()
            .mode(WhatAmI::Peer)
            .remote_bound(Bound::South),
        b: &g1,
        b2a: FaceDef::default()
            .mode(WhatAmI::Peer)
            .region(Region::default_south(WhatAmI::Peer)),
    }
    .establish();

    let mut g0_g1 = Connection {
        a: &g0,
        a2b: FaceDef::default().mode(WhatAmI::Peer),
        b: &g1,
        b2a: FaceDef::default().mode(WhatAmI::Peer),
    }
    .establish();

    let mut bi_fwd_all = || {
        EstablishedConnection::bi_fwd_many_unbounded([
            &mut n_g0, &mut n_g1, &mut s_g0, &mut s_g1, &mut g0_g1,
        ])
    };

    bi_fwd_all();

    let ke = "k";

    ss.declare_subscriber(None, 1, ke);
    bi_fwd_all();

    ns.put(ke, vec![0x42]);
    bi_fwd_all();

    assert_eq!(ss.recorder().pushes().len(), 1);

    assert!(n_g0.is_bi_complete());
    assert!(n_g1.is_bi_complete());
    assert!(s_g0.is_bi_complete());
    assert!(s_g1.is_bi_complete());
    assert!(g0_g1.is_bi_complete());
}

#[test]
fn test_multiple_gateways_query_routing_p2p_downstream() {
    try_init_tracing_subscriber();

    let g0 = HarnessBuilder::new()
        .mode(WhatAmI::Peer)
        .subregions([Region::default_south(WhatAmI::Peer)])
        .build();
    let g1 = HarnessBuilder::new()
        .mode(WhatAmI::Peer)
        .subregions([Region::default_south(WhatAmI::Peer)])
        .build();
    let n = HarnessBuilder::new()
        .mode(WhatAmI::Peer)
        .subregions([Region::Local])
        .build();
    let s = HarnessBuilder::new()
        .mode(WhatAmI::Peer)
        .subregions([Region::Local])
        .build();

    let ss = s.new_session();
    let ns = n.new_session();

    let mut n_g0 = Connection {
        a: &n,
        a2b: FaceDef::default().mode(WhatAmI::Peer),
        b: &g0,
        b2a: FaceDef::default().mode(WhatAmI::Peer),
    }
    .establish();

    let mut n_g1 = Connection {
        a: &n,
        a2b: FaceDef::default().mode(WhatAmI::Peer),
        b: &g1,
        b2a: FaceDef::default().mode(WhatAmI::Peer),
    }
    .establish();

    let mut s_g0 = Connection {
        a: &s,
        a2b: FaceDef::default()
            .mode(WhatAmI::Peer)
            .remote_bound(Bound::South),
        b: &g0,
        b2a: FaceDef::default()
            .mode(WhatAmI::Peer)
            .region(Region::default_south(WhatAmI::Peer)),
    }
    .establish();

    let mut s_g1 = Connection {
        a: &s,
        a2b: FaceDef::default()
            .mode(WhatAmI::Peer)
            .remote_bound(Bound::South),
        b: &g1,
        b2a: FaceDef::default()
            .mode(WhatAmI::Peer)
            .region(Region::default_south(WhatAmI::Peer)),
    }
    .establish();

    let mut g0_g1 = Connection {
        a: &g0,
        a2b: FaceDef::default().mode(WhatAmI::Peer),
        b: &g1,
        b2a: FaceDef::default().mode(WhatAmI::Peer),
    }
    .establish();

    let mut bi_fwd_all = || {
        EstablishedConnection::bi_fwd_many_unbounded([
            &mut n_g0, &mut n_g1, &mut s_g0, &mut s_g1, &mut g0_g1,
        ])
    };

    bi_fwd_all();

    let ke = "k";

    ss.declare_queryable(None, 1, ke);
    bi_fwd_all();

    ns.query(1, ke);
    bi_fwd_all();

    assert_eq!(ss.recorder().requests().len(), 1);

    assert!(n_g0.is_bi_complete());
    assert!(n_g1.is_bi_complete());
    assert!(s_g0.is_bi_complete());
    assert!(s_g1.is_bi_complete());
    assert!(g0_g1.is_bi_complete());
}

#[test]
fn test_multiple_gateways_data_routing_p2p_upstream() {
    try_init_tracing_subscriber();

    let g0 = HarnessBuilder::new()
        .mode(WhatAmI::Peer)
        .subregions([Region::default_south(WhatAmI::Peer)])
        .build();
    let g1 = HarnessBuilder::new()
        .mode(WhatAmI::Peer)
        .subregions([Region::default_south(WhatAmI::Peer)])
        .build();
    let n = HarnessBuilder::new()
        .mode(WhatAmI::Peer)
        .subregions([Region::Local])
        .build();
    let s = HarnessBuilder::new()
        .mode(WhatAmI::Peer)
        .subregions([Region::Local])
        .build();

    let ns = n.new_session();
    let ss = s.new_session();

    let mut n_g0 = Connection {
        a: &n,
        a2b: FaceDef::default().mode(WhatAmI::Peer),
        b: &g0,
        b2a: FaceDef::default().mode(WhatAmI::Peer),
    }
    .establish();

    let mut n_g1 = Connection {
        a: &n,
        a2b: FaceDef::default().mode(WhatAmI::Peer),
        b: &g1,
        b2a: FaceDef::default().mode(WhatAmI::Peer),
    }
    .establish();

    let mut s_g0 = Connection {
        a: &s,
        a2b: FaceDef::default()
            .mode(WhatAmI::Peer)
            .remote_bound(Bound::South),
        b: &g0,
        b2a: FaceDef::default()
            .mode(WhatAmI::Peer)
            .region(Region::default_south(WhatAmI::Peer)),
    }
    .establish();

    let mut s_g1 = Connection {
        a: &s,
        a2b: FaceDef::default()
            .mode(WhatAmI::Peer)
            .remote_bound(Bound::South),
        b: &g1,
        b2a: FaceDef::default()
            .mode(WhatAmI::Peer)
            .region(Region::default_south(WhatAmI::Peer)),
    }
    .establish();

    let mut g0_g1 = Connection {
        a: &g0,
        a2b: FaceDef::default().mode(WhatAmI::Peer),
        b: &g1,
        b2a: FaceDef::default().mode(WhatAmI::Peer),
    }
    .establish();

    let mut bi_fwd_all = || {
        EstablishedConnection::bi_fwd_many_unbounded([
            &mut n_g0, &mut n_g1, &mut s_g0, &mut s_g1, &mut g0_g1,
        ])
    };

    bi_fwd_all();

    let ke = "k";

    ns.declare_subscriber(None, 1, ke);
    bi_fwd_all();

    ss.put(ke, vec![0x42]);
    bi_fwd_all();

    assert!(n_g0.is_bi_complete());
    assert!(n_g1.is_bi_complete());
    assert!(s_g0.is_bi_complete());
    assert!(s_g1.is_bi_complete());
    assert!(g0_g1.is_bi_complete());

    assert_eq!(ns.recorder().pushes().len(), 1);
}

#[test]
fn test_multiple_gateways_data_routing_p2p_upstream_with_interest() {
    try_init_tracing_subscriber();

    let g0 = HarnessBuilder::new()
        .mode(WhatAmI::Peer)
        .subregions([Region::default_south(WhatAmI::Peer)])
        .build();
    let g1 = HarnessBuilder::new()
        .mode(WhatAmI::Peer)
        .subregions([Region::default_south(WhatAmI::Peer)])
        .build();
    let n = HarnessBuilder::new()
        .mode(WhatAmI::Peer)
        .subregions([Region::Local])
        .build();
    let s = HarnessBuilder::new()
        .mode(WhatAmI::Peer)
        .subregions([Region::Local])
        .build();

    let ns = n.new_session();
    let ss = s.new_session();

    let mut n_g0 = Connection {
        a: &n,
        a2b: FaceDef::default().mode(WhatAmI::Peer),
        b: &g0,
        b2a: FaceDef::default().mode(WhatAmI::Peer),
    }
    .establish();

    let mut n_g1 = Connection {
        a: &n,
        a2b: FaceDef::default().mode(WhatAmI::Peer),
        b: &g1,
        b2a: FaceDef::default().mode(WhatAmI::Peer),
    }
    .establish();

    let mut s_g0 = Connection {
        a: &s,
        a2b: FaceDef::default()
            .mode(WhatAmI::Peer)
            .remote_bound(Bound::South),
        b: &g0,
        b2a: FaceDef::default()
            .mode(WhatAmI::Peer)
            .region(Region::default_south(WhatAmI::Peer)),
    }
    .establish();

    let mut s_g1 = Connection {
        a: &s,
        a2b: FaceDef::default()
            .mode(WhatAmI::Peer)
            .remote_bound(Bound::South),
        b: &g1,
        b2a: FaceDef::default()
            .mode(WhatAmI::Peer)
            .region(Region::default_south(WhatAmI::Peer)),
    }
    .establish();

    let mut g0_g1 = Connection {
        a: &g0,
        a2b: FaceDef::default().mode(WhatAmI::Peer),
        b: &g1,
        b2a: FaceDef::default().mode(WhatAmI::Peer),
    }
    .establish();

    let mut bi_fwd_all = || {
        EstablishedConnection::bi_fwd_many_unbounded([
            &mut n_g0, &mut n_g1, &mut s_g0, &mut s_g1, &mut g0_g1,
        ])
    };

    bi_fwd_all();

    let ke = "k";

    ns.declare_subscriber(None, 1, ke);
    bi_fwd_all();

    ss.interest(
        1,
        InterestMode::CurrentFuture,
        InterestOptions::SUBSCRIBERS,
        ke,
    );
    bi_fwd_all();

    ss.put(ke, vec![0x42]);
    bi_fwd_all();

    assert_eq!(ns.recorder().pushes().len(), 1);

    assert!(n_g0.is_bi_complete());
    assert!(n_g1.is_bi_complete());
    assert!(s_g0.is_bi_complete());
    assert!(s_g1.is_bi_complete());
    assert!(g0_g1.is_bi_complete());
}

#[test]
fn test_multiple_gateways_query_routing_p2p_upstream() {
    try_init_tracing_subscriber();

    let g0 = HarnessBuilder::new()
        .mode(WhatAmI::Peer)
        .subregions([Region::default_south(WhatAmI::Peer)])
        .build();
    let g1 = HarnessBuilder::new()
        .mode(WhatAmI::Peer)
        .subregions([Region::default_south(WhatAmI::Peer)])
        .build();
    let n = HarnessBuilder::new()
        .mode(WhatAmI::Peer)
        .subregions([Region::Local])
        .build();
    let s = HarnessBuilder::new()
        .mode(WhatAmI::Peer)
        .subregions([Region::Local])
        .build();

    let ns = n.new_session();
    let ss = s.new_session();

    let mut n_g0 = Connection {
        a: &n,
        a2b: FaceDef::default().mode(WhatAmI::Peer),
        b: &g0,
        b2a: FaceDef::default().mode(WhatAmI::Peer),
    }
    .establish();

    let mut n_g1 = Connection {
        a: &n,
        a2b: FaceDef::default().mode(WhatAmI::Peer),
        b: &g1,
        b2a: FaceDef::default().mode(WhatAmI::Peer),
    }
    .establish();

    let mut s_g0 = Connection {
        a: &s,
        a2b: FaceDef::default()
            .mode(WhatAmI::Peer)
            .remote_bound(Bound::South),
        b: &g0,
        b2a: FaceDef::default()
            .mode(WhatAmI::Peer)
            .region(Region::default_south(WhatAmI::Peer)),
    }
    .establish();

    let mut s_g1 = Connection {
        a: &s,
        a2b: FaceDef::default()
            .mode(WhatAmI::Peer)
            .remote_bound(Bound::South),
        b: &g1,
        b2a: FaceDef::default()
            .mode(WhatAmI::Peer)
            .region(Region::default_south(WhatAmI::Peer)),
    }
    .establish();

    let mut g0_g1 = Connection {
        a: &g0,
        a2b: FaceDef::default().mode(WhatAmI::Peer),
        b: &g1,
        b2a: FaceDef::default().mode(WhatAmI::Peer),
    }
    .establish();

    let mut bi_fwd_all = || {
        EstablishedConnection::bi_fwd_many_unbounded([
            &mut n_g0, &mut n_g1, &mut s_g0, &mut s_g1, &mut g0_g1,
        ])
    };

    bi_fwd_all();

    let ke = "k";

    ns.declare_queryable(None, 1, ke);
    bi_fwd_all();

    ss.query(1, ke);
    bi_fwd_all();

    assert_eq!(ns.recorder().requests().len(), 1);

    assert!(n_g0.is_bi_complete());
    assert!(n_g1.is_bi_complete());
    assert!(s_g0.is_bi_complete());
    assert!(s_g1.is_bi_complete());
    assert!(g0_g1.is_bi_complete());
}

#[test]
fn test_multiple_gateways_query_routing_p2p_upstream_with_interest() {
    try_init_tracing_subscriber();

    let g0 = HarnessBuilder::new()
        .mode(WhatAmI::Peer)
        .subregions([Region::default_south(WhatAmI::Peer)])
        .build();
    let g1 = HarnessBuilder::new()
        .mode(WhatAmI::Peer)
        .subregions([Region::default_south(WhatAmI::Peer)])
        .build();
    let n = HarnessBuilder::new()
        .mode(WhatAmI::Peer)
        .subregions([Region::Local])
        .build();
    let s = HarnessBuilder::new()
        .mode(WhatAmI::Peer)
        .subregions([Region::Local])
        .build();

    let ns = n.new_session();
    let ss = s.new_session();

    let mut n_g0 = Connection {
        a: &n,
        a2b: FaceDef::default().mode(WhatAmI::Peer),
        b: &g0,
        b2a: FaceDef::default().mode(WhatAmI::Peer),
    }
    .establish();

    let mut n_g1 = Connection {
        a: &n,
        a2b: FaceDef::default().mode(WhatAmI::Peer),
        b: &g1,
        b2a: FaceDef::default().mode(WhatAmI::Peer),
    }
    .establish();

    let mut s_g0 = Connection {
        a: &s,
        a2b: FaceDef::default()
            .mode(WhatAmI::Peer)
            .remote_bound(Bound::South),
        b: &g0,
        b2a: FaceDef::default()
            .mode(WhatAmI::Peer)
            .region(Region::default_south(WhatAmI::Peer)),
    }
    .establish();

    let mut s_g1 = Connection {
        a: &s,
        a2b: FaceDef::default()
            .mode(WhatAmI::Peer)
            .remote_bound(Bound::South),
        b: &g1,
        b2a: FaceDef::default()
            .mode(WhatAmI::Peer)
            .region(Region::default_south(WhatAmI::Peer)),
    }
    .establish();

    let mut g0_g1 = Connection {
        a: &g0,
        a2b: FaceDef::default().mode(WhatAmI::Peer),
        b: &g1,
        b2a: FaceDef::default().mode(WhatAmI::Peer),
    }
    .establish();

    let mut bi_fwd_all = || {
        EstablishedConnection::bi_fwd_many_unbounded([
            &mut n_g0, &mut n_g1, &mut s_g0, &mut s_g1, &mut g0_g1,
        ])
    };

    bi_fwd_all();

    let ke = "k";

    ns.declare_queryable(None, 1, ke);
    bi_fwd_all();

    ss.interest(
        1,
        InterestMode::CurrentFuture,
        InterestOptions::QUERYABLES,
        ke,
    );
    bi_fwd_all();

    ss.query(1, ke);
    bi_fwd_all();

    assert_eq!(ns.recorder().requests().len(), 1);

    assert!(n_g0.is_bi_complete());
    assert!(n_g1.is_bi_complete());
    assert!(s_g0.is_bi_complete());
    assert!(s_g1.is_bi_complete());
    assert!(g0_g1.is_bi_complete());
}

/// Regression test for `inter_region_filter` using `src_zid` instead of `fwd_zid` when
/// `src.bound() == South`.
///
/// ```text
///           n            ← subscriber ns
///         /   \
///       g1     g2        ← two router gateways, each with a South{Router} sub-region
///         \   /
///           m            ← intermediate south-region router (Region::Local)
/// ```
#[test]
fn test_multiple_gateways_data_routing_r2r_upstream_gateway_source() {
    try_init_tracing_subscriber();

    // g1 needs Region::Local so it can host a session (g1s).
    let g1 = HarnessBuilder::new()
        .mode(WhatAmI::Router)
        .subregions([Region::default_south(WhatAmI::Router), Region::Local])
        .build();
    let g2 = HarnessBuilder::new()
        .mode(WhatAmI::Router)
        .subregions([Region::default_south(WhatAmI::Router)])
        .build();
    let n = HarnessBuilder::new()
        .mode(WhatAmI::Router)
        .subregions([Region::Local])
        .build();
    let m = HarnessBuilder::new()
        .mode(WhatAmI::Router)
        .subregions([Region::Local])
        .build();

    let ns = n.new_session();
    let g1s = g1.new_session();

    // North side: n connects to both gateways.
    let mut n_g1 = Connection {
        a: &n,
        a2b: FaceDef::default().mode(WhatAmI::Router),
        b: &g1,
        b2a: FaceDef::default().mode(WhatAmI::Router),
    }
    .establish();

    let mut n_g2 = Connection {
        a: &n,
        a2b: FaceDef::default().mode(WhatAmI::Router),
        b: &g2,
        b2a: FaceDef::default().mode(WhatAmI::Router),
    }
    .establish();

    // South side: m connects to both gateways, bridging their south sub-regions.
    let mut m_g1 = Connection {
        a: &m,
        a2b: FaceDef::default()
            .mode(WhatAmI::Router)
            .remote_bound(Bound::South),
        b: &g1,
        b2a: FaceDef::default()
            .mode(WhatAmI::Router)
            .region(Region::default_south(WhatAmI::Router)),
    }
    .establish();

    let mut m_g2 = Connection {
        a: &m,
        a2b: FaceDef::default()
            .mode(WhatAmI::Router)
            .remote_bound(Bound::South),
        b: &g2,
        b2a: FaceDef::default()
            .mode(WhatAmI::Router)
            .region(Region::default_south(WhatAmI::Router)),
    }
    .establish();

    let mut bi_fwd_all = || {
        EstablishedConnection::bi_fwd_many_unbounded([&mut n_g1, &mut n_g2, &mut m_g1, &mut m_g2])
    };

    bi_fwd_all();

    let ke = "k";

    ns.declare_subscriber(None, 1, ke);
    bi_fwd_all();

    g1s.put(ke, vec![0x42]);
    bi_fwd_all();

    assert_eq!(ns.recorder().pushes().len(), 1);

    assert!(n_g1.is_bi_complete());
    assert!(n_g2.is_bi_complete());
    assert!(m_g1.is_bi_complete());
    assert!(m_g2.is_bi_complete());
}

/// Same as [`multiple_gateways_data_routing_r2r_upstream_gateway_source`] but for queries.
#[test]
fn test_multiple_gateways_query_routing_r2r_upstream_gateway_source() {
    try_init_tracing_subscriber();

    let g1 = HarnessBuilder::new()
        .mode(WhatAmI::Router)
        .subregions([Region::default_south(WhatAmI::Router), Region::Local])
        .build();
    let g2 = HarnessBuilder::new()
        .mode(WhatAmI::Router)
        .subregions([Region::default_south(WhatAmI::Router)])
        .build();
    let n = HarnessBuilder::new()
        .mode(WhatAmI::Router)
        .subregions([Region::Local])
        .build();
    let m = HarnessBuilder::new()
        .mode(WhatAmI::Router)
        .subregions([Region::Local])
        .build();

    let ns = n.new_session();
    let g1s = g1.new_session();

    let mut n_g1 = Connection {
        a: &n,
        a2b: FaceDef::default().mode(WhatAmI::Router),
        b: &g1,
        b2a: FaceDef::default().mode(WhatAmI::Router),
    }
    .establish();

    let mut n_g2 = Connection {
        a: &n,
        a2b: FaceDef::default().mode(WhatAmI::Router),
        b: &g2,
        b2a: FaceDef::default().mode(WhatAmI::Router),
    }
    .establish();

    let mut m_g1 = Connection {
        a: &m,
        a2b: FaceDef::default()
            .mode(WhatAmI::Router)
            .remote_bound(Bound::South),
        b: &g1,
        b2a: FaceDef::default()
            .mode(WhatAmI::Router)
            .region(Region::default_south(WhatAmI::Router)),
    }
    .establish();

    let mut m_g2 = Connection {
        a: &m,
        a2b: FaceDef::default()
            .mode(WhatAmI::Router)
            .remote_bound(Bound::South),
        b: &g2,
        b2a: FaceDef::default()
            .mode(WhatAmI::Router)
            .region(Region::default_south(WhatAmI::Router)),
    }
    .establish();

    let mut bi_fwd_all = || {
        EstablishedConnection::bi_fwd_many_unbounded([&mut n_g1, &mut n_g2, &mut m_g1, &mut m_g2])
    };

    bi_fwd_all();

    let ke = "k";

    ns.declare_queryable(None, 1, ke);
    bi_fwd_all();

    g1s.query(1, ke);
    bi_fwd_all();

    assert_eq!(ns.recorder().requests().len(), 1);

    assert!(n_g1.is_bi_complete());
    assert!(n_g2.is_bi_complete());
    assert!(m_g1.is_bi_complete());
    assert!(m_g2.is_bi_complete());
}

/// In [route_data], we use [send_push_consume] consume to decide whether we may modify the source
/// message in place for forwarding or not. This test ensures that the `consume` API is respected.
///
/// [route_data]: crate::net::routing::dispatcher::pubsub::route_data
/// [send_push_consume]: crate::net::routing::dispatcher::face::Face
#[test]
fn test_push_message_consumption() {
    let g = HarnessBuilder::new()
        .mode(WhatAmI::Client) // Avoid the need for a runtime
        .subregions([Region::Local])
        .start_runtime(false)
        .build();
    let s1 = g.new_session();
    let s2 = g.new_session();

    s2.declare_subscriber(None, 3, "k/*");
    s2.declare_keyexpr(None, 5, "k/0");

    let mut push = Push::from(vec![42]);
    push.wire_expr = "k/0".into();
    let push_clone = push.clone();

    // Here the wire expr is non-scoped
    s1.face
        .send_push_consume(&mut push, Reliability::default(), false);

    assert_eq!(
        s2.recorder().pushes().len(),
        1,
        "s2 should receive exactly one publication"
    );

    assert_eq!(
        s2.recorder().pushes()[0].wire_expr,
        WireExpr {
            scope: 5,
            suffix: "".into(),
            mapping: Mapping::Receiver
        },
        "s2's received publication should be scoped"
    );

    assert_eq!(
        push, push_clone,
        "the push message should not be modified as `consume` is false"
    );
}
