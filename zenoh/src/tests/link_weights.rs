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

use std::{
    any::Any,
    str::{self, FromStr},
};

use zenoh_buffers::ZBuf;
use zenoh_config::{InterceptorFlow, LinkWeight};
use zenoh_keyexpr::keyexpr;
use zenoh_protocol::{
    network::{NetworkBodyMut, NetworkMessageMut, Push},
    zenoh::PushBody,
};
use zenoh_transport::{multicast::TransportMulticast, unicast::TransportUnicast};

use crate::net::routing::{interceptor::*, RoutingContext};

#[derive(Clone)]
struct LinkTraceInterceptorConf {
    flow: InterceptorFlow,
}

fn link_trace_interceptor_factories(
    config: &Vec<LinkTraceInterceptorConf>,
) -> Vec<InterceptorFactory> {
    let mut res: Vec<InterceptorFactory> = vec![];
    for c in config {
        res.push(Box::new(LinkTraceInterceptorFactory::new(c.clone())));
    }
    res
}

struct LinkTraceInterceptorFactory {
    conf: LinkTraceInterceptorConf,
}

impl LinkTraceInterceptorFactory {
    pub fn new(conf: LinkTraceInterceptorConf) -> Self {
        Self { conf }
    }
}

impl InterceptorFactoryTrait for LinkTraceInterceptorFactory {
    fn new_transport_unicast(
        &self,
        transport: &TransportUnicast,
    ) -> (Option<IngressInterceptor>, Option<EgressInterceptor>) {
        let zid = transport
            .get_zid()
            .map(|z| z.to_string())
            .unwrap_or_default();
        match self.conf.flow {
            InterceptorFlow::Egress => (
                None,
                Some(Box::new(LinkTraceInterceptor { zid: zid.clone() })),
            ),
            InterceptorFlow::Ingress => (
                Some(Box::new(LinkTraceInterceptor { zid: zid.clone() })),
                None,
            ),
        }
    }

    fn new_transport_multicast(
        &self,
        _transport: &TransportMulticast,
    ) -> Option<EgressInterceptor> {
        None
    }

    fn new_peer_multicast(&self, _transport: &TransportMulticast) -> Option<IngressInterceptor> {
        None
    }
}

struct LinkTraceInterceptor {
    zid: String,
}

impl InterceptorTrait for LinkTraceInterceptor {
    fn compute_keyexpr_cache(&self, _key_expr: &keyexpr) -> Option<Box<dyn Any + Send + Sync>> {
        None
    }

    fn intercept(
        &self,
        ctx: &mut RoutingContext<NetworkMessageMut<'_>>,
        _cache: Option<&Box<dyn Any + Send + Sync>>,
    ) -> bool {
        if let NetworkBodyMut::Push(&mut Push {
            payload: PushBody::Put(ref mut p),
            ..
        }) = &mut ctx.msg.body
        {
            let s = str::from_utf8(p.payload.to_zslice().as_slice())
                .unwrap()
                .to_string();
            let s = s + "->" + &self.zid;
            p.payload = ZBuf::from(s.as_bytes().to_owned());
        }
        true
    }
}

use std::time::Duration;

use zenoh_config::ZenohId;
use zenoh_core::ztimeout;

use crate::{config::WhatAmI, init_log_from_env_or, open, Config};

const TIMEOUT: Duration = Duration::from_secs(60);
const SLEEP: Duration = Duration::from_secs(1);

async fn get_basic_router_config(listen_ports: &[u16], connect_ports: &[u16]) -> Config {
    let mut config = Config::default();
    config.set_mode(Some(WhatAmI::Router)).unwrap();
    config
        .listen
        .endpoints
        .set(
            listen_ports
                .iter()
                .map(|p| format!("tcp/127.0.0.1:{p}").parse().unwrap())
                .collect::<Vec<_>>(),
        )
        .unwrap();
    config
        .connect
        .endpoints
        .set(
            connect_ports
                .iter()
                .map(|p| format!("tcp/127.0.0.1:{p}").parse().unwrap())
                .collect::<Vec<_>>(),
        )
        .unwrap();
    config.scouting.multicast.set_enabled(Some(false)).unwrap();
    config
}

async fn get_basic_client_config(port: u16) -> Config {
    let mut config = Config::default();
    config.set_mode(Some(WhatAmI::Client)).unwrap();
    config
        .connect
        .endpoints
        .set(vec![format!("tcp/127.0.0.1:{port}").parse().unwrap()])
        .unwrap();
    config.scouting.multicast.set_enabled(Some(false)).unwrap();
    config
}

async fn test_link_weights_inner(
    net: Vec<(u16, Vec<(u16, Option<u16>)>)>,
    source: u16,
    dest: u16,
    port_offset: u16,
) -> String {
    let start_id = ZenohId::from_str("a").unwrap();
    let end_id = ZenohId::from_str("b").unwrap();

    let c = LinkTraceInterceptorConf {
        flow: InterceptorFlow::Egress,
    };
    let f = move || link_trace_interceptor_factories(&vec![c.clone()]);
    let mut lk = crate::net::routing::interceptor::tests::ID_TO_INTERCEPTOR_FACTORIES
        .lock()
        .unwrap();

    lk.clear();
    lk.insert(start_id, Box::new(f.clone()));

    let mut routers = Vec::new();
    for v in &net {
        let zid = ZenohId::from_str(&v.0.to_string()).unwrap();
        lk.insert(zid, Box::new(f.clone()));
    }

    drop(lk);

    for v in &net {
        let zid = ZenohId::from_str(&v.0.to_string()).unwrap();
        let connect =
            v.1.iter()
                .map(|(id, _)| id + port_offset)
                .collect::<Vec<_>>();
        let mut config = get_basic_router_config(&[port_offset + v.0], &connect).await;
        config.set_id(zid).unwrap();
        let weights =
            v.1.iter()
                .filter_map(|(e, w)| {
                    w.map(|w| LinkWeight {
                        destination: e.to_string(),
                        weight: w.try_into().unwrap(),
                    })
                })
                .collect::<Vec<_>>();
        if !weights.is_empty() {
            config
                .routing
                .router
                .set_link_weights(Some(weights.try_into().unwrap()))
                .unwrap();
        }

        let router = ztimeout!(open(config)).unwrap();
        routers.push(router);
    }
    tokio::time::sleep(3 * SLEEP).await;

    let mut config_client_a = get_basic_client_config(source + port_offset).await;
    config_client_a.set_id(start_id).unwrap();
    let mut config_client_b = get_basic_client_config(dest + port_offset).await;
    config_client_b.set_id(end_id).unwrap();

    let session_a = ztimeout!(open(config_client_a)).unwrap();
    let session_b = ztimeout!(open(config_client_b)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let sub = ztimeout!(session_b.declare_subscriber("test/link_weights")).unwrap();

    tokio::time::sleep(SLEEP).await;
    ztimeout!(session_a.put("test/link_weights", "a")).unwrap();
    tokio::time::sleep(SLEEP).await;

    let msg = ztimeout!(sub.recv_async()).unwrap();

    msg.payload().try_to_string().unwrap().to_string()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_link_weights_diamond() {
    init_log_from_env_or("error");
    //       2
    //      / \
    // a - 1   4 - b
    //      \ /
    //       3

    let res = test_link_weights_inner(
        vec![
            (1, vec![(2, Some(10)), (3, None)]),
            (2, vec![(4, None)]),
            (3, vec![(4, None)]),
            (4, vec![]),
        ],
        1,
        4,
        28000,
    )
    .await;

    assert_eq!(res, "a->1->2->4->b");

    let res = test_link_weights_inner(
        vec![
            (1, vec![(3, None)]),
            (2, vec![(1, Some(10)), (4, None)]),
            (3, vec![(4, None)]),
            (4, vec![]),
        ],
        1,
        4,
        28100,
    )
    .await;

    assert_eq!(res, "a->1->2->4->b");

    let res = test_link_weights_inner(
        vec![
            (1, vec![(2, None), (3, None)]),
            (2, vec![(4, None)]),
            (3, vec![]),
            (4, vec![(3, Some(10))]),
        ],
        1,
        4,
        28200,
    )
    .await;

    assert_eq!(res, "a->1->3->4->b");

    let res = test_link_weights_inner(
        vec![
            (1, vec![(2, None), (3, None)]),
            (2, vec![(4, None)]),
            (3, vec![(4, Some(10))]),
            (4, vec![]),
        ],
        1,
        4,
        28300,
    )
    .await;

    assert_eq!(res, "a->1->3->4->b");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_link_weights_triangle() {
    init_log_from_env_or("error");
    //       2
    //      / \
    // a - 1 - 3 - b

    let res = test_link_weights_inner(
        vec![
            (1, vec![(2, Some(10)), (3, None)]),
            (2, vec![(3, Some(10))]),
            (3, vec![]),
        ],
        1,
        3,
        29000,
    )
    .await;

    assert_eq!(res, "a->1->2->3->b");

    let res = test_link_weights_inner(
        vec![
            (1, vec![(2, None), (3, Some(300))]),
            (2, vec![(3, None)]),
            (3, vec![]),
        ],
        1,
        3,
        29100,
    )
    .await;

    assert_eq!(res, "a->1->2->3->b");

    let res = test_link_weights_inner(
        vec![
            (1, vec![(2, None), (3, None)]),
            (2, vec![(3, None)]),
            (3, vec![]),
        ],
        1,
        3,
        29200,
    )
    .await;

    assert_eq!(res, "a->1->3->b");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_link_weights_hexagon() {
    init_log_from_env_or("error");
    //       2 - 5
    //      / \ / \
    // a - 1 - 4 - 7 - b
    //      \ / \ /
    //       3 - 6

    let res = test_link_weights_inner(
        vec![
            (1, vec![(2, Some(1)), (3, None), (4, None)]),
            (2, vec![(4, Some(1)), (5, None)]),
            (3, vec![(4, None), (6, None)]),
            (4, vec![(5, None), (6, Some(1)), (7, None)]),
            (5, vec![(7, None)]),
            (6, vec![(7, Some(1))]),
            (7, vec![]),
        ],
        1,
        7,
        30000,
    )
    .await;

    assert_eq!(res, "a->1->2->4->6->7->b");

    let res = test_link_weights_inner(
        vec![
            (1, vec![(2, None), (3, None), (4, None)]),
            (2, vec![(4, None), (5, None)]),
            (3, vec![(4, None), (6, None)]),
            (4, vec![(5, None), (6, None), (7, None)]),
            (5, vec![(7, None)]),
            (6, vec![(7, None)]),
            (7, vec![]),
        ],
        1,
        7,
        30100,
    )
    .await;

    assert_eq!(res, "a->1->4->7->b");
}
