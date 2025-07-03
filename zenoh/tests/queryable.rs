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

use std::{time::Duration, vec};

use futures::StreamExt;
use zenoh::{
    handlers::DefaultHandler,
    query::{ConsolidationMode, Query, QueryTarget, QueryableBuilder, Reply},
    sample::SampleKind,
    session::SessionGetBuilder,
    Session,
};
use zenoh_core::ztimeout;

const SLEEP: Duration = Duration::from_millis(100);
const TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, PartialEq, Clone, Copy)]
enum RKind {
    None,
    Reply,
    ReplyDel,
    ReplyErr,
}

// implement display for ReplyVariant
impl std::fmt::Display for RKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RKind::None => write!(f, "None"),
            RKind::Reply => write!(f, "Reply"),
            RKind::ReplyDel => write!(f, "ReplyDel"),
            RKind::ReplyErr => write!(f, "ReplyErr"),
        }
    }
}

impl<E> From<Result<Reply, E>> for RKind {
    fn from(result: Result<Reply, E>) -> Self {
        match result {
            Ok(reply) => reply.into(),
            Err(_) => RKind::None,
        }
    }
}

impl From<Reply> for RKind {
    fn from(reply: Reply) -> Self {
        match reply.result() {
            Ok(sample) => {
                if sample.kind() == SampleKind::Put {
                    RKind::Reply
                } else {
                    RKind::ReplyDel
                }
            }
            Err(_) => RKind::ReplyErr,
        }
    }
}

type QBuilder<'a> = QueryableBuilder<'a, 'a, DefaultHandler>;
type GBuilder<'a> = SessionGetBuilder<'a, 'a, DefaultHandler>;

macro_rules! msg {
    ($test_mode:expr, $test_name:expr) => {
        format!("{} - {}", $test_name, $test_mode)
    };
}

#[allow(clippy::too_many_arguments)]
async fn test<'a, QClosure, RClosure>(
    test_name: String,
    keyexpr: &'static str,
    q_session: &'a Session,
    r_session: &'a Session,
    q_builder: QClosure,
    r_builder: RClosure,
    replies_to_send: Vec<RKind>,
    replies_expected: Vec<Vec<RKind>>,
) where
    QClosure: FnOnce(QBuilder<'a>) -> QBuilder<'a>,
    RClosure: FnOnce(GBuilder<'a>) -> GBuilder<'a>,
{
    println!("Queryable: {test_name}");
    let test_name_clone = test_name.clone();
    let keyexpr_clone = keyexpr.to_string();
    let _queryable = ztimeout!(q_builder(q_session.declare_queryable(keyexpr)).callback(
        move |query: Query| {
            let replies_to_send = replies_to_send.clone();
            let test_name = test_name_clone.clone();
            let keyexpr = keyexpr_clone.clone();
            tokio::spawn(async move {
                for variant in replies_to_send {
                    match variant {
                        RKind::None => {}
                        RKind::Reply => {
                            ztimeout!(query.reply(&keyexpr, "reply")).unwrap_or_else(|_| {
                                panic!("{test_name} : failed to reply to query");
                            });
                        }
                        RKind::ReplyDel => {
                            ztimeout!(query.reply_del(&keyexpr)).unwrap_or_else(|_| {
                                panic!("{test_name} : failed to reply_del to query");
                            });
                        }
                        RKind::ReplyErr => {
                            ztimeout!(query.reply_err("error")).unwrap_or_else(|_| {
                                panic!("{test_name} : failed to reply_err to query");
                            });
                        }
                    }
                }
            });
        }
    ))
    .unwrap_or_else(|_| panic!("{test_name} : failed to declare queryable"));

    tokio::time::sleep(SLEEP).await;

    let replies_channel = ztimeout!(r_builder(r_session.get(keyexpr)))
        .unwrap_or_else(|_| panic!("{test_name} : failed to execute query"));
    let replies_received = replies_channel
        .into_stream()
        .then(|r| async move { RKind::from(r) })
        .collect::<Vec<_>>()
        .await;

    // Check if replies_received matches any of the expected variants
    let matches_any_expected = replies_expected
        .iter()
        .any(|expected| expected == &replies_received);

    assert!(
        matches_any_expected,
        "{test_name}: Received replies {replies_received:?} do not match any expected variants {replies_expected:?}"
    );
}

async fn test_queryable_impl(s1: &Session, s2: &Session, test_mode: &str, key_expr: &'static str) {
    test(
        msg!(test_mode, "Basic - reply first"),
        key_expr,
        s1,
        s2,
        |b| b,
        |b| b,
        vec![RKind::Reply, RKind::ReplyDel, RKind::ReplyErr],
        // There is currently no guarantee on reply ordering. ReplyErr being non consolidated, it arrives first.
        vec![vec![RKind::ReplyErr, RKind::ReplyDel]],
    )
    .await;
    test(
        msg!(test_mode, "Basic - reply_del first"),
        key_expr,
        s1,
        s2,
        |b| b,
        |b| b,
        vec![RKind::ReplyDel, RKind::Reply, RKind::ReplyErr],
        // There is currently no guarantee on reply ordering. ReplyErr being non consolidated, it arrives first.
        vec![vec![RKind::ReplyErr, RKind::Reply]],
    )
    .await;
    test(
        msg!(
            test_mode,
            "Completeness: Complete queryable with All target"
        ),
        key_expr,
        s1,
        s2,
        |b| b.complete(true),
        |b| b.target(QueryTarget::All),
        vec![RKind::Reply],
        vec![vec![RKind::Reply]],
    )
    .await;
    test(
        msg!(
            test_mode,
            "Completeness: Complete queryable with AllComplete target"
        ),
        key_expr,
        s1,
        s2,
        |b| b.complete(true),
        |b| b.target(QueryTarget::AllComplete),
        vec![RKind::Reply],
        vec![vec![RKind::Reply]],
    )
    .await;
    test(
        msg!(
            test_mode,
            "Completeness: Incomplete queryable with All target"
        ),
        key_expr,
        s1,
        s2,
        |b| b.complete(false),
        |b| b.target(QueryTarget::All),
        vec![RKind::Reply],
        vec![vec![RKind::Reply]],
    )
    .await;
    test(
        msg!(
            test_mode,
            "Completeness: Incomplete queryable with AllComplete target"
        ),
        key_expr,
        s1,
        s2,
        |b| b.complete(false),
        |b| b.target(QueryTarget::AllComplete),
        vec![RKind::Reply],
        vec![vec![]],
    )
    .await;
    test(
        msg!(test_mode, "Consolidation: None"),
        key_expr,
        s1,
        s2,
        |b| b,
        |b| b.consolidation(ConsolidationMode::None),
        vec![RKind::Reply, RKind::ReplyDel, RKind::ReplyErr],
        vec![vec![RKind::Reply, RKind::ReplyDel, RKind::ReplyErr]],
    )
    .await;
    test(
        msg!(test_mode, "Consolidation: Latest"),
        key_expr,
        s1,
        s2,
        |b| b,
        |b| b.consolidation(ConsolidationMode::Latest),
        vec![RKind::Reply, RKind::ReplyDel, RKind::ReplyErr],
        // There is currently no guarantee on reply ordering. ReplyErr being non consolidated, it arrives first.
        vec![vec![RKind::ReplyErr, RKind::ReplyDel]],
    )
    .await;
    test(
        msg!(test_mode, "Consolidation: Monotonic"),
        key_expr,
        s1,
        s2,
        |b| b,
        |b| b.consolidation(ConsolidationMode::Monotonic),
        vec![RKind::Reply, RKind::ReplyDel],
        // Due to timing, we might receive either just ReplyDel or both Reply and ReplyDel,
        // but thy should not be reordered.
        vec![vec![RKind::ReplyDel], vec![RKind::Reply, RKind::ReplyDel]],
    )
    .await;
    test(
        msg!(test_mode, "Consolidation: Monotonic 1"),
        key_expr,
        s1,
        s2,
        |b| b,
        |b| b.consolidation(ConsolidationMode::Monotonic),
        vec![RKind::ReplyDel, RKind::Reply],
        // Repeat to ensure that the order is preserved independently of the types of replies.
        vec![vec![RKind::Reply], vec![RKind::ReplyDel, RKind::Reply]],
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_queryable_different_sessions() {
    zenoh::init_log_from_env_or("error");

    // Declare session
    let s1 = ztimeout!(zenoh::open(zenoh::Config::default())).expect("Failed to open session s1");
    let s2 = ztimeout!(zenoh::open(zenoh::Config::default())).expect("Failed to open session s2");

    test_queryable_impl(
        &s1,
        &s2,
        "different sessions",
        "test/queryable/different_sessions",
    )
    .await;

    ztimeout!(s1.close()).expect("Failed to close session s1");
    ztimeout!(s2.close()).expect("Failed to close session s2");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_queryable_same_session() {
    zenoh::init_log_from_env_or("error");

    // Declare session
    let s1 = ztimeout!(zenoh::open(zenoh::Config::default())).expect("Failed to open session s1");

    test_queryable_impl(&s1, &s1, "same session", "test/queryable/same_session").await;

    ztimeout!(s1.close()).expect("Failed to close session s1");
}
