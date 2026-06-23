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

//! Tests involving the Adminspace.

use std::collections::HashSet;

use zenoh_buffers::buffer::SplitBuffer;
use zenoh_protocol::{
    core::{Region, WhatAmI},
    network::interest::{InterestMode, InterestOptions},
    zenoh::{PushBody, ResponseBody},
};

use super::{Connection, FaceDef, HarnessBuilder};

#[test_case::test_matrix([WhatAmI::Router, WhatAmI::Peer])]
fn test_sourced_entities_and_interests(mode: WhatAmI) {
    let g0 = HarnessBuilder::new()
        .zid("a0".parse().unwrap())
        .mode(mode)
        .subregions([Region::Local])
        .start_adminspace(true)
        .build();
    let g = g0.new_session();

    const P: Region = Region::default_south(WhatAmI::Peer);
    const C: Region = Region::default_south(WhatAmI::Client);

    let g1 = HarnessBuilder::new()
        .zid("a1".parse().unwrap())
        .mode(mode)
        .subregions([Region::Local, C, P]) // TODO(regions) add a router subregion
        .start_adminspace(true)
        .build();
    let s = g1.new_session();
    let c = g1.new_face(
        FaceDef::default()
            .region(C)
            .mode(WhatAmI::Client)
            .zid("c".parse().unwrap()),
    );
    let p = g1.new_face(
        FaceDef::default()
            .region(P)
            .mode(WhatAmI::Peer)
            .zid("b".parse().unwrap()),
    );

    for (prefix, face) in [("g", &g), ("s", &s), ("c", &c), ("p", &p)] {
        face.declare_subscriber(None, 1, format!("{prefix}/subscriber"));
        face.declare_queryable(None, 1, format!("{prefix}/queryable"));
        face.declare_token(None, 1, format!("{prefix}/token"));
        face.interest(
            1,
            InterestMode::CurrentFuture,
            InterestOptions::KEYEXPRS + InterestOptions::SUBSCRIBERS,
            format!("{prefix}/publisher"),
        );
        face.interest(
            2,
            InterestMode::CurrentFuture,
            InterestOptions::KEYEXPRS + InterestOptions::QUERYABLES,
            format!("{prefix}/querier"),
        );
    }

    let mut g0_g1 = Connection {
        a: &g0,
        b: &g1,
        a2b: FaceDef::default().mode(mode),
        b2a: FaceDef::default().mode(mode),
    }
    .establish();
    g0_g1.bi_fwd();

    #[derive(Debug, Default, serde::Deserialize, PartialEq, Eq, Hash)]
    struct Sources {
        routers: Vec<String>,
        peers: Vec<String>,
        clients: Vec<String>,
    }

    impl Sources {
        fn routers(self, zids: &[&str]) -> Self {
            self.with_mode(zids, WhatAmI::Router)
        }

        fn peers(self, zids: &[&str]) -> Self {
            self.with_mode(zids, WhatAmI::Peer)
        }

        fn clients(self, zids: &[&str]) -> Self {
            self.with_mode(zids, WhatAmI::Client)
        }

        fn with_mode(mut self, zids: &[&str], mode: WhatAmI) -> Self {
            match mode {
                WhatAmI::Router => self.routers.extend(zids.iter().map(|zid| zid.to_string())),
                WhatAmI::Peer => self.peers.extend(zids.iter().map(|zid| zid.to_string())),
                WhatAmI::Client => self.clients.extend(zids.iter().map(|zid| zid.to_string())),
            }
            self
        }

        fn with_keyexpr(self, k: &str) -> (String, Self) {
            (k.to_string(), self)
        }
    }

    let mut get = |ke: &str| -> HashSet<(String, Sources)> {
        s.query(1, ke.to_string());
        g0_g1.bi_fwd();
        let replies = s.recorder().responses();
        s.recorder().clear();
        replies
            .into_iter()
            .filter_map(|r| match r.payload {
                ResponseBody::Reply(reply) => match reply.payload {
                    PushBody::Put(put) => serde_json::from_slice(&put.payload.contiguous())
                        .ok()
                        .map(|s| (r.wire_expr.try_as_str().unwrap().to_string(), s)),
                    _ => None,
                },
                _ => None,
            })
            .collect()
    };

    // NOTE(regions): subscribers, queryables and tokens are _entities_: they are propagated in the
    // network. In contrast, publishers and queriers only exist locally within nodes that have
    // knowledge of interests.

    assert_eq!(
        get("@/a1/**"),
        [
            Sources::default()
                .clients(&["c"])
                .with_mode(&["a1"], mode)
                .with_keyexpr(&format!("@/a1/{mode}/queryable/c/queryable")),
            Sources::default()
                .peers(&["b"])
                .with_mode(&["a1"], mode)
                .with_keyexpr(&format!("@/a1/{mode}/queryable/p/queryable")),
            Sources::default()
                .clients(&["a1"])
                .with_mode(&["a1"], mode)
                .with_keyexpr(&format!("@/a1/{mode}/queryable/s/queryable")),
            Sources::default()
                .with_mode(&["a0"], mode)
                .with_keyexpr(&format!("@/a1/{mode}/queryable/g/queryable")),
            Sources::default()
                .clients(&["c"])
                .with_mode(&["a1"], mode)
                .with_keyexpr(&format!("@/a1/{mode}/subscriber/c/subscriber")),
            Sources::default()
                .peers(&["b"])
                .with_mode(&["a1"], mode)
                .with_keyexpr(&format!("@/a1/{mode}/subscriber/p/subscriber")),
            Sources::default()
                .clients(&["a1"])
                .with_mode(&["a1"], mode)
                .with_keyexpr(&format!("@/a1/{mode}/subscriber/s/subscriber")),
            Sources::default()
                .with_mode(&["a0"], mode)
                .with_keyexpr(&format!("@/a1/{mode}/subscriber/g/subscriber")),
            Sources::default()
                .clients(&["c"])
                .with_mode(&["a1"], mode)
                .with_keyexpr(&format!("@/a1/{mode}/token/c/token")),
            Sources::default()
                .peers(&["b"])
                .with_mode(&["a1"], mode)
                .with_keyexpr(&format!("@/a1/{mode}/token/p/token")),
            Sources::default()
                .clients(&["a1"])
                .with_mode(&["a1"], mode)
                .with_keyexpr(&format!("@/a1/{mode}/token/s/token")),
            Sources::default()
                .with_mode(&["a0"], mode)
                .with_keyexpr(&format!("@/a1/{mode}/token/g/token")),
            Sources::default()
                .clients(&["c"])
                .with_keyexpr(&format!("@/a1/{mode}/publisher/c/publisher")),
            Sources::default()
                .clients(&["a1"])
                .with_keyexpr(&format!("@/a1/{mode}/publisher/s/publisher")),
            Sources::default()
                .peers(&["b"])
                .with_keyexpr(&format!("@/a1/{mode}/publisher/p/publisher")),
            Sources::default()
                .clients(&["c"])
                .with_keyexpr(&format!("@/a1/{mode}/querier/c/querier")),
            Sources::default()
                .clients(&["a1"])
                .with_keyexpr(&format!("@/a1/{mode}/querier/s/querier")),
            Sources::default()
                .peers(&["b"])
                .with_keyexpr(&format!("@/a1/{mode}/querier/p/querier")),
        ]
        .into_iter()
        .collect::<HashSet<_>>()
    );

    assert_eq!(
        get("@/a0/**"),
        [
            Sources::default()
                .with_mode(&["a0"], mode)
                .clients(&["a0"])
                .with_keyexpr(&format!("@/a0/{mode}/subscriber/g/subscriber")),
            Sources::default()
                .with_mode(&["a0"], mode)
                .clients(&["a0"])
                .with_keyexpr(&format!("@/a0/{mode}/queryable/g/queryable")),
            Sources::default()
                .with_mode(&["a0"], mode)
                .clients(&["a0"])
                .with_keyexpr(&format!("@/a0/{mode}/token/g/token")),
            Sources::default()
                .clients(&["a0"])
                .with_keyexpr(&format!("@/a0/{mode}/publisher/g/publisher")),
            Sources::default()
                .clients(&["a0"])
                .with_keyexpr(&format!("@/a0/{mode}/querier/g/querier")),
            Sources::default()
                .with_mode(&["a1"], mode)
                .with_keyexpr(&format!("@/a0/{mode}/queryable/c/queryable")),
            Sources::default()
                .with_mode(&["a1"], mode)
                .with_keyexpr(&format!("@/a0/{mode}/queryable/p/queryable")),
            Sources::default()
                .with_mode(&["a1"], mode)
                .with_keyexpr(&format!("@/a0/{mode}/queryable/s/queryable")),
            Sources::default()
                .with_mode(&["a1"], mode)
                .with_keyexpr(&format!("@/a0/{mode}/subscriber/c/subscriber")),
            Sources::default()
                .with_mode(&["a1"], mode)
                .with_keyexpr(&format!("@/a0/{mode}/subscriber/p/subscriber")),
            Sources::default()
                .with_mode(&["a1"], mode)
                .with_keyexpr(&format!("@/a0/{mode}/subscriber/s/subscriber")),
            Sources::default()
                .with_mode(&["a1"], mode)
                .with_keyexpr(&format!("@/a0/{mode}/token/c/token")),
            Sources::default()
                .with_mode(&["a1"], mode)
                .with_keyexpr(&format!("@/a0/{mode}/token/p/token")),
            Sources::default()
                .with_mode(&["a1"], mode)
                .with_keyexpr(&format!("@/a0/{mode}/token/s/token")),
        ]
        .into_iter()
        .collect::<HashSet<_>>()
    );
}
