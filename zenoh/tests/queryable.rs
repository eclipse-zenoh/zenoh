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

use zenoh::{
    handlers::DefaultHandler,
    query::{Query, QueryableBuilder, Reply},
    sample::SampleKind,
    session::SessionGetBuilder,
    Session,
};
use zenoh_core::ztimeout;

const SLEEP: Duration = Duration::from_secs(1);
const TIMEOUT: Duration = Duration::from_secs(60);
const KEYEXPR: &str = "test/queryable";

#[derive(Debug, PartialEq, Clone, Copy)]
enum ReplyVariant {
    None,
    Reply,
    ReplyDel,
    ReplyErr,
}

// implement display for ReplyVariant
impl std::fmt::Display for ReplyVariant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReplyVariant::None => write!(f, "None"),
            ReplyVariant::Reply => write!(f, "Reply"),
            ReplyVariant::ReplyDel => write!(f, "ReplyDel"),
            ReplyVariant::ReplyErr => write!(f, "ReplyErr"),
        }
    }
}

impl<E> From<Result<Reply, E>> for ReplyVariant {
    fn from(result: Result<Reply, E>) -> Self {
        match result {
            Ok(reply) => reply.into(),
            Err(_) => ReplyVariant::None,
        }
    }
}

impl From<Reply> for ReplyVariant {
    fn from(reply: Reply) -> Self {
        match reply.result() {
            Ok(sample) => {
                if sample.kind() == SampleKind::Put {
                    ReplyVariant::Reply
                } else {
                    ReplyVariant::ReplyDel
                }
            }
            Err(_) => ReplyVariant::ReplyErr,
        }
    }
}

type QBuilder<'a> = QueryableBuilder<'a, 'a, DefaultHandler>;
type GBuilder<'a> = SessionGetBuilder<'a, 'a, DefaultHandler>;
async fn test<'a, QClosure, RClosure>(
    test_name: &'static str,
    q_session: &'a Session,
    r_session: &'a Session,
    q_builder: QClosure,
    r_builder: RClosure,
    expected: (ReplyVariant, ReplyVariant, ReplyVariant),
) where
    QClosure: FnOnce(QBuilder<'a>) -> QBuilder<'a>,
    RClosure: FnOnce(GBuilder<'a>) -> GBuilder<'a>,
{
    let _queryable = ztimeout!(q_builder(q_session.declare_queryable(KEYEXPR)).callback(
        move |query: Query| {
            tokio::spawn(async move {

                dbg!(query.key_expr());

                // send 3 variants of replies
                ztimeout!(query.reply(KEYEXPR, "reply")).unwrap_or_else(|_| {
                    panic!("{test_name} : failed to reply to query");
                });
                ztimeout!(query.reply_del(KEYEXPR)).unwrap_or_else(|_| {
                    panic!("{test_name} : failed to reply_del to query");
                });
                ztimeout!(query.reply_err("error")).unwrap_or_else(|_| {
                    panic!("{test_name} : failed to reply_err to query");
                });
            });
        }
    ))
    .unwrap_or_else(|_| panic!("{test_name} : failed to declare queryable"));

    tokio::time::sleep(SLEEP).await;

    let replies = ztimeout!(r_builder(r_session.get(KEYEXPR)))
        .unwrap_or_else(|_| panic!("{test_name} : failed to execute query"));

    let r0 = ztimeout!(replies.recv_async()).into();
    let r1 = ztimeout!(replies.recv_async()).into();
    let r2 = ztimeout!(replies.recv_async()).into();
    let (e0, e1, e2) = (expected.0, expected.1, expected.2);

    assert_eq!(
        (r0, r1, r2),
        (e0, e1, e2),
        "{test_name}: Expected replies ({e0},{e1},{e2}), got ({r0},{r1},{r2})",
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_queryable() {
    zenoh::init_log_from_env_or("error");

    // Declare session
    let s1 = ztimeout!(zenoh::open(zenoh::Config::default())).expect("Failed to open session s1");
    let s2 = ztimeout!(zenoh::open(zenoh::Config::default())).expect("Failed to open session s2");

    // These tests should receive responses
    test(
        "Basic queryable test, auto consolidation",
        &s1,
        &s2,
        |b| b,
        |b| b,
        (
            ReplyVariant::Reply,
            ReplyVariant::ReplyDel,
            ReplyVariant::ReplyErr,
        ),
    )
    .await;
    // test_ok(
    //     "Same session test",
    //     &s1,
    //     &s1.clone(),
    //     |s: &Session| s.declare_queryable(ke),
    //     |s: &Session| s.get(ke),
    // )
    // .await;

    // test_ok(
    //     "Complete queryable with All target",
    //     &s1,
    //     &s2,
    //     |s: &Session| s.declare_queryable(ke).complete(true),
    //     |s: &Session| s.get(ke).target(zenoh::query::QueryTarget::All),
    // )
    // .await;

    // test_ok(
    //     "Complete queryable with AllComplete target",
    //     &s1,
    //     &s2,
    //     |s: &Session| s.declare_queryable(ke).complete(true),
    //     |s: &Session| s.get(ke).target(zenoh::query::QueryTarget::AllComplete),
    // )
    // .await;

    // test_ok(
    //     "Incomplete queryable with All target",
    //     &s1,
    //     &s2,
    //     |s: &Session| s.declare_queryable(ke).complete(false),
    //     |s: &Session| s.get(ke).target(zenoh::query::QueryTarget::All),
    // )
    // .await;

    // test_miss(
    //     "Incomplete queryable with AllComplete target",
    //     &s1,
    //     &s2,
    //     |s: &Session| s.declare_queryable(ke).complete(false),
    //     |s: &Session| s.get(ke).target(zenoh::query::QueryTarget::AllComplete),
    // )
    // .await;

    // test_miss(
    //     "Incomplete queryable with AllComplete target, same session",
    //     &s1,
    //     &s1.clone(),
    //     |s: &Session| s.declare_queryable(ke).complete(false),
    //     |s: &Session| s.get(ke).target(zenoh::query::QueryTarget::AllComplete),
    // )
    // .await;

    ztimeout!(s1.close()).expect("Failed to close session s1");
    ztimeout!(s2.close()).expect("Failed to close session s2");
}
