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

//! Real-transport (loopback TCP) integration tests for **northbound declaration aggregation**
//! (`aggregation.upstream.{subscribers,queryables}`): a north-bound router HAT folds a downstream
//! session's per-key subscribers/queryables included by a configured prefix into a single
//! `${prefix}` declaration toward the upstream, suppressing the per-key children upstream while
//! keeping them locally for downward routing. An upstream router thus holds one Resource per prefix
//! instead of one per forwarded child key-expression.
//!
//! These exercise the full downstream→upstream routing path over real sessions (matching the
//! `scenario*` tests); deterministic in-process coverage is in
//! `zenoh/src/net/tests/regions/declare.rs::test_northbound_subscriber_aggregation`.
//! (For readability the tests below name the two routers `gateway`/`edge` and the upstream client
//! `client`, and use `branch1/...` key-expressions — all purely illustrative; the feature itself is
//! topology- and application-agnostic.)

use std::time::Duration;

use zenoh_config::WhatAmI::Router;
use zenoh_core::ztimeout;

use crate::{loc, Node};

const TIMEOUT: Duration = Duration::from_secs(60);

async fn count_admin(loc: &str, zid: &str, kind: &str, obs_id: &str, needle: &str) -> usize {
    use zenoh_config::WhatAmI::Client;
    let observer = ztimeout!(Node::new(Client, obs_id).connect(&[loc]).open());
    tokio::time::sleep(Duration::from_secs(1)).await;
    let sel = format!("@/{zid}/router/{kind}/**");
    let replies = ztimeout!(observer.get(sel.as_str())).unwrap();
    let mut n = 0usize;
    while let Ok(reply) = replies.recv_async().await {
        if let Ok(s) = reply.result() {
            let ke = s.key_expr().as_str().to_string();
            let payload = s
                .payload()
                .try_to_string()
                .map(|c| c.into_owned())
                .unwrap_or_default();
            if ke.contains(needle) || payload.contains(needle) {
                n += 1;
            }
        }
    }
    ztimeout!(observer.close()).unwrap();
    n
}

/// Subscribers: K per-key subscribers on a downstream session collapse to ONE `branch1/**` at the
/// upstream router; a publish from an upstream client still reaches the downstream subscriber, and
/// the aggregate is withdrawn on teardown.
#[cfg(feature = "internal")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn verify_upstream_subscriber_aggregation() {
    use std::sync::{Arc, Mutex};

    use zenoh_config::WhatAmI::Client;

    let k: usize = 20;

    let gateway = ztimeout!(Node::new(Router, "1ca00001")
        .listen("tcp/127.0.0.1:0")
        .open());
    let gateway_loc = loc!(gateway).to_string();
    let gateway_zid = ztimeout!(gateway.info().zid()).to_string();
    let edge = ztimeout!(Node::new(Router, "eda00001")
        .listen("tcp/127.0.0.1:0")
        .connect(&[gateway_loc.as_str()])
        .insert("aggregation/upstream/subscribers", "[\"branch1/**\"]")
        .open());
    let edge_loc = loc!(edge).to_string();
    let internal = ztimeout!(Node::new(Client, "c1a00001")
        .connect(&[edge_loc.as_str()])
        .open());

    let received: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let mut subs = Vec::new();
    for j in 0..k {
        let received = received.clone();
        subs.push(ztimeout!(internal
            .declare_subscriber(format!("branch1/sensor/{j:04}"))
            .callback(move |s| {
                received
                    .lock()
                    .unwrap()
                    .push(s.key_expr().as_str().to_string());
            })));
    }
    tokio::time::sleep(Duration::from_secs(2)).await;

    let count = count_admin(
        &gateway_loc,
        &gateway_zid,
        "subscriber",
        "2ba00001",
        "branch1",
    )
    .await;

    let client = ztimeout!(Node::new(Client, "aba00001")
        .connect(&[gateway_loc.as_str()])
        .open());
    tokio::time::sleep(Duration::from_millis(500)).await;
    ztimeout!(client.put("branch1/sensor/0005", "hello")).unwrap();
    tokio::time::sleep(Duration::from_secs(1)).await;
    let delivered = received
        .lock()
        .unwrap()
        .iter()
        .any(|kk| kk == "branch1/sensor/0005");

    drop(subs);
    ztimeout!(client.close()).unwrap();
    tokio::time::sleep(Duration::from_secs(2)).await;
    let count_after = count_admin(
        &gateway_loc,
        &gateway_zid,
        "subscriber",
        "2ba00002",
        "branch1",
    )
    .await;

    eprintln!(
        "\n=== upstream subscriber aggregation (K={k}) ===\n\
         gateway branch1 subscribers : {count} (expect 1)\n\
         client->branch delivery    : {delivered} (expect true)\n\
         after teardown           : {count_after} (expect 0)"
    );

    ztimeout!(internal.close()).unwrap();
    ztimeout!(edge.close()).unwrap();
    ztimeout!(gateway.close()).unwrap();

    assert_eq!(
        count, 1,
        "gateway must advertise one aggregate branch1 subscriber, got {count}"
    );
    assert!(
        delivered,
        "client->branch data must reach the internal subscriber through the aggregate"
    );
    assert_eq!(
        count_after, 0,
        "aggregate must be withdrawn after all children undeclared, got {count_after}"
    );
}

/// Queryables: K per-key queryables collapse to ONE at the upstream router; an upstream client `get`
/// reaches the real queryable; a wildcard `get` fans out to all children; a missing key returns
/// empty (no hang);
/// teardown withdraws the aggregate.
#[cfg(feature = "internal")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn verify_upstream_queryable_aggregation() {
    use zenoh::Wait;
    use zenoh_config::WhatAmI::Client;

    let k: usize = 20;

    let gateway = ztimeout!(Node::new(Router, "1ca00010")
        .listen("tcp/127.0.0.1:0")
        .open());
    let gateway_loc = loc!(gateway).to_string();
    let gateway_zid = ztimeout!(gateway.info().zid()).to_string();
    let edge = ztimeout!(Node::new(Router, "eda00010")
        .listen("tcp/127.0.0.1:0")
        .connect(&[gateway_loc.as_str()])
        .insert("aggregation/upstream/queryables", "[\"branch1/**\"]")
        .open());
    let edge_loc = loc!(edge).to_string();
    let internal = ztimeout!(Node::new(Client, "c1a00010")
        .connect(&[edge_loc.as_str()])
        .open());

    let mut qabls = Vec::new();
    for j in 0..k {
        let my_key = format!("branch1/sensor/{j:04}");
        qabls.push(ztimeout!(internal
            .declare_queryable(my_key.clone())
            .callback(move |q| {
                Wait::wait(q.reply(my_key.clone(), "pong")).unwrap();
            })));
    }
    tokio::time::sleep(Duration::from_secs(2)).await;

    let count = count_admin(
        &gateway_loc,
        &gateway_zid,
        "queryable",
        "2ba00010",
        "branch1",
    )
    .await;

    let client = ztimeout!(Node::new(Client, "aba00010")
        .connect(&[gateway_loc.as_str()])
        .open());
    tokio::time::sleep(Duration::from_millis(500)).await;

    let replies = ztimeout!(client.get("branch1/sensor/0005")).unwrap();
    let mut got_reply = false;
    while let Ok(reply) = replies.recv_async().await {
        if let Ok(s) = reply.result() {
            if s.key_expr().as_str() == "branch1/sensor/0005" {
                got_reply = true;
            }
        }
    }

    let wild = ztimeout!(client.get("branch1/**")).unwrap();
    let mut wild_replies = 0usize;
    while let Ok(reply) = wild.recv_async().await {
        if reply.result().is_ok() {
            wild_replies += 1;
        }
    }

    let empty = ztimeout!(client.get("branch1/sensor/9999")).unwrap();
    let mut empty_replies = 0usize;
    while let Ok(reply) = empty.recv_async().await {
        if reply.result().is_ok() {
            empty_replies += 1;
        }
    }

    drop(qabls);
    ztimeout!(client.close()).unwrap();
    tokio::time::sleep(Duration::from_secs(2)).await;
    let count_after = count_admin(
        &gateway_loc,
        &gateway_zid,
        "queryable",
        "2ba00011",
        "branch1",
    )
    .await;

    eprintln!(
        "\n=== upstream queryable aggregation (K={k}) ===\n\
         gateway branch1 queryables : {count} (expect 1)\n\
         concrete get reply      : {got_reply} (expect true)\n\
         wildcard get replies    : {wild_replies} (expect {k})\n\
         missing-key get replies : {empty_replies} (expect 0)\n\
         after teardown          : {count_after} (expect 0)"
    );

    ztimeout!(internal.close()).unwrap();
    ztimeout!(edge.close()).unwrap();
    ztimeout!(gateway.close()).unwrap();

    assert_eq!(
        count, 1,
        "gateway must advertise one aggregate branch1 queryable, got {count}"
    );
    assert!(
        got_reply,
        "client get must reach the real per-key queryable through the aggregate"
    );
    assert_eq!(
        wild_replies, k,
        "wildcard get must fan out to all {k} children, got {wild_replies}"
    );
    assert_eq!(
        empty_replies, 0,
        "missing-key get must return empty, got {empty_replies}"
    );
    assert_eq!(
        count_after, 0,
        "aggregate must be withdrawn after teardown, got {count_after}"
    );
}

/// **B-A2 regression — `target=AllComplete` must reach a complete child through the (complete=false)
/// aggregate.** The forwarding router folds its per-key queryables into a `complete=false`
/// `branch1/**` aggregate. An upstream client issuing `get(target=AllComplete)` for a key served by a
/// genuinely-complete downstream queryable must be forwarded through the aggregate (the upstream
/// router treats the non-complete wildcard covering the query, toward a router, as a transparent
/// forwarder) and reach the child — previously the upstream router's AllComplete branch dropped the
/// aggregate and silently returned zero replies. The negative control proves the forwarder does NOT
/// over-broaden: AllComplete to a NON-complete child still returns empty (the forwarding router
/// re-applies the filter and excludes it).
#[cfg(feature = "internal")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn verify_upstream_queryable_allcomplete() {
    use zenoh::{query::QueryTarget, Wait};
    use zenoh_config::WhatAmI::Client;

    let gateway = ztimeout!(Node::new(Router, "1ca00012")
        .listen("tcp/127.0.0.1:0")
        .open());
    let gateway_loc = loc!(gateway).to_string();
    let edge = ztimeout!(Node::new(Router, "eda00012")
        .listen("tcp/127.0.0.1:0")
        .connect(&[gateway_loc.as_str()])
        .insert("aggregation/upstream/queryables", "[\"branch1/**\"]")
        .open());
    let edge_loc = loc!(edge).to_string();
    let internal = ztimeout!(Node::new(Client, "c1a00012")
        .connect(&[edge_loc.as_str()])
        .open());

    // A genuinely-complete child (the case AllComplete must reach) and a non-complete child
    // (negative control: AllComplete must NOT spuriously reach it via the forwarder).
    let q_complete = ztimeout!(internal
        .declare_queryable("branch1/sensor/0005")
        .complete(true)
        .callback(|q| {
            Wait::wait(q.reply("branch1/sensor/0005", "pong")).unwrap();
        }));
    let q_incomplete = ztimeout!(internal
        .declare_queryable("branch1/diag/0001")
        .complete(false)
        .callback(|q| {
            Wait::wait(q.reply("branch1/diag/0001", "pong")).unwrap();
        }));
    tokio::time::sleep(Duration::from_secs(2)).await;

    let client = ztimeout!(Node::new(Client, "aba00012")
        .connect(&[gateway_loc.as_str()])
        .open());
    tokio::time::sleep(Duration::from_millis(500)).await;

    // (1) THE FIX: AllComplete to the complete child must reach it through the aggregate.
    let ac = ztimeout!(client
        .get("branch1/sensor/0005")
        .target(QueryTarget::AllComplete))
    .unwrap();
    let mut ac_replies = 0usize;
    while let Ok(reply) = ac.recv_async().await {
        if let Ok(s) = reply.result() {
            if s.key_expr().as_str() == "branch1/sensor/0005" {
                ac_replies += 1;
            }
        }
    }

    // (2) NEGATIVE CONTROL: AllComplete to the non-complete child must stay empty.
    let acn = ztimeout!(client
        .get("branch1/diag/0001")
        .target(QueryTarget::AllComplete))
    .unwrap();
    let mut acn_replies = 0usize;
    while let Ok(reply) = acn.recv_async().await {
        if reply.result().is_ok() {
            acn_replies += 1;
        }
    }

    // (3) CONTROL: default BestMatching still reaches the complete child.
    let bm = ztimeout!(client.get("branch1/sensor/0005")).unwrap();
    let mut bm_replies = 0usize;
    while let Ok(reply) = bm.recv_async().await {
        if let Ok(s) = reply.result() {
            if s.key_expr().as_str() == "branch1/sensor/0005" {
                bm_replies += 1;
            }
        }
    }

    eprintln!(
        "\n=== B-A2 AllComplete through aggregate ===\n\
         AllComplete -> complete child   : {ac_replies} (expect >=1)\n\
         AllComplete -> incomplete child : {acn_replies} (expect 0)\n\
         BestMatching -> complete child  : {bm_replies} (expect >=1)"
    );

    drop(q_complete);
    drop(q_incomplete);
    ztimeout!(client.close()).unwrap();
    ztimeout!(internal.close()).unwrap();
    ztimeout!(edge.close()).unwrap();
    ztimeout!(gateway.close()).unwrap();

    assert!(
        ac_replies >= 1,
        "B-A2: AllComplete get must reach the complete child through the aggregate, got {ac_replies}"
    );
    assert_eq!(
        acn_replies, 0,
        "AllComplete must not over-broaden to a non-complete child, got {acn_replies}"
    );
    assert!(
        bm_replies >= 1,
        "BestMatching get must still reach the child, got {bm_replies}"
    );
}

/// **No-shadow correctness.** TWO forwarding routers advertise the SAME prefix `branch1/**` (each
/// with distinct keys). Because the aggregate is always advertised `complete=false` (it never
/// asserts `complete=true`), the default `BestMatching` get must NOT be shadowed onto a single
/// router — a client querying a key served by router B must still reach it even though router A also
/// advertises the prefix. (Under an unconditional-complete design this would silently return empty.)
#[cfg(feature = "internal")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn verify_multi_edge_same_prefix_no_shadow() {
    use zenoh::Wait;
    use zenoh_config::WhatAmI::Client;

    let gateway = ztimeout!(Node::new(Router, "1ca00020")
        .listen("tcp/127.0.0.1:0")
        .open());
    let gateway_loc = loc!(gateway).to_string();

    let mk_edge = |zid: &'static str| {
        let gateway_loc = gateway_loc.clone();
        async move {
            ztimeout!(Node::new(Router, zid)
                .listen("tcp/127.0.0.1:0")
                .connect(&[gateway_loc.as_str()])
                .insert("aggregation/upstream/queryables", "[\"branch1/**\"]")
                .open())
        }
    };
    let edge_a = mk_edge("eda00020").await;
    let edge_b = mk_edge("eda00021").await;
    let edge_a_loc = loc!(edge_a).to_string();
    let edge_b_loc = loc!(edge_b).to_string();

    // branchA serves branch1/a/**, branchB serves branch1/b/** — same configured prefix, disjoint keys.
    let branch_a = ztimeout!(Node::new(Client, "c1a00020")
        .connect(&[edge_a_loc.as_str()])
        .open());
    let branch_b = ztimeout!(Node::new(Client, "c1a00021")
        .connect(&[edge_b_loc.as_str()])
        .open());
    // Declared COMPLETE — the discriminating case: if the folded `branch1/**` aggregate inherited a
    // child's complete=true, BestMatching would shadow one router. The aggregate is advertised
    // non-complete (a presence advertisement), so the query fans out and both routers stay reachable.
    let _qa = ztimeout!(branch_a
        .declare_queryable("branch1/a/x")
        .complete(true)
        .callback(|q| Wait::wait(q.reply("branch1/a/x", "from-A")).unwrap()));
    let _qb = ztimeout!(branch_b
        .declare_queryable("branch1/b/y")
        .complete(true)
        .callback(|q| Wait::wait(q.reply("branch1/b/y", "from-B")).unwrap()));
    tokio::time::sleep(Duration::from_secs(2)).await;

    let client = ztimeout!(Node::new(Client, "aba00020")
        .connect(&[gateway_loc.as_str()])
        .open());
    tokio::time::sleep(Duration::from_millis(500)).await;

    let mut got_a = false;
    let mut got_b = false;
    let ra = ztimeout!(client.get("branch1/a/x")).unwrap();
    while let Ok(reply) = ra.recv_async().await {
        if let Ok(s) = reply.result() {
            if s.key_expr().as_str() == "branch1/a/x" {
                got_a = true;
            }
        }
    }
    let rb = ztimeout!(client.get("branch1/b/y")).unwrap();
    while let Ok(reply) = rb.recv_async().await {
        if let Ok(s) = reply.result() {
            if s.key_expr().as_str() == "branch1/b/y" {
                got_b = true;
            }
        }
    }

    eprintln!(
        "\n=== multi-edge same-prefix (no shadow) ===\n\
         get branch1/a/x reached edge A: {got_a} (expect true)\n\
         get branch1/b/y reached edge B: {got_b} (expect true)"
    );

    ztimeout!(client.close()).unwrap();
    ztimeout!(branch_a.close()).unwrap();
    ztimeout!(branch_b.close()).unwrap();
    ztimeout!(edge_a.close()).unwrap();
    ztimeout!(edge_b.close()).unwrap();
    ztimeout!(gateway.close()).unwrap();

    assert!(
        got_a,
        "key served by edge A must be reachable (no shadowing)"
    );
    assert!(
        got_b,
        "key served by edge B must be reachable (no shadowing)"
    );
}

/// Cross-mesh: the aggregate (sub + qabl) propagates natively across a router mesh; a client on
/// router A reaches the downstream subscriber/queryable behind router B for both pub and get.
#[cfg(feature = "internal")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn verify_upstream_aggregation_across_mesh() {
    use std::sync::{Arc, Mutex};

    use zenoh::Wait;
    use zenoh_config::WhatAmI::Client;

    let k: usize = 20;

    let gateway_a = ztimeout!(Node::new(Router, "1ca00030")
        .listen("tcp/127.0.0.1:0")
        .open());
    let gateway_a_loc = loc!(gateway_a).to_string();
    let gateway_a_zid = ztimeout!(gateway_a.info().zid()).to_string();
    let gateway_b = ztimeout!(Node::new(Router, "1ca00031")
        .listen("tcp/127.0.0.1:0")
        .connect(&[gateway_a_loc.as_str()])
        .open());
    let gateway_b_loc = loc!(gateway_b).to_string();
    let edge = ztimeout!(Node::new(Router, "eda00030")
        .listen("tcp/127.0.0.1:0")
        .connect(&[gateway_b_loc.as_str()])
        .insert("aggregation/upstream/subscribers", "[\"branch1/**\"]")
        .insert("aggregation/upstream/queryables", "[\"branch1/**\"]")
        .open());
    let edge_loc = loc!(edge).to_string();
    let internal = ztimeout!(Node::new(Client, "c1a00030")
        .connect(&[edge_loc.as_str()])
        .open());

    let received: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let mut subs = Vec::new();
    let mut qabls = Vec::new();
    for j in 0..k {
        let received = received.clone();
        subs.push(ztimeout!(internal
            .declare_subscriber(format!("branch1/sensor/{j:04}"))
            .callback(move |s| received
                .lock()
                .unwrap()
                .push(s.key_expr().as_str().to_string()))));
        let my_key = format!("branch1/sensor/{j:04}");
        qabls.push(ztimeout!(internal
            .declare_queryable(my_key.clone())
            .callback(
                move |q| Wait::wait(q.reply(my_key.clone(), "pong")).unwrap()
            )));
    }
    tokio::time::sleep(Duration::from_secs(3)).await;

    let subs_at_a = count_admin(
        &gateway_a_loc,
        &gateway_a_zid,
        "subscriber",
        "2ba00030",
        "branch1",
    )
    .await;
    let qabls_at_a = count_admin(
        &gateway_a_loc,
        &gateway_a_zid,
        "queryable",
        "2ba00031",
        "branch1",
    )
    .await;

    let client = ztimeout!(Node::new(Client, "aba00030")
        .connect(&[gateway_a_loc.as_str()])
        .open());
    tokio::time::sleep(Duration::from_millis(800)).await;
    ztimeout!(client.put("branch1/sensor/0005", "cmd")).unwrap();
    let replies = ztimeout!(client.get("branch1/sensor/0007")).unwrap();
    let mut got_reply = false;
    while let Ok(reply) = replies.recv_async().await {
        if let Ok(s) = reply.result() {
            if s.key_expr().as_str() == "branch1/sensor/0007" {
                got_reply = true;
            }
        }
    }
    tokio::time::sleep(Duration::from_secs(1)).await;
    let delivered = received
        .lock()
        .unwrap()
        .iter()
        .any(|kk| kk == "branch1/sensor/0005");

    eprintln!(
        "\n=== cross-mesh aggregation (K={k}) ===\n\
         gateway_A subs={subs_at_a} qabls={qabls_at_a} (expect 1,1)\n\
         client(A)->branch(B) pub={delivered} get={got_reply} (expect true,true)"
    );

    drop(subs);
    drop(qabls);
    ztimeout!(client.close()).unwrap();
    ztimeout!(internal.close()).unwrap();
    ztimeout!(edge.close()).unwrap();
    ztimeout!(gateway_b.close()).unwrap();
    ztimeout!(gateway_a.close()).unwrap();

    assert_eq!(
        subs_at_a, 1,
        "aggregate subscriber must propagate cross-mesh, got {subs_at_a}"
    );
    assert_eq!(
        qabls_at_a, 1,
        "aggregate queryable must propagate cross-mesh, got {qabls_at_a}"
    );
    assert!(delivered, "client(A)->branch(B) pub must be delivered");
    assert!(got_reply, "client(A)->branch(B) get must reply");
}

/// Scale bench (ignored): upstream cardinality + RSS, per-key vs aggregated. Tune AGG_N / AGG_K.
#[cfg(feature = "internal")]
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore = "perf bench; run with --ignored --nocapture"]
async fn perf_upstream_aggregation_scale() {
    use zenoh_config::WhatAmI::Client;

    fn read_rss_kb() -> u64 {
        let Ok(status) = std::fs::read_to_string("/proc/self/status") else {
            return 0;
        };
        for line in status.lines() {
            if let Some(rest) = line.strip_prefix("VmRSS:") {
                return rest
                    .split_whitespace()
                    .next()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
            }
        }
        0
    }

    let n: usize = std::env::var("AGG_N")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100);
    let k: usize = std::env::var("AGG_K")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100);
    eprintln!(
        "\n=== upstream-agg scale: N={n} branchs, K={k} keys ({} total) ===",
        n * k
    );

    for aggregated in [false, true] {
        let gateway = ztimeout!(Node::new(Router, "1c000060")
            .listen("tcp/127.0.0.1:0")
            .open());
        let gateway_loc = loc!(gateway).to_string();
        let gateway_zid = ztimeout!(gateway.info().zid()).to_string();
        let mut eb = Node::new(Router, "ed000061")
            .listen("tcp/127.0.0.1:0")
            .connect(&[gateway_loc.as_str()]);
        if aggregated {
            let prefixes = (0..n)
                .map(|i| format!("\"branch{i}/**\""))
                .collect::<Vec<_>>()
                .join(",");
            eb = eb.insert("aggregation/upstream/subscribers", &format!("[{prefixes}]"));
        }
        let edge = ztimeout!(eb.open());
        let edge_loc = loc!(edge).to_string();
        tokio::time::sleep(Duration::from_secs(1)).await;
        let rss_before = read_rss_kb();

        let mut branchs = Vec::with_capacity(n);
        let mut subs = Vec::new();
        for i in 0..n {
            let branch = ztimeout!(Node::new(Client, &format!("c1{i:06x}"))
                .connect(&[edge_loc.as_str()])
                .open());
            for j in 0..k {
                subs.push(ztimeout!(branch
                    .declare_subscriber(format!("branch{i}/sensor{j:04}"))
                    .callback(|_| {})));
            }
            branchs.push(branch);
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
        let rss_after = read_rss_kb();
        let card = count_admin(
            &gateway_loc,
            &gateway_zid,
            "subscriber",
            "2b000062",
            "branch",
        )
        .await;
        eprintln!(
            "{}: gateway-card={card} rss-delta={} kB",
            if aggregated {
                "AGGREGATED"
            } else {
                "PER-KEY   "
            },
            rss_after.saturating_sub(rss_before)
        );
        drop(subs);
        for r in branchs {
            let _ = r.close().await;
        }
        ztimeout!(edge.close()).unwrap();
        ztimeout!(gateway.close()).unwrap();
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}
