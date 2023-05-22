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
use futures::future::select_all;
use futures::FutureExt as _;
use std::time::Duration;

use async_std::prelude::FutureExt;
use zenoh::config::{whatami::WhatAmI, Config};
use zenoh::prelude::r#async::*;
use zenoh::Result;
use zenoh_core::zasync_executor_init;
use zenoh_result::bail;

const TIMEOUT: Duration = Duration::from_secs(10);
const MSG_COUNT: usize = 1_000;
const MSG_SIZE: [usize; 2] = [1_024, 131_072];

macro_rules! ztimeout {
    ($f:expr) => {
        $f.timeout(TIMEOUT).await?
    };
}

#[derive(Debug)]
enum Task {
    Pub(String, usize),
    Sub(String, usize),
    Sleep(Duration),
}

#[derive(Debug)]
struct Node {
    name: String,
    mode: WhatAmI,
    listen: Vec<String>,
    connect: Vec<String>,
    task: Vec<Task>,
    config: Option<Config>,
}

impl Default for Node {
    fn default() -> Self {
        Self {
            name: "TestNode".into(),
            mode: WhatAmI::Peer,
            listen: vec![],
            connect: vec![],
            task: vec![],
            config: None,
        }
    }
}

async fn run_recipe(receipe: impl IntoIterator<Item = Node>) -> Result<()> {
    let futures = receipe.into_iter().map(|node|
        async move {
        dbg!(node.name);

        // Load the config and build up a session
        let mut config = node.config.unwrap_or_else(Config::default);
        config.set_mode(Some(node.mode)).unwrap();
        config.scouting.multicast.set_enabled(Some(false)).unwrap();
        config
            .listen
            .set_endpoints(node.listen.iter().map(|x| x.parse().unwrap()).collect())
            .unwrap();
        config
            .connect
            .set_endpoints(node.connect.iter().map(|x| x.parse().unwrap()).collect())
            .unwrap();

        // In case of client can't connect to some peers/routers
        let session = if let Ok(session) = zenoh::open(config.clone()).res_async().await {
            session.into_arc()
        } else {
            log::info!("Sleep and retry to connect to the endpoint...");
            async_std::task::sleep(Duration::from_secs(3)).await;
            zenoh::open(config).res_async().await?.into_arc()
        };

        // Each node consists of a specified session associated with tasks to run
        let futs = node.task.into_iter().map(|task| {

            // Multiple tasks share the session
            let session = session.clone();

            match task {

                // Subscription task
                Task::Sub(topic, expected_size) => {
                    async_std::task::spawn(async move {
                        let sub =
                            ztimeout!(session.declare_subscriber(&topic).res_async())?;
                        let mut counter = 0;
                        while let Ok(sample) = sub.recv_async().await {
                            let recv_size = sample.value.payload.len();
                            if recv_size != expected_size {
                                bail!("Received payload size {recv_size} mismatches the expected {expected_size}");
                            }
                            counter += 1;
                            if counter >= MSG_COUNT {
                                log::info!("Received sufficient amount of messages.");
                                break;
                            }
                        }
                        Result::Ok(())
                    })
                }

                // Publishment task
                Task::Pub(topic, payload_size) => {
                    async_std::task::spawn(async move {
                        loop {
                            // async_std::task::sleep(Duration::from_millis(300)).await;
                            ztimeout!(session
                                .put(&topic, vec![0u8; payload_size])
                                .congestion_control(CongestionControl::Block)
                                .res_async())?;
                        }
                    })
                }

                // Sleep a while according to the given duration
                Task::Sleep(dur) => async_std::task::spawn(async move {
                    async_std::task::sleep(dur).await;
                    Ok(())
                }),
            }
        });

        let (state, _, _) = select_all(futs).await;
        state
    }.boxed());

    let (state, _, _) = select_all(futures).timeout(TIMEOUT).await?;
    state
}

#[test]
fn gossip() -> Result<()> {
    async_std::task::block_on(async {
        zasync_executor_init!();
        let locator = String::from("tcp/127.0.0.1:17448");
        let topic = String::from("testTopic");

        // Gossip test
        let recipe = [
            Node {
                name: "Peer0".into(),
                mode: WhatAmI::Peer,
                listen: vec![locator.clone()],
                task: vec![Task::Sleep(Duration::from_secs(2))],
                ..Default::default()
            },
            Node {
                name: "Peer1".into(),
                connect: vec![locator.clone()],
                mode: WhatAmI::Peer,
                task: vec![Task::Pub(topic.clone(), 8)],
                ..Default::default()
            },
            Node {
                name: "Peer2".into(),
                mode: WhatAmI::Peer,
                connect: vec![locator.clone()],
                task: vec![Task::Sub(topic.clone(), 8)],
                ..Default::default()
            },
        ];
        run_recipe(recipe).await?;
        println!("Gossip test passed.");
        Result::Ok(())
    })?;
    Ok(())
}

#[test]
fn failover_brokering() -> Result<()> {
    async_std::task::block_on(async {
        zasync_executor_init!();
        let locator = String::from("tcp/127.0.0.1:17448");
        let topic = String::from("testTopic");

        // Gossip test
        let recipe = [
            Node {
                name: "Peer0".into(),
                mode: WhatAmI::Peer,
                listen: vec![locator.clone()],
                task: vec![Task::Sleep(Duration::from_secs(2))],
                ..Default::default()
            },
            Node {
                name: "Peer1".into(),
                connect: vec![locator.clone()],
                mode: WhatAmI::Peer,
                task: vec![Task::Pub(topic.clone(), 8)],
                ..Default::default()
            },
            Node {
                name: "Peer2".into(),
                mode: WhatAmI::Peer,
                connect: vec![locator.clone()],
                task: vec![Task::Sub(topic.clone(), 8)],
                ..Default::default()
            },
        ];
        run_recipe(recipe).await?;
        println!("Gossip test passed.");
        Result::Ok(())
    })?;
    Ok(())
}

#[test]
fn pubsub_combination() -> Result<()> {
    async_std::task::block_on(async {
        zasync_executor_init!();
        env_logger::try_init()?;

        let modes = [WhatAmI::Peer, WhatAmI::Client];

        let mut idx = 0;
        let base_port = 17450;
        let recipe_list = modes
            .map(|n1| modes.map(|n2| (n1, n2)))
            .concat()
            .into_iter()
            .flat_map(|(n1, n2)| {
                MSG_SIZE.map(|msg_size| {
                    idx += 1;
                    let topic = format!("pubsub_combination_topic_{idx}");
                    let locator = format!("tcp/127.0.0.1:{}", base_port + idx);

                    [
                        Node {
                            name: "Router".into(),
                            mode: WhatAmI::Router,
                            listen: vec![locator.clone()],
                            task: vec![Task::Sleep(Duration::from_secs(30))],
                            ..Default::default()
                        },
                        Node {
                            name: "Node1".into(),
                            connect: vec![locator.clone()],
                            mode: n1,
                            task: vec![Task::Pub(topic.clone(), msg_size)],
                            ..Default::default()
                        },
                        Node {
                            name: "Node2".into(),
                            mode: n2,
                            connect: vec![locator],
                            task: vec![Task::Sub(topic, msg_size)],
                            ..Default::default()
                        },
                    ]
                })
            });

        for recipe in recipe_list {
            run_recipe(recipe).await?;
        }

        println!("Pub/sub combination test passed.");
        Result::Ok(())
    })?;
    Ok(())
}
