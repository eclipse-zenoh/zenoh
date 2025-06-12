//
// Copyright (c) 2023 ZettaScale Technology
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

use std::time::Duration;
use zenoh::Session;
use zenoh_core::ztimeout;

const SLEEP: Duration = Duration::from_secs(1);
const TIMEOUT: Duration = Duration::from_secs(60);

// Common function that takes 2 closure parameters and performs the test
// First closure should return queryable builder, second should return querier builder
// These closures return result from declare_queryable/get but without executing with ztimeout
async fn test<'a, QClosure, RClosure>(
    test_name: &str,
    session1: &'a zenoh::Session,
    session2: &'a zenoh::Session,
    queryable_closure: QClosure,
    querier_closure: RClosure,
) where
    QClosure: FnOnce(
        &'a zenoh::Session,
    )
        -> zenoh::query::QueryableBuilder<'a, 'a, zenoh::handlers::DefaultHandler>,
    RClosure: FnOnce(
        &'a zenoh::Session,
    )
        -> zenoh::session::SessionGetBuilder<'a, 'a, zenoh::handlers::DefaultHandler>,
{
    // Actually declare queryable and querier and perform the query
    let queryable = ztimeout!(queryable_closure(session1))
        .unwrap_or_else(|_| panic!("{test_name}: Failed to declare queryable"));

    // Wait for declaration to propagate
    tokio::time::sleep(SLEEP).await;

    // Handle queryable requests in background (using channel model)
    let handle = tokio::spawn({
        let test_name = test_name.to_string();
        async move {
            while let Ok(query) = queryable.recv_async().await {
                ztimeout!(query.reply(query.key_expr(), "test_response"))
                    .unwrap_or_else(|_| panic!("{test_name} : Failed to reply to query"));
            }
        }
    });

    // Execute query using querier
    let replies = ztimeout!(querier_closure(session2))
        .unwrap_or_else(|_| panic!("{test_name}: Failed to execute query"));
    let reply = ztimeout!(replies.recv_async())
        .unwrap_or_else(|_| panic!("{test_name}: Failed to receive reply from queryable"));
    assert!(reply.result().is_ok());
    let _sample = reply
        .result()
        .unwrap_or_else(|_| panic!("{test_name}: Failed to get sample from reply"));

    handle.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_queryable() {
    zenoh::init_log_from_env_or("error");

    // Declare session
    let s1 = ztimeout!(zenoh::open(zenoh::Config::default())).expect("Failed to open session s1");
    let s2 = ztimeout!(zenoh::open(zenoh::Config::default())).expect("Failed to open session s2");
    let ke = "test/queryable";

    test(
        "Basic queryable test",
        &s1,
        &s2,
        |s: &Session| s.declare_queryable(ke),
        |s: &Session| s.get(ke),
    )
    .await;
    test(
        "Same session test",
        &s1,
        &s1.clone(),
        |s: &Session| s.declare_queryable(ke),
        |s: &Session| s.get(ke),
    )
    .await;

    test(
        "Complete queryable with All target",
        &s1,
        &s2,
        |s: &Session| s.declare_queryable(ke).complete(true),
        |s: &Session| s.get(ke).target(zenoh::query::QueryTarget::All),
    )
    .await;

    test(
        "Complete queryable with AllComplete target",
        &s1,
        &s2,
        |s: &Session| s.declare_queryable(ke).complete(true),
        |s: &Session| s.get(ke).target(zenoh::query::QueryTarget::AllComplete),
    )
    .await;

    test(
        "Incomplete queryable with All target",
        &s1,
        &s2,
        |s: &Session| s.declare_queryable(ke).complete(false),
        |s: &Session| s.get(ke).target(zenoh::query::QueryTarget::All),
    )
    .await;

    test(
        "Incomplete queryable with AllComplete target",
        &s1,
        &s2,
        |s: &Session| s.declare_queryable(ke).complete(false),
        |s: &Session| s.get(ke).target(zenoh::query::QueryTarget::AllComplete),
    )
    .await;

    ztimeout!(s1.close()).expect("Failed to close session s1");
    ztimeout!(s2.close()).expect("Failed to close session s2");
}
