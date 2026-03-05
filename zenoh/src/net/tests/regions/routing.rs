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

use std::str::FromStr;

use tracing_subscriber::EnvFilter;
use zenoh_protocol::core::{Region, WhatAmI};

use crate::key_expr::KeyExpr;

use super::{FaceConfig, Harness};

/// Tests data routing between p2p subregions.
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
    tracing_subscriber::fmt()
        .pretty()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    const S1: Region = Region::South {
        id: 0,
        mode: WhatAmI::Peer,
    };

    const S2: Region = Region::South {
        id: 1,
        mode: WhatAmI::Peer,
    };

    let g = Harness::with_subregions_noruntime(WhatAmI::default(), [S1, S2]);

    let p0 = g.new_face(FaceConfig::default().mode(WhatAmI::Peer).region(S1));
    let p1 = g.new_face(FaceConfig::default().mode(WhatAmI::Peer).region(S2));

    let ke = KeyExpr::from_str("k").unwrap();

    p0.declare_subscriber(1, &ke);
    p1.declare_subscriber(1, &ke);

    p0.put(&ke, vec![42]);
    p1.put(&ke, vec![43]);

    assert_eq!(p0.recorder().pushes().len(), 1);
    assert_eq!(p0.recorder().pushes().len(), 1);
}

/// Same as [`test_p2p_inter_subregion_data_routing`] but for queries.
#[test]
fn test_p2p_inter_subregion_query_routing() {
    tracing_subscriber::fmt()
        .pretty()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    const S1: Region = Region::South {
        id: 0,
        mode: WhatAmI::Peer,
    };

    const S2: Region = Region::South {
        id: 1,
        mode: WhatAmI::Peer,
    };

    let g = Harness::with_subregions_noruntime(WhatAmI::default(), [S1, S2]);

    let p0 = g.new_face(FaceConfig::default().mode(WhatAmI::Peer).region(S1));
    let p1 = g.new_face(FaceConfig::default().mode(WhatAmI::Peer).region(S2));

    let ke = KeyExpr::from_str("k").unwrap();

    p0.declare_queryable(1, &ke);
    p1.declare_queryable(1, &ke);

    p0.query(1, &ke);
    p1.query(1, &ke);

    assert_eq!(p0.recorder().requests().len(), 1);
    assert_eq!(p0.recorder().requests().len(), 1);
}
