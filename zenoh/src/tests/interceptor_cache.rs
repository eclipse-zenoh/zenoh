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

use std::str::FromStr;

use zenoh_buffers::ZBuf;
use zenoh_keyexpr::keyexpr;
use zenoh_protocol::{
    network::{NetworkBodyMut, NetworkMessageMut, Push},
    zenoh::PushBody,
};
use zenoh_transport::{multicast::TransportMulticast, unicast::TransportUnicast};

use crate::net::routing::interceptor::*;

#[derive(Clone)]
struct TestInterceptorConf {
    flow: InterceptorFlow,
    data: String,
}

fn test_interceptor_factories(config: &Vec<TestInterceptorConf>) -> Vec<InterceptorFactory> {
    let mut res: Vec<InterceptorFactory> = vec![];
    for c in config {
        res.push(Box::new(TestInterceptorFactory::new(c.clone())));
    }
    res
}

struct TestInterceptorFactory {
    conf: TestInterceptorConf,
}

impl TestInterceptorFactory {
    pub fn new(conf: TestInterceptorConf) -> Self {
        Self { conf }
    }
}

impl InterceptorFactoryTrait for TestInterceptorFactory {
    fn new_transport_unicast(
        &self,
        _transport: &TransportUnicast,
    ) -> (Option<IngressInterceptor>, Option<EgressInterceptor>) {
        match self.conf.flow {
            InterceptorFlow::Egress => {
                (None, Some(Box::new(TestInterceptor::new(&self.conf.data))))
            }
            InterceptorFlow::Ingress => {
                (Some(Box::new(TestInterceptor::new(&self.conf.data))), None)
            }
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

struct TestInterceptor {
    data: String,
}

impl TestInterceptor {
    fn new(data: &str) -> Self {
        TestInterceptor {
            data: data.to_owned(),
        }
    }
}

impl InterceptorTrait for TestInterceptor {
    fn compute_keyexpr_cache(&self, _key_expr: &keyexpr) -> Option<Box<dyn Any + Send + Sync>> {
        Some(Box::new(self.data.clone()))
    }

    fn intercept(&self, msg: &mut NetworkMessageMut, ctx: &mut dyn InterceptorContext) -> bool {
        let cache_hit = ctx.get_cache(msg).is_some();
        if let NetworkBodyMut::Push(&mut Push {
            payload: PushBody::Put(ref mut p),
            ..
        }) = &mut msg.body
        {
            let out = format!("Cache hit: {cache_hit}, data: {}", &self.data);
            p.payload = ZBuf::from(out.as_bytes().to_owned());
        }
        true
    }
}

use std::{any::Any, time::Duration};

use zenoh_config::{InterceptorFlow, ZenohId};
use zenoh_core::ztimeout;

use crate::{config::WhatAmI, init_log_from_env_or, open, Config};

const TIMEOUT: Duration = Duration::from_secs(60);
const SLEEP: Duration = Duration::from_secs(1);

async fn get_basic_router_config(port: u16) -> Config {
    let mut config = Config::default();
    config.set_mode(Some(WhatAmI::Router)).unwrap();
    config
        .listen
        .endpoints
        .set(vec![format!("tcp/127.0.0.1:{port}").parse().unwrap()])
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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_interceptors_cache_update_ingress() {
    let router_id = ZenohId::from_str("a1").unwrap();
    let c = TestInterceptorConf {
        flow: InterceptorFlow::Ingress,
        data: "1".to_string(),
    };
    let f = move || test_interceptor_factories(&vec![c.clone()]);
    let _ = crate::net::routing::interceptor::tests::ID_TO_INTERCEPTOR_FACTORIES
        .lock()
        .unwrap()
        .insert(router_id, Box::new(f));

    init_log_from_env_or("error");
    let mut config_router = get_basic_router_config(27701).await;
    config_router.set_id(Some(router_id)).unwrap();

    let config_client1 = get_basic_client_config(27701).await;
    let config_client2 = get_basic_client_config(27701).await;

    let router = ztimeout!(open(config_router.clone())).unwrap();
    tokio::time::sleep(SLEEP).await;
    let session1 = ztimeout!(open(config_client1)).unwrap();
    let session2 = ztimeout!(open(config_client2)).unwrap();
    tokio::time::sleep(SLEEP).await;
    let sub = ztimeout!(session2.declare_subscriber("test/cache")).unwrap();
    let pub1 = ztimeout!(session1.declare_publisher("test/cache")).unwrap();
    let pub2 = ztimeout!(session1.declare_publisher("test/*")).unwrap();
    tokio::time::sleep(SLEEP).await;
    ztimeout!(pub1.put("some_data_1")).unwrap();
    tokio::time::sleep(SLEEP).await;
    ztimeout!(pub2.put("some_data_2")).unwrap();

    let msg1 = ztimeout!(sub.recv_async()).unwrap();
    let msg2 = ztimeout!(sub.recv_async()).unwrap();

    assert_eq!(
        msg1.payload().try_to_string().unwrap().as_ref(),
        "Cache hit: true, data: 1"
    );
    assert_eq!(
        msg2.payload().try_to_string().unwrap().as_ref(),
        "Cache hit: false, data: 1"
    );

    let c = TestInterceptorConf {
        flow: InterceptorFlow::Ingress,
        data: "2".to_string(),
    };
    let f = move || test_interceptor_factories(&vec![c.clone()]);
    let _ = crate::net::routing::interceptor::tests::ID_TO_INTERCEPTOR_FACTORIES
        .lock()
        .unwrap()
        .insert(router_id, Box::new(f));

    router
        .0
        .static_runtime()
        .unwrap()
        .router()
        .tables
        .regen_interceptors(&config_router)
        .unwrap();
    tokio::time::sleep(SLEEP).await;

    ztimeout!(pub1.put("some_data_1")).unwrap();
    tokio::time::sleep(SLEEP).await;
    ztimeout!(pub2.put("some_data_2")).unwrap();
    let sub2 = ztimeout!(session2.declare_subscriber("test2/cache")).unwrap();
    let pub3 = ztimeout!(session1.declare_publisher("test2/cache")).unwrap();
    tokio::time::sleep(SLEEP).await;
    ztimeout!(pub3.put("some_data_3")).unwrap();

    let msg1 = ztimeout!(sub.recv_async()).unwrap();
    let msg2 = ztimeout!(sub.recv_async()).unwrap();
    let msg3 = ztimeout!(sub2.recv_async()).unwrap();

    assert_eq!(
        msg1.payload().try_to_string().unwrap().as_ref(),
        "Cache hit: true, data: 2"
    );
    assert_eq!(
        msg2.payload().try_to_string().unwrap().as_ref(),
        "Cache hit: false, data: 2"
    );
    assert_eq!(
        msg3.payload().try_to_string().unwrap().as_ref(),
        "Cache hit: true, data: 2"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_interceptors_cache_update_egress() {
    let router_id = ZenohId::from_str("a1").unwrap();
    let c = TestInterceptorConf {
        flow: InterceptorFlow::Egress,
        data: "1".to_string(),
    };
    let f = move || test_interceptor_factories(&vec![c.clone()]);
    let _ = crate::net::routing::interceptor::tests::ID_TO_INTERCEPTOR_FACTORIES
        .lock()
        .unwrap()
        .insert(router_id, Box::new(f));

    init_log_from_env_or("error");
    let mut config_router = get_basic_router_config(27702).await;
    config_router.set_id(Some(router_id)).unwrap();

    let config_client1 = get_basic_client_config(27702).await;
    let config_client2 = get_basic_client_config(27702).await;

    let router = ztimeout!(open(config_router.clone())).unwrap();
    tokio::time::sleep(SLEEP).await;
    let session1 = ztimeout!(open(config_client1)).unwrap();
    let session2 = ztimeout!(open(config_client2)).unwrap();
    tokio::time::sleep(SLEEP).await;
    let sub = ztimeout!(session2.declare_subscriber("test/cache")).unwrap();
    let pub1 = ztimeout!(session1.declare_publisher("test/cache")).unwrap();
    let pub2 = ztimeout!(session1.declare_publisher("test/*")).unwrap();
    tokio::time::sleep(SLEEP).await;
    ztimeout!(pub1.put("some_data_1")).unwrap();
    tokio::time::sleep(SLEEP).await;
    ztimeout!(pub2.put("some_data_2")).unwrap();

    let msg1 = ztimeout!(sub.recv_async()).unwrap();
    let msg2 = ztimeout!(sub.recv_async()).unwrap();

    assert_eq!(
        msg1.payload().try_to_string().unwrap().as_ref(),
        "Cache hit: true, data: 1"
    );
    assert_eq!(
        msg2.payload().try_to_string().unwrap().as_ref(),
        "Cache hit: false, data: 1"
    );

    let c = TestInterceptorConf {
        flow: InterceptorFlow::Egress,
        data: "2".to_string(),
    };
    let f = move || test_interceptor_factories(&vec![c.clone()]);
    let _ = crate::net::routing::interceptor::tests::ID_TO_INTERCEPTOR_FACTORIES
        .lock()
        .unwrap()
        .insert(router_id, Box::new(f));

    router
        .0
        .static_runtime()
        .unwrap()
        .router()
        .tables
        .regen_interceptors(&config_router)
        .unwrap();
    tokio::time::sleep(SLEEP).await;

    ztimeout!(pub1.put("some_data_1")).unwrap();
    tokio::time::sleep(SLEEP).await;
    ztimeout!(pub2.put("some_data_2")).unwrap();
    let sub2 = ztimeout!(session2.declare_subscriber("test2/cache")).unwrap();
    let pub3 = ztimeout!(session1.declare_publisher("test2/cache")).unwrap();
    tokio::time::sleep(SLEEP).await;
    ztimeout!(pub3.put("some_data_3")).unwrap();

    let msg1 = ztimeout!(sub.recv_async()).unwrap();
    let msg2 = ztimeout!(sub.recv_async()).unwrap();
    let msg3 = ztimeout!(sub2.recv_async()).unwrap();

    assert_eq!(
        msg1.payload().try_to_string().unwrap().as_ref(),
        "Cache hit: true, data: 2"
    );
    assert_eq!(
        msg2.payload().try_to_string().unwrap().as_ref(),
        "Cache hit: false, data: 2"
    );
    assert_eq!(
        msg3.payload().try_to_string().unwrap().as_ref(),
        "Cache hit: true, data: 2"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_interceptors_cache_update_egress_then_ingress() {
    let router_id = ZenohId::from_str("a1").unwrap();
    let c = TestInterceptorConf {
        flow: InterceptorFlow::Egress,
        data: "1".to_string(),
    };
    let f = move || test_interceptor_factories(&vec![c.clone()]);
    let _ = crate::net::routing::interceptor::tests::ID_TO_INTERCEPTOR_FACTORIES
        .lock()
        .unwrap()
        .insert(router_id, Box::new(f));

    init_log_from_env_or("error");
    let mut config_router = get_basic_router_config(27703).await;
    config_router.set_id(Some(router_id)).unwrap();

    let config_client1 = get_basic_client_config(27703).await;
    let config_client2 = get_basic_client_config(27703).await;

    let router = ztimeout!(open(config_router.clone())).unwrap();
    tokio::time::sleep(SLEEP).await;
    let session1 = ztimeout!(open(config_client1)).unwrap();
    let session2 = ztimeout!(open(config_client2)).unwrap();
    tokio::time::sleep(SLEEP).await;
    let sub = ztimeout!(session2.declare_subscriber("test/cache")).unwrap();
    let pub1 = ztimeout!(session1.declare_publisher("test/cache")).unwrap();
    let pub2 = ztimeout!(session1.declare_publisher("test/*")).unwrap();
    tokio::time::sleep(SLEEP).await;
    ztimeout!(pub1.put("some_data_1")).unwrap();
    tokio::time::sleep(SLEEP).await;
    ztimeout!(pub2.put("some_data_2")).unwrap();

    let msg1 = ztimeout!(sub.recv_async()).unwrap();
    let msg2 = ztimeout!(sub.recv_async()).unwrap();

    assert_eq!(
        msg1.payload().try_to_string().unwrap().as_ref(),
        "Cache hit: true, data: 1"
    );
    assert_eq!(
        msg2.payload().try_to_string().unwrap().as_ref(),
        "Cache hit: false, data: 1"
    );

    let c = TestInterceptorConf {
        flow: InterceptorFlow::Ingress,
        data: "2".to_string(),
    };
    let f = move || test_interceptor_factories(&vec![c.clone()]);
    let _ = crate::net::routing::interceptor::tests::ID_TO_INTERCEPTOR_FACTORIES
        .lock()
        .unwrap()
        .insert(router_id, Box::new(f));

    router
        .0
        .static_runtime()
        .unwrap()
        .router()
        .tables
        .regen_interceptors(&config_router)
        .unwrap();
    tokio::time::sleep(SLEEP).await;

    ztimeout!(pub1.put("some_data_1")).unwrap();
    tokio::time::sleep(SLEEP).await;
    ztimeout!(pub2.put("some_data_2")).unwrap();
    let sub2 = ztimeout!(session2.declare_subscriber("test2/cache")).unwrap();
    let pub3 = ztimeout!(session1.declare_publisher("test2/cache")).unwrap();
    tokio::time::sleep(SLEEP).await;
    ztimeout!(pub3.put("some_data_3")).unwrap();

    let msg1 = ztimeout!(sub.recv_async()).unwrap();
    let msg2 = ztimeout!(sub.recv_async()).unwrap();
    let msg3 = ztimeout!(sub2.recv_async()).unwrap();

    assert_eq!(
        msg1.payload().try_to_string().unwrap().as_ref(),
        "Cache hit: true, data: 2"
    );
    assert_eq!(
        msg2.payload().try_to_string().unwrap().as_ref(),
        "Cache hit: false, data: 2"
    );
    assert_eq!(
        msg3.payload().try_to_string().unwrap().as_ref(),
        "Cache hit: true, data: 2"
    );
}
