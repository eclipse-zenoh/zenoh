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

//! Tests involving [`zenoh_protocol::network::oam`].

use zenoh_protocol::core::{Bound, Region, WhatAmI};

use super::{Connection, EstablishedConnection, FaceDef, HarnessBuilder};

/// Checks the following properties:
/// 1. Peer gateways, in single-hop gossip mode, propagate linkstate information downstream without
///    link info.
/// 2. Non-gateway peers, in single-hop mode, propagate linkstate information upstream with link
///    info.
#[test]
fn test_peer_singlehop_gossip_linkstate_propagation() {
    const S: Region = Region::default_south(WhatAmI::Peer);
    let a = HarnessBuilder::new()
        .zid("a".parse().unwrap())
        .subregions([S])
        .build();

    let b0 = HarnessBuilder::new().zid("b0".parse().unwrap()).build();
    let b1 = HarnessBuilder::new().zid("b1".parse().unwrap()).build();

    let mut b0_b1 = Connection {
        a: &b0,
        a2b: FaceDef::default(),
        b: &b1,
        b2a: FaceDef::default(),
    }
    .establish();

    let mut a_b0 = Connection {
        a: &a,
        a2b: FaceDef::default().region(S),
        b: &b0,
        b2a: FaceDef::default().remote_bound(Bound::South),
    }
    .establish();

    let mut a_b1 = Connection {
        a: &a,
        a2b: FaceDef::default().region(S),
        b: &b1,
        b2a: FaceDef::default().remote_bound(Bound::South),
    }
    .establish();

    EstablishedConnection::bi_fwd_many_unbounded([&mut b0_b1, &mut a_b0, &mut a_b1]);

    assert!(
        a_b0.a2b
            .recorder()
            .flat_linkstates()
            .into_iter()
            .filter(|ls| ls.psid != 0)
            .all(|ls| ls.links.is_empty()),
        "all linkstate updates propagated by a to b0 should have an empty `links` field"
    );

    assert!(
        a_b1.a2b
            .recorder()
            .flat_linkstates()
            .into_iter()
            .filter(|ls| ls.psid != 0)
            .all(|ls| ls.links.is_empty()),
        "all linkstate updates propagated by a to b1 should have an empty `links` field"
    );

    assert!(
        a_b0.b2a
            .recorder()
            .flat_linkstates()
            .into_iter()
            .any(|ls| ls.psid == 0 && !ls.links.is_empty()),
        "there exists a self linkstate update sent from b0 to a with a non-empty `links` field"
    );

    assert!(
        a_b1.b2a
            .recorder()
            .flat_linkstates()
            .into_iter()
            .any(|ls| ls.psid == 0 && !ls.links.is_empty()),
        "there exists a self linkstate update sent from b1 to a with a non-empty `links` field"
    );

    assert!(
        a_b0.b2a
            .recorder()
            .flat_linkstates()
            .into_iter()
            .filter(|ls| ls.psid != 0)
            .all(|ls| ls.links.is_empty()),
        "all linkstate updates propagated by b0 to a should have an empty `links` field"
    );

    assert!(
        a_b1.b2a
            .recorder()
            .flat_linkstates()
            .into_iter()
            .filter(|ls| ls.psid != 0)
            .all(|ls| ls.links.is_empty()),
        "all linkstate updates propagated by b1 to a should have an empty `links` field"
    );

    assert!(
        b0_b1
            .a2b
            .recorder()
            .flat_linkstates()
            .into_iter()
            .filter(|ls| ls.psid != 0)
            .all(|ls| ls.links.is_empty()),
        "all linkstate updates propagated by b0 to b1 should have an empty `links` field"
    );

    assert!(
        b0_b1
            .b2a
            .recorder()
            .flat_linkstates()
            .into_iter()
            .filter(|ls| ls.psid != 0)
            .all(|ls| ls.links.is_empty()),
        "all linkstate updates propagated by b1 to b0 should have an empty `links` field"
    );
}
