//
// Copyright (c) 2024 ZettaScale Technology
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

use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
};

use nonempty_collections::nev;
use zenoh::{key_expr::KeyExpr, query::ConsolidationMode, Config, Wait};
use zenoh_config::{
    DownsamplingItemConf, DownsamplingMessage, DownsamplingRuleConf, InterceptorFlow,
};

// Tokio's time granularity on different platforms
#[cfg(target_os = "windows")]
static MINIMAL_SLEEP_INTERVAL_MS: u64 = 17;
#[cfg(not(target_os = "windows"))]
static MINIMAL_SLEEP_INTERVAL_MS: u64 = 2;

static REPEAT: usize = 3;
static WARMUP_MS: u64 = 500;

fn build_config(
    locator: &str,
    ds_config: Vec<DownsamplingItemConf>,
    flow: InterceptorFlow,
) -> (Config, Config) {
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

    receiver_config
        .listen
        .endpoints
        .set(vec![locator.parse().unwrap()])
        .unwrap();
    sender_config
        .connect
        .endpoints
        .set(vec![locator.parse().unwrap()])
        .unwrap();

    match flow {
        InterceptorFlow::Egress => sender_config.set_downsampling(ds_config).unwrap(),
        InterceptorFlow::Ingress => receiver_config.set_downsampling(ds_config).unwrap(),
    };

    (sender_config, receiver_config)
}

fn downsampling_pub_sub_test<F>(
    pub_config: Config,
    sub_config: Config,
    ke_prefix: &str,
    ke_of_rates: Vec<KeyExpr<'static>>,
    rate_check: F,
) where
    F: Fn(KeyExpr<'_>, usize) -> bool + Send + 'static,
{
    type Counters<'a> = Arc<HashMap<KeyExpr<'a>, AtomicUsize>>;
    let counters: Counters = Arc::new(
        ke_of_rates
            .clone()
            .into_iter()
            .map(|ke| (ke, AtomicUsize::new(0)))
            .collect(),
    );

    let sub_session = zenoh::open(sub_config).wait().unwrap();
    let _sub = sub_session
        .declare_subscriber(format!("{ke_prefix}/*"))
        .callback({
            let counters = counters.clone();
            move |sample| {
                counters
                    .get(sample.key_expr())
                    .map(|ctr| ctr.fetch_add(1, Ordering::SeqCst));
            }
        })
        .wait()
        .unwrap();

    let is_terminated = Arc::new(AtomicBool::new(false));
    let c_is_terminated = is_terminated.clone();
    let handle = std::thread::spawn(move || {
        let pub_session = zenoh::open(pub_config).wait().unwrap();
        let publishers: Vec<_> = ke_of_rates
            .into_iter()
            .map(|ke| pub_session.declare_publisher(ke).wait().unwrap())
            .collect();
        let interval = std::time::Duration::from_millis(MINIMAL_SLEEP_INTERVAL_MS);
        while !c_is_terminated.load(Ordering::SeqCst) {
            publishers.iter().for_each(|publ| {
                publ.put("message").wait().unwrap();
            });
            std::thread::sleep(interval);
        }
    });

    std::thread::sleep(std::time::Duration::from_millis(WARMUP_MS));
    counters.iter().for_each(|(_, ctr)| {
        ctr.swap(0, Ordering::SeqCst);
    });

    for _ in 0..REPEAT {
        std::thread::sleep(std::time::Duration::from_secs(1));
        counters.iter().for_each(|(ke, ctr)| {
            let rate = ctr.swap(0, Ordering::SeqCst);
            if !rate_check(ke.into(), rate) {
                panic!("The test failed on the {ke:?} at the rate of {rate:?}");
            }
        });
    }

    let _ = is_terminated.swap(true, Ordering::SeqCst);
    if let Err(err) = handle.join() {
        panic!("Failed to join the handle due to {err:?}");
    }
}

fn downsampling_by_keyexpr_impl(flow: InterceptorFlow) {
    let ke_prefix = "test/downsamples_by_keyexp";
    let locator = "tcp/127.0.0.1:31445";

    let ke_10hz: KeyExpr = format!("{ke_prefix}/10hz").try_into().unwrap();
    let ke_20hz: KeyExpr = format!("{ke_prefix}/20hz").try_into().unwrap();

    let ds_config = DownsamplingItemConf {
        id: None,
        flows: Some(nev![flow]),
        interfaces: None,
        link_protocols: None,
        messages: nev![DownsamplingMessage::Put],
        rules: nev![
            DownsamplingRuleConf {
                key_expr: ke_10hz.clone().into(),
                freq: 10.0,
            },
            DownsamplingRuleConf {
                key_expr: ke_20hz.clone().into(),
                freq: 20.0,
            },
        ],
    };

    let ke_of_rates: Vec<KeyExpr<'static>> = ds_config
        .rules
        .iter()
        .map(|x| x.key_expr.clone().into())
        .collect();

    let rate_check = move |ke: KeyExpr, rate: usize| -> bool {
        tracing::info!("keyexpr: {ke}, rate: {rate}");
        if ke == ke_10hz {
            rate > 0 && rate <= 10 + 1
        } else if ke == ke_20hz {
            rate > 0 && rate <= 20 + 1
        } else {
            tracing::error!("Shouldn't reach this case. Invalid keyexpr {ke} detected.");
            false
        }
    };

    let (pub_config, sub_config) = build_config(locator, vec![ds_config], flow);

    downsampling_pub_sub_test(pub_config, sub_config, ke_prefix, ke_of_rates, rate_check);
}

#[test]
fn downsampling_by_keyexpr() {
    zenoh::init_log_from_env_or("error");
    downsampling_by_keyexpr_impl(InterceptorFlow::Ingress);
    downsampling_by_keyexpr_impl(InterceptorFlow::Egress);
}

#[cfg(unix)]
fn downsampling_by_interface_impl(flow: InterceptorFlow) {
    let ke_prefix = "test/downsamples_by_interface";
    let locator = "tcp/127.0.0.1:31446";

    let ke_10hz: KeyExpr = format!("{ke_prefix}/10hz").try_into().unwrap();
    let ke_no_effect: KeyExpr = format!("{ke_prefix}/no_effect").try_into().unwrap();
    let ke_of_rates: Vec<KeyExpr<'static>> = vec![ke_10hz.clone(), ke_no_effect.clone()];

    let ds_config = vec![
        DownsamplingItemConf {
            id: Some("someid".to_string()),
            flows: Some(nev![flow]),
            interfaces: Some(nev!["lo".to_string(), "lo0".to_string()]),
            link_protocols: None,
            messages: nev![DownsamplingMessage::Put],
            rules: nev![DownsamplingRuleConf {
                key_expr: ke_10hz.clone().into(),
                freq: 10.0,
            }],
        },
        DownsamplingItemConf {
            id: None,
            flows: Some(nev![flow]),
            interfaces: Some(nev!["some_unknown_interface".to_string()]),
            link_protocols: None,
            messages: nev![DownsamplingMessage::Put],
            rules: nev![DownsamplingRuleConf {
                key_expr: ke_no_effect.clone().into(),
                freq: 10.0,
            }],
        },
    ];

    let rate_check = move |ke: KeyExpr, rate: usize| -> bool {
        tracing::info!("keyexpr: {ke}, rate: {rate}");
        if ke == ke_10hz {
            rate > 0 && rate <= 10 + 1
        } else if ke == ke_no_effect {
            rate > 10
        } else {
            tracing::error!("Shouldn't reach this case. Invalid keyexpr {ke} detected.");
            false
        }
    };

    let (pub_config, sub_config) = build_config(locator, ds_config, flow);

    downsampling_pub_sub_test(pub_config, sub_config, ke_prefix, ke_of_rates, rate_check);
}

#[cfg(unix)]
#[test]
fn downsampling_by_interface() {
    zenoh::init_log_from_env_or("error");
    downsampling_by_interface_impl(InterceptorFlow::Ingress);
    downsampling_by_interface_impl(InterceptorFlow::Egress);
}

#[cfg(unix)]
fn downsampling_by_protocol_impl(flow: InterceptorFlow) {
    use zenoh_config::InterceptorLink;

    let ke_prefix = "test/downsamples_by_interface";
    let locator = "tcp/127.0.0.1:31447";

    let ke_10hz: KeyExpr = format!("{ke_prefix}/10hz").try_into().unwrap();
    let ke_no_effect: KeyExpr = format!("{ke_prefix}/no_effect").try_into().unwrap();
    let ke_of_rates: Vec<KeyExpr<'static>> = vec![ke_10hz.clone(), ke_no_effect.clone()];

    let ds_config = vec![
        DownsamplingItemConf {
            id: Some("someid".to_string()),
            flows: Some(nev![flow]),
            interfaces: None,
            link_protocols: Some(nev![InterceptorLink::Tcp]),
            messages: nev![DownsamplingMessage::Put],
            rules: nev![DownsamplingRuleConf {
                key_expr: ke_10hz.clone().into(),
                freq: 10.0,
            }],
        },
        DownsamplingItemConf {
            id: None,
            flows: Some(nev![flow]),
            interfaces: None,
            link_protocols: Some(nev![InterceptorLink::Serial]),
            messages: nev![DownsamplingMessage::Put],
            rules: nev![DownsamplingRuleConf {
                key_expr: ke_no_effect.clone().into(),
                freq: 10.0,
            }],
        },
    ];

    let rate_check = move |ke: KeyExpr, rate: usize| -> bool {
        tracing::info!("keyexpr: {ke}, rate: {rate}");
        if ke == ke_10hz {
            rate > 0 && rate <= 10 + 1
        } else if ke == ke_no_effect {
            rate > 10
        } else {
            tracing::error!("Shouldn't reach this case. Invalid keyexpr {ke} detected.");
            false
        }
    };

    let (pub_config, sub_config) = build_config(locator, ds_config, flow);

    downsampling_pub_sub_test(pub_config, sub_config, ke_prefix, ke_of_rates, rate_check);
}

#[test]
fn downsampling_by_protocol() {
    zenoh::init_log_from_env_or("error");
    downsampling_by_protocol_impl(InterceptorFlow::Ingress);
    downsampling_by_protocol_impl(InterceptorFlow::Egress);
}

#[test]
#[should_panic(expected = "unknown variant `down`")]
fn downsampling_config_error_wrong_strategy() {
    zenoh::init_log_from_env_or("error");

    let mut config = Config::default();
    config
        .insert_json5(
            "downsampling",
            r#"
              [
                {
                  flows: ["down"],
                  messages: ["put"],
                  rules: [
                    { key_expr: "test/downsamples_by_keyexp/r100", freq: 10, },
                    { key_expr: "test/downsamples_by_keyexp/r50", freq: 20, }
                  ],
                },
              ]
            "#,
        )
        .unwrap();

    zenoh::open(config).wait().unwrap();
}

#[test]
#[should_panic(expected = "Invalid Downsampling config: id 'REPEATED' is repeated")]
fn downsampling_config_error_repeated_id() {
    zenoh::init_log_from_env_or("error");

    let mut config = Config::default();
    config
        .insert_json5(
            "downsampling",
            r#"
              [
                {
                  id: "REPEATED",
                  flows: ["egress"],
                  messages: ["put"],
                  rules: [
                    { key_expr: "test/downsamples_by_keyexp/r100", freq: 10, },
                  ],
                },
                {
                  id: "REPEATED",
                  flows: ["ingress"],
                  messages: ["put"],
                  rules: [
                    { key_expr: "test/downsamples_by_keyexp/r50", freq: 20, }
                  ],
                },
              ]
            "#,
        )
        .unwrap();

    zenoh::open(config).wait().unwrap();
}

fn downsampling_query_reply_test(
    query_config: Config,
    queryable_config: Config,
    queryable_ke: &str,
    reply_counter: Arc<AtomicUsize>,
    nb_queries: usize,
) {
    let query_session = zenoh::open(query_config).wait().unwrap();
    let queryable_session = zenoh::open(queryable_config).wait().unwrap();

    let response_ke = queryable_ke.to_owned();
    queryable_session
        .declare_queryable(queryable_ke)
        .callback(move |query| {
            query.reply(&response_ke, "".as_bytes()).wait().unwrap();
        })
        .background()
        .wait()
        .unwrap();

    std::thread::sleep(std::time::Duration::from_secs(1));

    let mut handles = Vec::new();
    for _ in 0..nb_queries {
        let queryable_ke = queryable_ke.to_owned();
        let query_session = query_session.clone();
        let reply_counter = reply_counter.clone();
        let handle = std::thread::spawn(move || {
            let query = query_session
                .get(&queryable_ke)
                .consolidation(ConsolidationMode::None)
                .timeout(std::time::Duration::from_secs(5))
                .wait()
                .unwrap();
            while let Ok(reply) = query.recv() {
                if reply.into_result().is_ok() {
                    reply_counter.fetch_add(1, Ordering::SeqCst);
                }
            }
        });
        handles.push(handle);
    }
    for handle in handles {
        handle.join().unwrap();
    }
    query_session.close().wait().unwrap();
    queryable_session.close().wait().unwrap();
}

fn downsampling_query_rate_test(flow: InterceptorFlow) {
    let queryable_ke = "test/downsamples_query";
    let locator = "tcp/127.0.0.1:31448";

    let ds_config = DownsamplingItemConf {
        id: None,
        flows: Some(nev![flow]),
        interfaces: None,
        link_protocols: None,
        messages: nev![DownsamplingMessage::Query],
        rules: nev![DownsamplingRuleConf {
            key_expr: queryable_ke.try_into().unwrap(),
            freq: 0.01,
        }],
    };

    let (query_config, queryable_config) = build_config(locator, vec![ds_config], flow);
    let reply_counter = Arc::new(AtomicUsize::new(0));
    downsampling_query_reply_test(
        query_config,
        queryable_config,
        queryable_ke,
        reply_counter.clone(),
        2,
    );
    assert!(reply_counter.load(Ordering::SeqCst) == 1);
}

fn downsampling_reply_rate_test(flow: InterceptorFlow) {
    let queryable_ke = "test/downsamples_reply";
    let locator = "tcp/127.0.0.1:31449";

    let ds_config = DownsamplingItemConf {
        id: None,
        flows: Some(nev![flow]),
        interfaces: None,
        link_protocols: None,
        messages: nev![DownsamplingMessage::Reply],
        rules: nev![DownsamplingRuleConf {
            key_expr: queryable_ke.try_into().unwrap(),
            freq: 0.01,
        }],
    };

    let (queryable_config, query_config) = build_config(locator, vec![ds_config], flow);
    let reply_counter = Arc::new(AtomicUsize::new(0));
    downsampling_query_reply_test(
        query_config,
        queryable_config,
        queryable_ke,
        reply_counter.clone(),
        2,
    );

    assert!(reply_counter.load(Ordering::SeqCst) == 1);
}

#[test]
fn downsampling_query_test() {
    zenoh::init_log_from_env_or("error");
    downsampling_query_rate_test(InterceptorFlow::Ingress);
    downsampling_query_rate_test(InterceptorFlow::Egress);
}

#[test]
fn downsampling_reply_test() {
    zenoh::init_log_from_env_or("error");
    downsampling_reply_rate_test(InterceptorFlow::Ingress);
    downsampling_reply_rate_test(InterceptorFlow::Egress);
}
