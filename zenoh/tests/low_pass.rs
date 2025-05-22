//
// Copyright (c) 2025 ZettaScale Technology
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

#![cfg(feature = "internal_config")]
#![cfg(unix)]

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use nonempty_collections::{nev, NEVec};
#[cfg(feature = "unstable")]
use zenoh::query::{ConsolidationMode, Reply};
use zenoh::{bytes::ZBytes, Config, Wait};
use zenoh_config::{InterceptorFlow, InterceptorLink, LowPassFilterConf, LowPassFilterMessage};

static SMALL_MSG_STR: &str = "S";
static BIG_MSG_STR: &str = "B";
static BIG_MSG_SIZE: usize = 50;
static SMALL_MSG_SIZE: usize = 5;
static LOWPASS_RULE_BYTES: usize = 25;

static DECLARATION_DELAY_MS: u64 = 250;
static MESSAGES_DELAY_MS: u64 = 1000;

static TEST_PORTS_TCP: [u16; 4] = [31048, 31049, 31050, 31051];

#[derive(Default)]
struct LowPassTestResult {
    small_put: AtomicBool,
    big_put: AtomicBool,
    small_del: AtomicBool,
    big_del: AtomicBool,
    #[cfg(feature = "unstable")]
    small_query: AtomicBool,
    #[cfg(feature = "unstable")]
    big_query: AtomicBool,
    #[cfg(feature = "unstable")]
    small_reply_put: AtomicBool,
    #[cfg(feature = "unstable")]
    big_reply_put: AtomicBool,
    #[cfg(feature = "unstable")]
    small_reply_del: AtomicBool,
    #[cfg(feature = "unstable")]
    big_reply_del: AtomicBool,
    #[cfg(feature = "unstable")]
    small_reply_err: AtomicBool,
    #[cfg(feature = "unstable")]
    big_reply_err: AtomicBool,
}

impl LowPassTestResult {
    fn all_small(&self) -> bool {
        let all = self.small_put.load(Ordering::Relaxed) && self.small_del.load(Ordering::Relaxed);
        #[cfg(feature = "unstable")]
        let all = all
            && self.small_query.load(Ordering::Relaxed)
            && self.small_reply_put.load(Ordering::Relaxed)
            && self.small_reply_del.load(Ordering::Relaxed)
            && self.small_reply_err.load(Ordering::Relaxed);
        all
    }

    #[cfg(feature = "unstable")]
    fn big_query(&self) -> bool {
        self.big_query.load(Ordering::Relaxed)
    }

    fn big_put(&self) -> bool {
        self.big_put.load(Ordering::Relaxed)
    }

    fn big_del(&self) -> bool {
        self.big_del.load(Ordering::Relaxed)
    }

    #[cfg(feature = "unstable")]
    fn all_big_replies(&self) -> bool {
        self.big_reply_put.load(Ordering::Relaxed)
            && self.big_reply_del.load(Ordering::Relaxed)
            && self.big_reply_err.load(Ordering::Relaxed)
    }

    #[cfg(feature = "unstable")]
    fn no_big_replies(&self) -> bool {
        !(self.big_reply_put.load(Ordering::Relaxed)
            || self.big_reply_del.load(Ordering::Relaxed)
            || self.big_reply_err.load(Ordering::Relaxed))
    }
}

#[test]
fn lowpass_query_test() {
    zenoh::init_log_from_env_or("error");

    let nominal_assertions = |test_res: &LowPassTestResult| {
        assert!(test_res.all_small());
        #[cfg(feature = "unstable")]
        assert!(test_res.all_big_replies());
        assert!(test_res.big_put());
        assert!(test_res.big_del());
    };
    let assert_applied = |test_res: &LowPassTestResult| {
        nominal_assertions(test_res);
        #[cfg(feature = "unstable")]
        assert!(!test_res.big_query());
    };
    let assert_not_applied = |test_res: &LowPassTestResult| {
        nominal_assertions(test_res);
        #[cfg(feature = "unstable")]
        assert!(test_res.big_query());
    };

    lowpass_query_filter_test(InterceptorFlow::Ingress, None, None, assert_applied);
    lowpass_query_filter_test(InterceptorFlow::Egress, None, None, assert_applied);

    lowpass_query_filter_test(
        InterceptorFlow::Ingress,
        None,
        Some(nev![InterceptorLink::Tcp]),
        assert_applied,
    );
    lowpass_query_filter_test(
        InterceptorFlow::Egress,
        None,
        Some(nev![InterceptorLink::Tcp]),
        assert_applied,
    );

    lowpass_query_filter_test(
        InterceptorFlow::Ingress,
        None,
        Some(nev![InterceptorLink::Udp]),
        assert_not_applied,
    );
    lowpass_query_filter_test(
        InterceptorFlow::Egress,
        None,
        Some(nev![InterceptorLink::Udp]),
        assert_not_applied,
    );
}

#[test]
fn lowpass_reply_test() {
    zenoh::init_log_from_env_or("error");

    let nominal_assertions = |test_res: &LowPassTestResult| {
        assert!(test_res.all_small());
        #[cfg(feature = "unstable")]
        assert!(test_res.big_query());
        assert!(test_res.big_put());
        assert!(test_res.big_del());
    };
    let assert_applied = |test_res: &LowPassTestResult| {
        nominal_assertions(test_res);
        #[cfg(feature = "unstable")]
        assert!(test_res.no_big_replies());
    };
    let assert_not_applied = |test_res: &LowPassTestResult| {
        nominal_assertions(test_res);
        #[cfg(feature = "unstable")]
        assert!(test_res.all_big_replies());
    };

    lowpass_reply_filter_test(InterceptorFlow::Ingress, None, None, assert_applied);
    lowpass_reply_filter_test(InterceptorFlow::Egress, None, None, assert_applied);

    lowpass_reply_filter_test(
        InterceptorFlow::Ingress,
        None,
        Some(nev![InterceptorLink::Tcp]),
        assert_applied,
    );
    lowpass_reply_filter_test(
        InterceptorFlow::Egress,
        None,
        Some(nev![InterceptorLink::Tcp]),
        assert_applied,
    );

    lowpass_reply_filter_test(
        InterceptorFlow::Ingress,
        None,
        Some(nev![InterceptorLink::Udp]),
        assert_not_applied,
    );
    lowpass_reply_filter_test(
        InterceptorFlow::Egress,
        None,
        Some(nev![InterceptorLink::Udp]),
        assert_not_applied,
    );
}

#[test]
fn lowpass_put_test() {
    zenoh::init_log_from_env_or("error");

    let nominal_assertions = |test_res: &LowPassTestResult| {
        assert!(test_res.all_small());
        #[cfg(feature = "unstable")]
        assert!(test_res.all_big_replies());
        #[cfg(feature = "unstable")]
        assert!(test_res.big_query());
        assert!(test_res.big_del());
    };
    let assert_applied = |test_res: &LowPassTestResult| {
        nominal_assertions(test_res);
        assert!(!test_res.big_put());
    };
    let assert_not_applied = |test_res: &LowPassTestResult| {
        nominal_assertions(test_res);
        assert!(test_res.big_put());
    };

    lowpass_put_filter_test(InterceptorFlow::Ingress, None, None, assert_applied);
    lowpass_put_filter_test(InterceptorFlow::Egress, None, None, assert_applied);

    lowpass_put_filter_test(
        InterceptorFlow::Ingress,
        None,
        Some(nev![InterceptorLink::Tcp]),
        assert_applied,
    );
    lowpass_put_filter_test(
        InterceptorFlow::Egress,
        None,
        Some(nev![InterceptorLink::Tcp]),
        assert_applied,
    );

    lowpass_put_filter_test(
        InterceptorFlow::Ingress,
        None,
        Some(nev![InterceptorLink::Udp]),
        assert_not_applied,
    );
    lowpass_put_filter_test(
        InterceptorFlow::Egress,
        None,
        Some(nev![InterceptorLink::Udp]),
        assert_not_applied,
    );
}

#[test]
fn lowpass_del_test() {
    zenoh::init_log_from_env_or("error");

    let nominal_assertions = |test_res: &LowPassTestResult| {
        assert!(test_res.all_small());
        #[cfg(feature = "unstable")]
        assert!(test_res.all_big_replies());
        #[cfg(feature = "unstable")]
        assert!(test_res.big_query());
        assert!(test_res.big_put());
    };
    let assert_applied = |test_res: &LowPassTestResult| {
        nominal_assertions(test_res);
        assert!(!test_res.big_del());
    };
    let assert_not_applied = |test_res: &LowPassTestResult| {
        nominal_assertions(test_res);
        assert!(test_res.big_del());
    };

    lowpass_del_filter_test(InterceptorFlow::Ingress, None, None, assert_applied);
    lowpass_del_filter_test(InterceptorFlow::Egress, None, None, assert_applied);

    lowpass_del_filter_test(
        InterceptorFlow::Ingress,
        None,
        Some(nev![InterceptorLink::Tcp]),
        assert_applied,
    );
    lowpass_del_filter_test(
        InterceptorFlow::Egress,
        None,
        Some(nev![InterceptorLink::Tcp]),
        assert_applied,
    );

    lowpass_del_filter_test(
        InterceptorFlow::Ingress,
        None,
        Some(nev![InterceptorLink::Udp]),
        assert_not_applied,
    );
    lowpass_del_filter_test(
        InterceptorFlow::Egress,
        None,
        Some(nev![InterceptorLink::Udp]),
        assert_not_applied,
    );
}

fn lowpass_query_filter_test(
    flow: InterceptorFlow,
    interfaces: Option<NEVec<String>>,
    link_protocols: Option<NEVec<InterceptorLink>>,
    assertions: impl Fn(&LowPassTestResult),
) {
    let prefix = "test/lowpass_query";
    let locator = format!("tcp/127.0.0.1:{}", TEST_PORTS_TCP[0]);

    let lpf_config = LowPassFilterConf {
        id: None,
        flows: Some(nev![flow]),
        interfaces,
        link_protocols,
        messages: nev![LowPassFilterMessage::Query],
        key_exprs: nev![format!("{prefix}/**").try_into().unwrap()],
        size_limit: LOWPASS_RULE_BYTES,
    };

    let (query_config, queryable_config) = build_config(vec![lpf_config], flow);
    let test_res =
        lowpass_pub_sub_query_reply_test(&locator, query_config, queryable_config, prefix);

    assertions(&test_res);
}

fn lowpass_reply_filter_test(
    flow: InterceptorFlow,
    interfaces: Option<NEVec<String>>,
    link_protocols: Option<NEVec<InterceptorLink>>,
    assertions: impl Fn(&LowPassTestResult),
) {
    let prefix = "test/lowpass_reply";
    let locator = format!("tcp/127.0.0.1:{}", TEST_PORTS_TCP[1]);

    let lpf_config = LowPassFilterConf {
        id: None,
        flows: Some(nev![flow]),
        interfaces,
        link_protocols,
        messages: nev![LowPassFilterMessage::Reply],
        key_exprs: nev![format!("{prefix}/**").try_into().unwrap()],
        size_limit: LOWPASS_RULE_BYTES,
    };

    let (queryable_config, query_config) = build_config(vec![lpf_config], flow);
    let test_res =
        lowpass_pub_sub_query_reply_test(&locator, query_config, queryable_config, prefix);

    assertions(&test_res);
}

fn lowpass_put_filter_test(
    flow: InterceptorFlow,
    interfaces: Option<NEVec<String>>,
    link_protocols: Option<NEVec<InterceptorLink>>,
    assertions: impl Fn(&LowPassTestResult),
) {
    let prefix = "test/lowpass_put";
    let locator = format!("tcp/127.0.0.1:{}", TEST_PORTS_TCP[2]);

    let lpf_config = LowPassFilterConf {
        id: None,
        flows: Some(nev![flow]),
        interfaces,
        link_protocols,
        messages: nev![LowPassFilterMessage::Put],
        key_exprs: nev![format!("{prefix}/**").try_into().unwrap()],
        size_limit: LOWPASS_RULE_BYTES,
    };

    let (pub_config, sub_config) = build_config(vec![lpf_config], flow);
    let test_res = lowpass_pub_sub_query_reply_test(&locator, pub_config, sub_config, prefix);

    assertions(&test_res);
}

fn lowpass_del_filter_test(
    flow: InterceptorFlow,
    interfaces: Option<NEVec<String>>,
    link_protocols: Option<NEVec<InterceptorLink>>,
    assertions: impl Fn(&LowPassTestResult),
) {
    let prefix = "test/lowpass_del";
    let locator = format!("tcp/127.0.0.1:{}", TEST_PORTS_TCP[3]);

    let lpf_config = LowPassFilterConf {
        id: None,
        flows: Some(nev![flow]),
        interfaces,
        link_protocols,
        messages: nev![LowPassFilterMessage::Delete],
        key_exprs: nev![format!("{prefix}/**").try_into().unwrap()],
        size_limit: LOWPASS_RULE_BYTES,
    };

    let (pub_config, sub_config) = build_config(vec![lpf_config], flow);
    let test_res = lowpass_pub_sub_query_reply_test(&locator, pub_config, sub_config, prefix);

    assertions(&test_res);
}

fn build_config(lpf_config: Vec<LowPassFilterConf>, flow: InterceptorFlow) -> (Config, Config) {
    let mut sender_config = Config::default();
    sender_config
        .scouting
        .multicast
        .set_enabled(Some(false))
        .unwrap();

    let mut receiver_config = Config::default();
    receiver_config
        .scouting
        .multicast
        .set_enabled(Some(false))
        .unwrap();

    match flow {
        InterceptorFlow::Egress => sender_config.set_low_pass_filter(lpf_config).unwrap(),
        InterceptorFlow::Ingress => receiver_config.set_low_pass_filter(lpf_config).unwrap(),
    };

    (sender_config, receiver_config)
}

fn set_locators(locator: &str, listen_config: &mut Config, connect_config: &mut Config) {
    listen_config
        .listen
        .endpoints
        .set(vec![locator.parse().unwrap()])
        .unwrap();
    connect_config
        .connect
        .endpoints
        .set(vec![locator.parse().unwrap()])
        .unwrap();
}

fn lowpass_pub_sub_query_reply_test(
    locator: &str,
    mut writer_config: Config,
    mut reader_config: Config,
    ke_prefix: &str,
) -> Arc<LowPassTestResult> {
    let test_results = Arc::new(LowPassTestResult::default());

    let small_payload: ZBytes = (0..SMALL_MSG_SIZE)
        .map(|_| SMALL_MSG_STR)
        .collect::<String>()
        .into();
    let big_payload: ZBytes = (0..BIG_MSG_SIZE)
        .map(|_| BIG_MSG_STR)
        .collect::<String>()
        .into();

    set_locators(locator, &mut reader_config, &mut writer_config);

    let reader_session = zenoh::open(reader_config).wait().unwrap();
    let _sub = reader_session
        .declare_subscriber(format!("{ke_prefix}/*"))
        .callback({
            let test_results = test_results.clone();
            move |sample| match sample.kind() {
                zenoh::sample::SampleKind::Put => {
                    let content = sample.payload().try_to_string().unwrap();
                    if content.contains(BIG_MSG_STR) {
                        test_results.big_put.store(true, Ordering::Relaxed);
                    } else if content.contains(SMALL_MSG_STR) {
                        test_results.small_put.store(true, Ordering::Relaxed);
                    }
                }
                zenoh::sample::SampleKind::Delete => {
                    let content = sample.attachment().unwrap().try_to_string().unwrap();
                    if content.contains(BIG_MSG_STR) {
                        test_results.big_del.store(true, Ordering::Relaxed);
                    } else if content.contains(SMALL_MSG_STR) {
                        test_results.small_del.store(true, Ordering::Relaxed);
                    }
                }
            }
        })
        .wait()
        .unwrap();

    #[cfg(feature = "unstable")]
    let _queryable = reader_session
        .declare_queryable(format!("{ke_prefix}/*"))
        .callback({
            let test_results = test_results.clone();
            let small_payload = small_payload.clone();
            let big_payload = big_payload.clone();

            move |query| {
                let content = query.payload().unwrap().try_to_string().unwrap();
                if content.contains(BIG_MSG_STR) {
                    test_results.big_query.store(true, Ordering::Relaxed);
                } else if content.contains(SMALL_MSG_STR) {
                    test_results.small_query.store(true, Ordering::Relaxed);
                }
                query
                    .reply(query.key_expr(), small_payload.clone())
                    .wait()
                    .unwrap();
                query
                    .reply(query.key_expr(), big_payload.clone())
                    .wait()
                    .unwrap();
                query
                    .reply_del(query.key_expr())
                    .attachment(small_payload.clone())
                    .wait()
                    .unwrap();
                query
                    .reply_del(query.key_expr())
                    .attachment(big_payload.clone())
                    .wait()
                    .unwrap();
                query.reply_err(small_payload.clone()).wait().unwrap();
                query.reply_err(big_payload.clone()).wait().unwrap();
            }
        })
        .wait()
        .unwrap();

    let writer_session = zenoh::open(writer_config).wait().unwrap();
    let publ = writer_session
        .declare_publisher(format!("{ke_prefix}/pub"))
        .wait()
        .unwrap();
    #[cfg(feature = "unstable")]
    let querier = writer_session
        .declare_querier(format!("{ke_prefix}/query"))
        .consolidation(ConsolidationMode::None)
        .wait()
        .unwrap();

    std::thread::sleep(std::time::Duration::from_millis(DECLARATION_DELAY_MS));

    publ.put(small_payload.clone()).wait().unwrap();
    publ.put(big_payload.clone()).wait().unwrap();
    publ.delete()
        .attachment(small_payload.clone())
        .wait()
        .unwrap();
    publ.delete()
        .attachment(big_payload.clone())
        .wait()
        .unwrap();

    #[cfg(feature = "unstable")]
    let query_callback = {
        let test_results = test_results.clone();
        move |reply: Reply| match reply.into_result() {
            Ok(sample) => match sample.kind() {
                zenoh::sample::SampleKind::Put => {
                    let content = sample.payload().try_to_string().unwrap();
                    if content.contains(BIG_MSG_STR) {
                        test_results.big_reply_put.store(true, Ordering::Relaxed);
                    } else if content.contains(SMALL_MSG_STR) {
                        test_results.small_reply_put.store(true, Ordering::Relaxed);
                    }
                }
                zenoh::sample::SampleKind::Delete => {
                    let content = sample.attachment().unwrap().try_to_string().unwrap();
                    if content.contains(BIG_MSG_STR) {
                        test_results.big_reply_del.store(true, Ordering::Relaxed);
                    } else if content.contains(SMALL_MSG_STR) {
                        test_results.small_reply_del.store(true, Ordering::Relaxed);
                    }
                }
            },
            Err(err) => {
                let content = err.payload().try_to_string().unwrap();
                if content.contains(SMALL_MSG_STR) {
                    test_results.small_reply_err.store(true, Ordering::Relaxed);
                } else if content.contains(BIG_MSG_STR) {
                    test_results.big_reply_err.store(true, Ordering::Relaxed);
                }
            }
        }
    };
    #[cfg(feature = "unstable")]
    querier
        .get()
        .payload(small_payload.clone())
        .callback(query_callback.clone())
        .wait()
        .unwrap();
    #[cfg(feature = "unstable")]
    querier
        .get()
        .payload(big_payload.clone())
        .callback(query_callback.clone())
        .wait()
        .unwrap();

    std::thread::sleep(std::time::Duration::from_millis(MESSAGES_DELAY_MS));
    writer_session.close().wait().unwrap();
    reader_session.close().wait().unwrap();
    test_results
}
