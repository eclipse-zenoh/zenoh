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

use zenoh_protocol::{
    core::{Bound, Region, WhatAmI},
    network::{
        declare::{DeclareToken, TokenId},
        interest::{InterestMode, InterestOptions},
    },
};

use super::{try_init_tracing_subscriber, Connection, FaceConfig, Harness};

/// Current token propagation.
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
fn test_current_token_propagation() {
    try_init_tracing_subscriber();

    let r = Harness::new_router();
    let c1 = Harness::new_client();
    let c2 = Harness::new_client();

    let s1 = c1.new_session();
    let s2 = c1.new_session();

    let r_face = FaceConfig::default()
        .region(Region::North)
        .remote_bound(Bound::South)
        .mode(WhatAmI::Router);

    let c_face = FaceConfig::default()
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

    s2.declare_token(1, "test".try_into().unwrap(), None);
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

/// Concurrent current-future interest in two-region hierarchies.
///
/// ```d2
/// shape: sequence_diagram
///
/// S -> G: Open
///
/// S -> G.x1: Interest id=X mode=CF
/// G.x1 -> S: DeclareFinal iid=X
///
/// G -> N: Open
///
/// G -> N: Interest id=X' mode=CF
///
/// S -> G.x3: Interest id=Y mode=C
/// G.x3 -> N: Interest id=Y' mode=CF
///
/// N -> G.x2: Declare iid=X'
/// G.x2 -> NS Declare iid=None
///
/// N -> G.x4: Declare iid=Y'
/// G.x4 -> S: Declare iid=Y
/// G.x4 -> S: DeclareFinal iid=Y
/// ```
///
/// From the perspective of G, declaration w/ interest id SHOULD not imply the existence of a
/// breadcrumb. This observation led to a correction of the previous model where token declarations
/// with interest set where expected to correspond to a pending current interest. This fix is
/// codified into the `RouteCurrentEntityResult` data structure.
#[ignore]
#[test]
fn test_concurrent_cf_interests() {
    const S: Region = Region::default_south(WhatAmI::Peer);

    let g = Harness::with_subregions_noruntime(
        WhatAmI::default(),
        [S], // TODO(regions): make test cases for all south modes.
    );

    let _n = g.new_face(
        FaceConfig::default()
            .mode(WhatAmI::default())
            .remote_bound(Bound::South),
    );

    let _s = g.new_face(FaceConfig::default().mode(WhatAmI::Peer).region(S));

    todo!()
}
