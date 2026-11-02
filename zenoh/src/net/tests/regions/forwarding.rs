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
        declare::queryable::ext::QueryableInfoType,
        ext::{NodeIdType, QoSType},
        interest::{InterestMode, InterestOptions},
        request::ext::QueryTarget,
        Mapping, Push, Request,
    },
    zenoh::{Query, RequestBody},
};

use super::{try_init_tracing_subscriber, FaceDef, HarnessBuilder};
use crate::net::{
    primitives::Primitives,
    tests::regions::{Connection, EstablishedConnection},
};

fn query_same_key_completeness(complete_flags: &[bool], target: QueryTarget) -> usize {
    try_init_tracing_subscriber();

    let client_region = Region::default_south(WhatAmI::Client);
    let gateway = HarnessBuilder::new()
        .mode(WhatAmI::Router)
        .subregions([Region::Local, client_region])
        .build();
    let session = gateway.new_session();
    let client = gateway.new_face(
        FaceDef::default()
            .mode(WhatAmI::Client)
            .region(client_region),
    );

    for (id, &complete) in complete_flags.iter().enumerate() {
        session.declare_queryable(
            None,
            id as u32 + 1,
            "test/queryable/mixed_completeness/**",
            QueryableInfoType {
                complete,
                distance: 0,
            },
        );
    }

    session.recorder().clear();
    client.face.send_request(&mut Request {
        id: 1,
        wire_expr: "test/queryable/mixed_completeness/probe".into(),
        ext_qos: QoSType::DEFAULT,
        ext_tstamp: None,
        ext_nodeid: NodeIdType::DEFAULT,
        ext_target: target,
        ext_budget: None,
        ext_timeout: None,
        ext_ts_stack: None,
        payload: RequestBody::Query(Query::default()),
    });
    let requests = session.recorder().requests();
    for request in &requests {
        assert_eq!(request.ext_target, target);
    }
    requests.len()
}

#[test]
fn test_queryable_mixed_completeness_single_complete() {
    assert_eq!(
        query_same_key_completeness(&[true], QueryTarget::AllComplete),
        1
    );
}

#[test]
fn test_queryable_mixed_completeness_all_target() {
    assert_eq!(
        query_same_key_completeness(&[true, false], QueryTarget::All),
        1
    );
}

/// Regression for #2614: an incomplete declaration must not hide a complete
/// queryable on the same session/key from a remote AllComplete query.
///
/// Both queryables are injected on one local session face of a router gateway.
/// Region::Local selects hat/broker/queries.rs::register_queryable, not the
/// client, peer, or router registration implementations. A mock remote client
/// sends the query; no real transport connections or sleeps are needed.
///
/// The assertion checks forwarding to the local session face, not individual
/// queryable callbacks or replies. Filtering the incomplete queryable within
/// the session is outside this routing test's scope.
#[test]
fn test_queryable_mixed_completeness_all_complete() {
    assert_eq!(
        query_same_key_completeness(&[true, false], QueryTarget::AllComplete),
        1,
        "AllComplete must still be forwarded to the session with a complete queryable"
    );
}

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

    p0.declare_queryable(None, 1, ke, QueryableInfoType::DEFAULT);
    p1.declare_queryable(None, 1, ke, QueryableInfoType::DEFAULT);

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

    r0s.declare_queryable(None, 1, ke, QueryableInfoType::DEFAULT);
    r1s.declare_queryable(None, 1, ke, QueryableInfoType::DEFAULT);
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

    c0.declare_queryable(None, 1, ke, QueryableInfoType::DEFAULT);
    c1.declare_queryable(None, 1, ke, QueryableInfoType::DEFAULT);

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

    ps.declare_queryable(None, 1, ke, QueryableInfoType::DEFAULT);
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

    ss.declare_queryable(None, 1, ke, QueryableInfoType::DEFAULT);
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

    rs.declare_queryable(None, 1, ke, QueryableInfoType::DEFAULT);
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

    rs.declare_queryable(None, 1, ke, QueryableInfoType::DEFAULT);
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

    ns.declare_queryable(None, 1, ke, QueryableInfoType::DEFAULT);
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

    ss.declare_queryable(None, 1, ke, QueryableInfoType::DEFAULT);
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

    ns.declare_queryable(None, 1, ke, QueryableInfoType::DEFAULT);
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

    ns.declare_queryable(None, 1, ke, QueryableInfoType::DEFAULT);
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

    ns.declare_queryable(None, 1, ke, QueryableInfoType::DEFAULT);
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

/// Test that query routes in router networks are disabled even if queryable info doesn't change upon some queryable undeclaration.
#[test]
fn test_complete_queryable_failover() {
    let r0 = HarnessBuilder::new()
        .mode(WhatAmI::Router)
        .subregions([Region::Local])
        .start_runtime(true)
        .build();

    let r1 = HarnessBuilder::new()
        .mode(WhatAmI::Router)
        .subregions([Region::Local])
        .start_runtime(true)
        .build();

    let r2 = HarnessBuilder::new()
        .mode(WhatAmI::Router)
        .subregions([Region::Local])
        .start_runtime(true)
        .build();

    let mut r0_r1 = Connection {
        a: &r0,
        a2b: FaceDef::default().mode(WhatAmI::Router),
        b: &r1,
        b2a: FaceDef::default().mode(WhatAmI::Router),
    }
    .establish();

    let mut r0_r2 = Connection {
        a: &r0,
        a2b: FaceDef::default().mode(WhatAmI::Router),
        b: &r2,
        b2a: FaceDef::default().mode(WhatAmI::Router),
    }
    .establish();

    let mut bi_fwd_all = || {
        EstablishedConnection::bi_fwd_many_unbounded([&mut r0_r1, &mut r0_r2]);
        assert!(r0_r1.is_bi_complete());
        assert!(r0_r2.is_bi_complete());
    };

    bi_fwd_all();

    let s0 = r0.new_session();
    let s1 = r1.new_session();
    let s2 = r2.new_session();

    let ke = "k";

    s1.declare_queryable(
        None,
        1,
        ke,
        QueryableInfoType {
            complete: true,
            distance: 0,
        },
    );

    s2.declare_queryable(
        None,
        1,
        ke,
        QueryableInfoType {
            complete: true,
            distance: 0,
        },
    );

    bi_fwd_all();

    s0.query(1, ke);

    bi_fwd_all();

    let is_reqs1_empty = s1.recorder().requests().is_empty();
    let is_reqs2_empty = s2.recorder().requests().is_empty();

    assert!(
        is_reqs1_empty || is_reqs2_empty,
        "exactly one queryable should receive the query"
    );

    if !is_reqs1_empty {
        s1.undeclare_queryable(1);
    } else {
        s2.undeclare_queryable(1);
    }

    bi_fwd_all();

    s0.query(2, ke);

    bi_fwd_all();

    assert_eq!(r0_r1.a2b.recorder().requests().len(), 1);
    assert_eq!(r0_r2.a2b.recorder().requests().len(), 1);
}

#[test_case::test_matrix([WhatAmI::Client, WhatAmI::Peer])]
fn test_routing_after_publisher_undeclaration(mode: WhatAmI) {
    const SUBSCRIBER_ID: u32 = 1;
    const KEYEXPR: &str = "k";
    const INTEREST_ID: u32 = 1;

    let s = HarnessBuilder::new()
        .mode(mode)
        .subregions([Region::Local])
        .build();

    let n = s.new_face(
        FaceDef::default()
            .region(Region::North)
            .remote_bound(Bound::South),
    );
    let l = s.new_session();

    l.interest(
        INTEREST_ID,
        InterestMode::CurrentFuture,
        InterestOptions::KEYEXPRS + InterestOptions::SUBSCRIBERS,
        KEYEXPR,
    );

    assert_eq!(
        n.recorder().interests().len(),
        1,
        "current-future interest 1 should be propagated upstream"
    );

    n.declare_subscriber(Some(INTEREST_ID), SUBSCRIBER_ID, KEYEXPR);
    n.declare_final(1);

    assert_eq!(
        l.recorder().subscribers().len(),
        1,
        "subscriber 1 should be propagated to the local interested face"
    );

    l.interest(
        INTEREST_ID,
        InterestMode::Final,
        InterestOptions::KEYEXPRS + InterestOptions::SUBSCRIBERS,
        KEYEXPR,
    );

    n.undeclare_subscriber(SUBSCRIBER_ID);

    assert_eq!(l.recorder().undeclared_subscribers().len(), 1, "subscriber 1's undeclaration should be propagated downstream, even though it has sent `InterestFinal`");

    // NOTE: if a subscriber were to declared upstream at this point, it would
    // not be propagated downstream as the local face no longer has an active interest

    l.put(KEYEXPR, vec![42]);

    assert_eq!(
        n.recorder().pushes().len(),
        1,
        "gateway should forward the PUT; it has stale knowledge of upstream subscribers at this point"
    );
}

#[test_case::test_matrix([WhatAmI::Client, WhatAmI::Peer])]
fn test_routing_after_queryable_undeclaration(mode: WhatAmI) {
    const QUERYABLE_ID: u32 = 1;
    const KEYEXPR: &str = "k";
    const INTEREST_ID: u32 = 1;

    let s = HarnessBuilder::new()
        .mode(mode)
        .subregions([Region::Local])
        .build();

    let n = s.new_face(
        FaceDef::default()
            .region(Region::North)
            .remote_bound(Bound::South),
    );
    let l = s.new_session();

    l.interest(
        INTEREST_ID,
        InterestMode::CurrentFuture,
        InterestOptions::KEYEXPRS + InterestOptions::QUERYABLES,
        KEYEXPR,
    );

    assert_eq!(
        n.recorder().interests().len(),
        1,
        "current-future interest 1 should be propagated upstream"
    );

    n.declare_queryable(
        Some(INTEREST_ID),
        QUERYABLE_ID,
        KEYEXPR,
        QueryableInfoType::default(),
    );
    n.declare_final(INTEREST_ID);

    assert_eq!(
        l.recorder().queryables().len(),
        1,
        "queryable 1 should be propagated to the local interested face"
    );

    l.interest(
        INTEREST_ID,
        InterestMode::Final,
        InterestOptions::KEYEXPRS + InterestOptions::QUERYABLES,
        KEYEXPR,
    );

    n.undeclare_queryable(QUERYABLE_ID);

    assert_eq!(l.recorder().undeclared_queryables().len(), 1, "queryable 1's undeclaration should be propagated downstream, even though it has sent `InterestFinal`");

    // NOTE: if a queryable were to declared upstream at this point, it would
    // not be propagated downstream as the local face no longer has an active interest

    l.query(1, KEYEXPR);

    assert_eq!(
        n.recorder().requests().len(),
        1,
        "gateway should forward the GET; it has stale knowledge of upstream queryables at this point"
    );
}

// These use the real interest dispatcher and route cache. Only transport is
// replaced by the existing recording face harness; no routing state is edited.
#[test_case::test_matrix([WhatAmI::Client, WhatAmI::Peer], [false, true], [0u8, 1u8, 2u8])]
fn test_interest_generation_routing(mode: WhatAmI, queryable: bool, scenario: u8) {
    const KEY: &str = "discovery/generation";
    let gateway = HarnessBuilder::new()
        .mode(mode)
        .subregions([Region::Local])
        .build();
    let remote = gateway.new_face(
        FaceDef::default()
            .region(Region::North)
            .remote_bound(Bound::South),
    );
    let local = gateway.new_session();
    let options = InterestOptions::KEYEXPRS
        + if queryable {
            InterestOptions::QUERYABLES
        } else {
            InterestOptions::SUBSCRIBERS
        };
    local.interest(1, InterestMode::CurrentFuture, options, KEY);
    let first = remote.recorder().interests().last().unwrap().id;
    let send = |id| {
        if queryable {
            local.query(id, KEY);
        } else {
            local.put(KEY, vec![42]);
        }
    };
    let forwarded = || {
        if queryable {
            remote.recorder().requests().len()
        } else {
            remote.recorder().pushes().len()
        }
    };
    if scenario != 2 {
        remote.declare_final(first);
        send(1);
        assert_eq!(
            forwarded(),
            0,
            "a completed empty snapshot should prune the route"
        );
    }
    if scenario == 0 {
        // No entity declaration/undeclaration occurs to invalidate the cached
        // empty route. Ending the snapshot must restore fallback forwarding.
        local.interest(1, InterestMode::Final, options, KEY);
    } else {
        local.interest(2, InterestMode::CurrentFuture, options, KEY);
        let second = remote.recorder().interests().last().unwrap().id;
        assert_ne!(first, second);
        if scenario == 2 {
            remote.declare_final(first);
        }
        // In both orders, generation two still has an incomplete snapshot.
    }
    send(2);
    assert_eq!(
        forwarded(),
        1,
        "a retired or incomplete discovery snapshot must not suppress forwarding"
    );
    if scenario != 0 {
        let second = remote.recorder().interests().last().unwrap().id;
        remote.declare_final(second);
        remote.recorder().clear();
        send(3);
        assert_eq!(
            forwarded(),
            0,
            "all completed empty snapshots should prune the route again"
        );
    }
}
