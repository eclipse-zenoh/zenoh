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

// Implements the common test logic but returns the reply for further verification
async fn test_impl<'a, QClosure, RClosure>(
    test_name: &str,
    session1: &'a zenoh::Session,
    session2: &'a zenoh::Session,
    queryable_closure: QClosure,
    querier_closure: RClosure,
) -> (Option<zenoh::query::Reply>, tokio::task::JoinHandle<()>)
where
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
    let replies = match ztimeout!(querier_closure(session2)) {
        Ok(r) => r,
        Err(e) => {
            handle.abort();
            panic!("{test_name}: Failed to execute query: {e}")
        }
    };

    // Try to receive a reply with a timeout
    let reply = match ztimeout!(replies.recv_async()) {
        Ok(r) => Some(r),
        Err(_) => None,
    };

    (reply, handle)
}

// Test that expects to receive a sample (positive test)
async fn test_ok<'a, QClosure, RClosure>(
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
    let (reply, handle) = test_impl(
        test_name,
        session1,
        session2,
        queryable_closure,
        querier_closure,
    )
    .await;

    // Clean up resources first to avoid panic during abort
    handle.abort();

    // Verify that reply was received
    let reply =
        reply.unwrap_or_else(|| panic!("{test_name}: Failed to receive reply from queryable"));
    assert!(
        reply.result().is_ok(),
        "{test_name}: Reply result is not OK"
    );
    let sample = reply
        .result()
        .unwrap_or_else(|_| panic!("{test_name}: Failed to get sample from reply"));

    // Verify the sample content
    assert_eq!(
        sample
            .payload()
            .try_to_string()
            .unwrap_or_else(|_| panic!("{test_name}: Failed to convert payload to string")),
        "test_response",
        "{test_name}: Unexpected response payload"
    );
}

// Test that expects NOT to receive a sample (negative test)
async fn test_miss<'a, QClosure, RClosure>(
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
    let (reply, handle) = test_impl(
        test_name,
        session1,
        session2,
        queryable_closure,
        querier_closure,
    )
    .await;

    // Clean up resources
    handle.abort();

    // Verify that no reply was received
    assert!(
        reply.is_none(),
        "{test_name}: Unexpectedly received a reply when none was expected"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_queryable() {
    zenoh::init_log_from_env_or("error");

    // Declare session
    let s1 = ztimeout!(zenoh::open(zenoh::Config::default())).expect("Failed to open session s1");
    let s2 = ztimeout!(zenoh::open(zenoh::Config::default())).expect("Failed to open session s2");
    let ke = "test/queryable";

    // These tests should receive responses
    test_ok(
        "Basic queryable test",
        &s1,
        &s2,
        |s: &Session| s.declare_queryable(ke),
        |s: &Session| s.get(ke),
    )
    .await;
    test_ok(
        "Same session test",
        &s1,
        &s1.clone(),
        |s: &Session| s.declare_queryable(ke),
        |s: &Session| s.get(ke),
    )
    .await;

    test_ok(
        "Complete queryable with All target",
        &s1,
        &s2,
        |s: &Session| s.declare_queryable(ke).complete(true),
        |s: &Session| s.get(ke).target(zenoh::query::QueryTarget::All),
    )
    .await;

    test_ok(
        "Complete queryable with AllComplete target",
        &s1,
        &s2,
        |s: &Session| s.declare_queryable(ke).complete(true),
        |s: &Session| s.get(ke).target(zenoh::query::QueryTarget::AllComplete),
    )
    .await;

    test_ok(
        "Incomplete queryable with All target",
        &s1,
        &s2,
        |s: &Session| s.declare_queryable(ke).complete(false),
        |s: &Session| s.get(ke).target(zenoh::query::QueryTarget::All),
    )
    .await;

    test_miss(
        "Incomplete queryable with AllComplete target",
        &s1,
        &s2,
        |s: &Session| s.declare_queryable(ke).complete(false),
        |s: &Session| s.get(ke).target(zenoh::query::QueryTarget::AllComplete),
    )
    .await;

    test_miss(
        "Incomplete queryable with AllComplete target, same session",
        &s1,
        &s1.clone(),
        |s: &Session| s.declare_queryable(ke).complete(false),
        |s: &Session| s.get(ke).target(zenoh::query::QueryTarget::AllComplete),
    )
    .await;

    ztimeout!(s1.close()).expect("Failed to close session s1");
    ztimeout!(s2.close()).expect("Failed to close session s2");
}
