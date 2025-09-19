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
#![cfg(feature = "internal")]

use std::{
    any::Any,
    collections::HashMap,
    str::{self, FromStr},
};

use zenoh_buffers::ZBuf;
use zenoh_config::{InterceptorFlow, TransportWeight};
use zenoh_keyexpr::keyexpr;
use zenoh_protocol::{
    core::ZenohIdProto,
    network::{NetworkBodyMut, NetworkMessageMut, Push},
    zenoh::PushBody,
};
use zenoh_transport::{multicast::TransportMulticast, unicast::TransportUnicast};

use crate::{
    net::{protocol::linkstate::LinkInfo, routing::interceptor::*},
    Session,
};

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

    fn intercept(&self, msg: &mut NetworkMessageMut, _ctx: &mut dyn InterceptorContext) -> bool {
        if let NetworkBodyMut::Push(&mut Push {
            payload: PushBody::Put(ref mut p),
            ..
        }) = &mut msg.body
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

async fn get_basic_router_config(
    listen_ports: &[u16],
    connect_ports: &[u16],
    wai: WhatAmI,
) -> Config {
    let mut config = Config::default();
    config.set_mode(Some(wai)).unwrap();
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
    config.scouting.gossip.set_enabled(Some(false)).unwrap();
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

struct Net {
    routers: Vec<Session>,
    source: Session,
    dest: Session,
    wai: WhatAmI,
}

impl Net {
    #[allow(clippy::type_complexity)]
    async fn update_weights(&mut self, net: Vec<(u16, Vec<(u16, Option<u16>)>)>) {
        for (i, v) in net.iter().enumerate() {
            let weights =
                v.1.iter()
                    .filter_map(|(e, w)| {
                        w.map(|w| TransportWeight {
                            dst_zid: ZenohId::from_str(&e.to_string()).unwrap(),
                            weight: w.try_into().unwrap(),
                        })
                    })
                    .collect::<Vec<_>>();

            match self.wai {
                WhatAmI::Router => self.routers[i]
                    .0
                    .static_runtime()
                    .unwrap()
                    .config()
                    .lock()
                    .routing
                    .router
                    .linkstate
                    .set_transport_weights(weights)
                    .unwrap(),
                WhatAmI::Peer => self.routers[i]
                    .0
                    .static_runtime()
                    .unwrap()
                    .config()
                    .lock()
                    .routing
                    .peer
                    .linkstate
                    .set_transport_weights(weights)
                    .unwrap(),
                WhatAmI::Client => unreachable!(),
            };
            self.routers[i]
                .0
                .static_runtime()
                .unwrap()
                .update_network()
                .unwrap();
        }
    }
}

#[allow(clippy::type_complexity)]
async fn create_routers_net(
    net: Vec<(u16, Vec<(u16, Option<u16>)>)>,
    source: u16,
    dest: u16,
    port_offset: u16,
) -> Net {
    create_net(net, source, dest, port_offset, WhatAmI::Router).await
}

#[allow(clippy::type_complexity)]
async fn create_peers_net(
    net: Vec<(u16, Vec<(u16, Option<u16>)>)>,
    source: u16,
    dest: u16,
    port_offset: u16,
) -> Net {
    create_net(net, source, dest, port_offset, WhatAmI::Peer).await
}

#[allow(clippy::type_complexity)]
async fn create_net(
    net: Vec<(u16, Vec<(u16, Option<u16>)>)>,
    source: u16,
    dest: u16,
    port_offset: u16,
    wai: WhatAmI,
) -> Net {
    let start_id = ZenohId::from_str("a").unwrap();
    let end_id = ZenohId::from_str("b").unwrap();

    {
        let c = LinkTraceInterceptorConf {
            flow: InterceptorFlow::Egress,
        };
        let f = move || link_trace_interceptor_factories(&vec![c.clone()]);
        let mut lk = crate::net::routing::interceptor::tests::ID_TO_INTERCEPTOR_FACTORIES
            .lock()
            .unwrap();

        lk.insert(start_id, Box::new(f.clone()));

        for v in &net {
            let zid = ZenohId::from_str(&v.0.to_string()).unwrap();
            lk.insert(zid, Box::new(f.clone()));
        }
    }
    let mut routers = Vec::new();

    for v in &net {
        let zid = ZenohId::from_str(&v.0.to_string()).unwrap();
        let connect =
            v.1.iter()
                .map(|(id, _)| id + port_offset)
                .collect::<Vec<_>>();
        let mut config = get_basic_router_config(&[port_offset + v.0], &connect, wai).await;
        config.set_id(Some(zid)).unwrap();
        let weights =
            v.1.iter()
                .filter_map(|(e, w)| {
                    w.map(|w| TransportWeight {
                        dst_zid: ZenohId::from_str(&e.to_string()).unwrap(),
                        weight: w.try_into().unwrap(),
                    })
                })
                .collect::<Vec<_>>();
        match wai {
            WhatAmI::Router => {
                config
                    .routing
                    .router
                    .linkstate
                    .set_transport_weights(weights)
                    .unwrap();
            }
            WhatAmI::Peer => {
                config
                    .routing
                    .peer
                    .linkstate
                    .set_transport_weights(weights)
                    .unwrap();
                config
                    .routing
                    .peer
                    .set_mode(Some("linkstate".to_string()))
                    .unwrap();
            }
            WhatAmI::Client => unreachable!(),
        }

        let router = ztimeout!(open(config)).unwrap();
        routers.push(router);
    }

    let mut config_client_a = get_basic_client_config(source + port_offset).await;
    config_client_a.set_id(Some(start_id)).unwrap();
    let mut config_client_b = get_basic_client_config(dest + port_offset).await;
    config_client_b.set_id(Some(end_id)).unwrap();

    let session_a = ztimeout!(open(config_client_a)).unwrap();
    let session_b = ztimeout!(open(config_client_b)).unwrap();
    tokio::time::sleep(SLEEP).await;

    Net {
        routers,
        source: session_a,
        dest: session_b,
        wai,
    }
}

#[allow(clippy::type_complexity)]
async fn test_link_weights_inner(
    net: Vec<(u16, Vec<(u16, Option<u16>)>)>,
    source: u16,
    dest: u16,
    port_offset: u16,
) -> String {
    let net = create_routers_net(net, source, dest, port_offset).await;

    let sub = ztimeout!(net.dest.declare_subscriber("test/link_weights")).unwrap();

    tokio::time::sleep(3 * SLEEP).await;
    ztimeout!(net.source.put("test/link_weights", "a")).unwrap();

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

    // should resolve weights to 55 on 1-2 and 2-3, so will pick 1-3 path (with default cost of 100)
    let res = test_link_weights_inner(
        vec![
            (1, vec![(2, Some(10)), (3, None)]),
            (2, vec![(1, Some(55)), (3, Some(10))]),
            (3, vec![(2, Some(55))]),
        ],
        1,
        3,
        29300,
    )
    .await;

    assert_eq!(res, "a->1->3->b");

    // should resolve weights to 45 on 1-2 and 2-3, so will pick 1-2-3 path with total cost of 2 * 45 = 90
    let res = test_link_weights_inner(
        vec![
            (1, vec![(2, Some(45)), (3, None)]),
            (2, vec![(1, Some(10)), (3, Some(45))]),
            (3, vec![(2, Some(10))]),
        ],
        1,
        3,
        29400,
    )
    .await;

    assert_eq!(res, "a->1->2->3->b");
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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_link_weights_update_diamond() {
    init_log_from_env_or("error");
    //       2
    //      / \
    // a - 1   4 - b
    //      \ /
    //       3

    let mut net = create_routers_net(
        vec![
            (1, vec![(2, Some(10)), (3, None)]),
            (2, vec![(4, None)]),
            (3, vec![(4, None)]),
            (4, vec![]),
        ],
        1,
        4,
        31000,
    )
    .await;

    let sub = ztimeout!(net.dest.declare_subscriber("test/link_weights")).unwrap();

    tokio::time::sleep(3 * SLEEP).await;
    ztimeout!(net.source.put("test/link_weights", "a")).unwrap();

    let msg = ztimeout!(sub.recv_async())
        .unwrap()
        .payload()
        .try_to_string()
        .unwrap()
        .to_string();
    assert_eq!(msg, "a->1->2->4->b");

    ztimeout!(net.update_weights(vec![
        (1, vec![(2, None), (3, Some(10))]),
        (2, vec![(4, None)]),
        (3, vec![(4, None)]),
        (4, vec![]),
    ]));

    tokio::time::sleep(3 * SLEEP).await;
    ztimeout!(net.source.put("test/link_weights", "a")).unwrap();

    let msg = ztimeout!(sub.recv_async())
        .unwrap()
        .payload()
        .try_to_string()
        .unwrap()
        .to_string();
    assert_eq!(msg, "a->1->3->4->b");

    ztimeout!(net.update_weights(vec![
        (1, vec![(2, None), (3, Some(200))]),
        (2, vec![(4, None)]),
        (3, vec![(4, None)]),
        (4, vec![]),
    ]));

    tokio::time::sleep(3 * SLEEP).await;
    ztimeout!(net.source.put("test/link_weights", "a")).unwrap();

    let msg = ztimeout!(sub.recv_async())
        .unwrap()
        .payload()
        .try_to_string()
        .unwrap()
        .to_string();
    assert_eq!(msg, "a->1->2->4->b");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_link_weights_update_hexagon() {
    init_log_from_env_or("error");
    //       2 - 5
    //      / \ / \
    // a - 1 - 4 - 7 - b
    //      \ / \ /
    //       3 - 6

    let mut net = create_routers_net(
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
        32000,
    )
    .await;

    let sub = ztimeout!(net.dest.declare_subscriber("test/link_weights")).unwrap();

    tokio::time::sleep(3 * SLEEP).await;
    ztimeout!(net.source.put("test/link_weights", "a")).unwrap();

    let msg = ztimeout!(sub.recv_async())
        .unwrap()
        .payload()
        .try_to_string()
        .unwrap()
        .to_string();

    assert_eq!(msg, "a->1->2->4->6->7->b");

    ztimeout!(net.update_weights(vec![
        (1, vec![(2, None), (3, None), (4, None)]),
        (2, vec![(4, None), (5, None)]),
        (3, vec![(4, None), (6, None)]),
        (4, vec![(5, None), (6, None), (7, None)]),
        (5, vec![(7, None)]),
        (6, vec![(7, None)]),
        (7, vec![]),
    ],));

    tokio::time::sleep(3 * SLEEP).await;
    ztimeout!(net.source.put("test/link_weights", "a")).unwrap();

    let msg = ztimeout!(sub.recv_async())
        .unwrap()
        .payload()
        .try_to_string()
        .unwrap()
        .to_string();

    assert_eq!(msg, "a->1->4->7->b");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_link_weights_update_diamond_peers() {
    init_log_from_env_or("error");
    //       2
    //      / \
    // a - 1   4 - b
    //      \ /
    //       3

    let mut net = create_peers_net(
        vec![
            (1, vec![(2, Some(10)), (3, None)]),
            (2, vec![(4, None)]),
            (3, vec![(4, None)]),
            (4, vec![]),
        ],
        1,
        4,
        33000,
    )
    .await;

    let sub = ztimeout!(net.dest.declare_subscriber("test/link_weights")).unwrap();

    tokio::time::sleep(3 * SLEEP).await;
    ztimeout!(net.source.put("test/link_weights", "a")).unwrap();

    let msg = ztimeout!(sub.recv_async())
        .unwrap()
        .payload()
        .try_to_string()
        .unwrap()
        .to_string();
    assert_eq!(msg, "a->1->2->4->b");

    ztimeout!(net.update_weights(vec![
        (1, vec![(2, None), (3, Some(10))]),
        (2, vec![(4, None)]),
        (3, vec![(4, None)]),
        (4, vec![]),
    ]));

    tokio::time::sleep(3 * SLEEP).await;
    ztimeout!(net.source.put("test/link_weights", "a")).unwrap();

    let msg = ztimeout!(sub.recv_async())
        .unwrap()
        .payload()
        .try_to_string()
        .unwrap()
        .to_string();
    assert_eq!(msg, "a->1->3->4->b");

    ztimeout!(net.update_weights(vec![
        (1, vec![(2, None), (3, Some(200))]),
        (2, vec![(4, None)]),
        (3, vec![(4, None)]),
        (4, vec![]),
    ]));

    tokio::time::sleep(3 * SLEEP).await;
    ztimeout!(net.source.put("test/link_weights", "a")).unwrap();

    let msg = ztimeout!(sub.recv_async())
        .unwrap()
        .payload()
        .try_to_string()
        .unwrap()
        .to_string();
    assert_eq!(msg, "a->1->2->4->b");
}

async fn test_link_weights_info_diamond_inner(port_offset: u16, wai: WhatAmI) {
    //       2
    //      / \
    // a - 1 - 4 - b
    //      \ /
    //       3

    let net = create_net(
        vec![
            (1, vec![(2, Some(10)), (3, None), (4, Some(20))]),
            (2, vec![(4, None)]),
            (3, vec![(4, None)]),
            (4, vec![(1, Some(30))]),
        ],
        1,
        4,
        port_offset,
        wai,
    )
    .await;

    let info = net.routers[0].0.static_runtime().unwrap().get_links_info();

    let expected = HashMap::from([
        (
            ZenohIdProto::from_str("2").unwrap(),
            LinkInfo {
                src_weight: Some(10),
                dst_weight: None,
                actual_weight: 10,
            },
        ),
        (
            ZenohIdProto::from_str("3").unwrap(),
            LinkInfo {
                src_weight: None,
                dst_weight: None,
                actual_weight: 100,
            },
        ),
        (
            ZenohIdProto::from_str("4").unwrap(),
            LinkInfo {
                src_weight: Some(20),
                dst_weight: Some(30),
                actual_weight: 30,
            },
        ),
    ]);

    assert_eq!(info, expected);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_link_weights_info_diamond_routers() {
    init_log_from_env_or("error");
    test_link_weights_info_diamond_inner(36000, WhatAmI::Router).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_link_weights_info_diamond_peers() {
    init_log_from_env_or("error");
    test_link_weights_info_diamond_inner(35000, WhatAmI::Peer).await;
}
