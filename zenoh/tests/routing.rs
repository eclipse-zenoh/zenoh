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
use futures::future::try_join_all;
use futures::FutureExt as _;
use std::sync::atomic::Ordering;
use std::sync::{atomic::AtomicUsize, Arc};
use std::time::Duration;

use async_std::prelude::FutureExt;
use zenoh::config::{whatami::WhatAmI, Config};
use zenoh::prelude::r#async::*;
use zenoh::{value::Value, Result};
use zenoh_config::whatami::WhatAmIMatcher;
use zenoh_config::ModeDependentValue;
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

#[derive(Debug, Clone, PartialEq, Eq)]
enum Task {
    Pub(String, usize),
    Sub(String, usize),
    Queryable(String, usize),
    Get(String, usize),
    Sleep(Duration),
    Wait,
    CheckPoint,
}

impl Task {
    async fn run(
        &self,
        session: Arc<Session>,
        remaining_checkpoints: Arc<AtomicUsize>,
    ) -> Result<()> {
        match self {
            Self::Sub(ke, expected_size) => {
                let sub = ztimeout!(session.declare_subscriber(ke).res_async())?;
                let mut counter = 0;
                while let Ok(sample) = sub.recv_async().await {
                    let recv_size = sample.value.payload.len();
                    if recv_size != *expected_size {
                        bail!("Received payload size {recv_size} mismatches the expected {expected_size}");
                    }
                    counter += 1;
                    if counter >= MSG_COUNT {
                        println!("Sub received sufficient amount of messages.");
                        println!("Sub task done.");
                        break;
                    }
                }
            }

            Self::Pub(ke, payload_size) => {
                let value: Value = vec![0u8; *payload_size].into();
                // while !terminated.load(Ordering::Relaxed) {
                while remaining_checkpoints.load(Ordering::Relaxed) > 0 {
                    ztimeout!(session
                        .put(ke, value.clone())
                        .congestion_control(CongestionControl::Block)
                        .res_async())?;
                }
                println!("Pub task done.");
            }

            Self::Get(ke, expected_size) => {
                let mut counter = 0;
                while counter < MSG_COUNT {
                    let replies = ztimeout!(session.get(ke).res_async())?;
                    while let Ok(reply) = replies.recv_async().await {
                        let recv_size = reply.sample?.value.payload.len();
                        if recv_size != *expected_size {
                            bail!("Received payload size {recv_size} mismatches the expected {expected_size}");
                        }
                        counter += 1;
                    }
                }
                println!("Get task done.");
            }

            Self::Queryable(ke, payload_size) => {
                let queryable = session.declare_queryable(ke).res_async().await?;
                let sample = Sample::try_from(ke.clone(), vec![0u8; *payload_size])?;

                loop {
                    futures::select! {
                        query = queryable.recv_async() => {
                            query?.reply(Ok(sample.clone())).res_async().await?;
                        },

                        _ = async_std::task::sleep(Duration::from_millis(300)).fuse() => {
                            if remaining_checkpoints.load(Ordering::Relaxed) == 0 {
                                break;
                            }
                        }
                    }
                }
                println!("Queryable task done.");
            }

            Self::Sleep(dur) => {
                async_std::task::sleep(*dur).await;
            }

            Self::CheckPoint => {
                if remaining_checkpoints.fetch_sub(1, Ordering::Relaxed) <= 1 {
                    println!("The end of the recipe.");
                }
            }

            Self::Wait => {
                // while !terminated.load(Ordering::Relaxed) {
                while remaining_checkpoints.load(Ordering::Relaxed) > 0 {
                    async_std::task::sleep(Duration::from_millis(100)).await;
                }
            }
        }
        Result::Ok(())
    }
}

type SequentialTask = Vec<Task>;
type ConcurrentTask = Vec<SequentialTask>;

#[derive(Debug, Clone)]
struct Node {
    name: String,
    mode: WhatAmI,
    listen: Vec<String>,
    connect: Vec<String>,
    task: ConcurrentTask,
    config: Option<Config>,
    warmup: Duration,
}

impl Node {
    fn num_checkpoints(&self) -> usize {
        self.task
            .iter()
            .map(|seq_tasks| {
                seq_tasks
                    .iter()
                    .filter(|task| **task == Task::CheckPoint)
                    .count()
            })
            .sum()
    }
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
            warmup: Duration::from_secs(0),
        }
    }
}

#[derive(Debug, Clone)]
struct Recipe {
    nodes: Vec<Node>,
}

impl std::fmt::Display for Recipe {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let names: Vec<_> = self.nodes.iter().map(|node| node.name.clone()).collect();
        write!(f, "[{}]", names.join(", "))
    }
}

impl Recipe {
    fn new(nodes: impl IntoIterator<Item = Node>) -> Self {
        let nodes = nodes.into_iter().collect();
        Self { nodes }
    }

    fn num_checkpoints(&self) -> usize {
        self.nodes.iter().map(|node| node.num_checkpoints()).sum()
    }

    async fn run(&self) -> Result<()> {
        // let terminated = Arc::new(AtomicBool::default());
        let num_checkpoints = self.num_checkpoints();
        let remaining_checkpoints = Arc::new(AtomicUsize::new(num_checkpoints));
        println!("Recipe beging testing with {num_checkpoints} checkpoint(s).");

        let futures = self.nodes.clone().into_iter().map(|node| {
            println!("Node: {} started.", &node.name);

            // // Multiple nodes share the same terminated flag
            let remaining_checkpoints = remaining_checkpoints.clone();

            async move {
                dbg!(&node.name);

                // Load the config and build up a session
                let config = {
                    let mut config = node.config.unwrap_or_default();
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
                    config
                };

                async_std::task::sleep(node.warmup).await;

                // In case of client can't connect to some peers/routers
                let session = if let Ok(session) = zenoh::open(config.clone()).res_async().await {
                    session.into_arc()
                } else {
                    println!("Sleep and retry to connect to the endpoint...");
                    async_std::task::sleep(Duration::from_secs(1)).await;
                    zenoh::open(config).res_async().await?.into_arc()
                };

                // Each node consists of a specified session associated with tasks to run
                let futs = node.task.into_iter().map(|seq_tasks| {
                    // Multiple tasks share the session
                    let session = session.clone();
                    // let terminated = seq_task_terminated.clone();
                    let remaining_checkpoints = remaining_checkpoints.clone();
                    async_std::task::spawn(async move {
                        for t in seq_tasks {
                            // t.run(session.clone(), seq_task_terminated.clone()).await?;
                            t.run(session.clone(), remaining_checkpoints.clone())
                                .await?;
                        }
                        Result::Ok(())
                    })
                });

                // let (state, _, _) = select_all(futs).await;
                // state
                try_join_all(futs).await?;
                Arc::try_unwrap(session)
                    .unwrap()
                    .close()
                    .res_async()
                    .await?;
                println!("Node: {} is closed.", &node.name);
                Result::Ok(())
            }
        });

        // let (state, _, _) = select_all(futures).timeout(TIMEOUT).await?;
        // state
        try_join_all(futures).timeout(TIMEOUT).await??;
        Ok(())
    }
}

#[test]
fn gossip() -> Result<()> {
    async_std::task::block_on(async {
        zasync_executor_init!();
        env_logger::try_init()?;

        let locator = String::from("tcp/127.0.0.1:17448");
        let ke = String::from("testKeyExprGossip");
        let msg_size = 8;

        // Gossip test
        let recipe = Recipe::new([
            Node {
                name: format!("RTR {}", WhatAmI::Peer),
                mode: WhatAmI::Peer,
                listen: vec![locator.clone()],
                task: ConcurrentTask::from([SequentialTask::from([Task::Sleep(
                    Duration::from_millis(1000),
                )])]),
                ..Default::default()
            },
            Node {
                name: format!("Pub & Queryable {}", WhatAmI::Peer),
                connect: vec![locator.clone()],
                mode: WhatAmI::Peer,
                task: ConcurrentTask::from([
                    SequentialTask::from([
                        Task::Sleep(Duration::from_millis(2000)),
                        Task::Pub(ke.clone(), msg_size),
                    ]),
                    SequentialTask::from([
                        Task::Sleep(Duration::from_millis(2000)),
                        Task::Queryable(ke.clone(), msg_size),
                    ]),
                ]),
                ..Default::default()
            },
            Node {
                name: format!("Sub & Get {}", WhatAmI::Peer),
                mode: WhatAmI::Peer,
                connect: vec![locator.clone()],
                task: ConcurrentTask::from([
                    SequentialTask::from([
                        Task::Sleep(Duration::from_millis(2000)),
                        Task::Sub(ke.clone(), msg_size),
                        Task::CheckPoint,
                    ]),
                    SequentialTask::from([
                        Task::Sleep(Duration::from_millis(2000)),
                        Task::Get(ke, msg_size),
                        Task::CheckPoint,
                    ]),
                ]),
                ..Default::default()
            },
        ]);

        println!("[Recipe]: {}", &recipe);
        recipe.run().await?;
        println!("Gossip test passed.");
        Result::Ok(())
    })?;
    Ok(())
}

#[test]
fn static_failover_brokering() -> Result<()> {
    async_std::task::block_on(async {
        zasync_executor_init!();
        // env_logger::try_init()?;

        let locator = String::from("tcp/127.0.0.1:17449");
        let ke = String::from("testKeyExprStaticFailoverBrokering");
        let msg_size = 8;

        let disable_autoconnect_config = || {
            let mut config = Config::default();
            config
                .scouting
                .gossip
                .set_autoconnect(Some(ModeDependentValue::Unique(WhatAmIMatcher::try_from(
                    128,
                )?)))
                .unwrap();
            Some(config)
        };

        let recipe = Recipe::new([
            Node {
                name: format!("RTR {}", WhatAmI::Router),
                mode: WhatAmI::Router,
                listen: vec![locator.clone()],
                task: ConcurrentTask::from([SequentialTask::from([Task::Wait])]),
                ..Default::default()
            },
            Node {
                name: format!("Pub {}", WhatAmI::Peer),
                mode: WhatAmI::Peer,
                connect: vec![locator.clone()],
                config: disable_autoconnect_config(),
                task: ConcurrentTask::from([
                    SequentialTask::from([Task::Pub(ke.clone(), msg_size)]),
                    SequentialTask::from([Task::Queryable(ke.clone(), msg_size)]),
                ]),
                ..Default::default()
            },
            Node {
                name: format!("Sub {}", WhatAmI::Peer),
                mode: WhatAmI::Peer,
                connect: vec![locator.clone()],
                config: disable_autoconnect_config(),
                task: ConcurrentTask::from([
                    SequentialTask::from([Task::Sub(ke.clone(), msg_size), Task::CheckPoint]),
                    SequentialTask::from([Task::Get(ke.clone(), msg_size), Task::CheckPoint]),
                ]),
                ..Default::default()
            },
        ]);
        println!("[Recipe]: {}", &recipe);
        recipe.run().await?;
        println!("Static failover brokering test passed.");
        Result::Ok(())
    })?;
    Ok(())
}

#[test]
fn pubsub_combination() -> Result<()> {
    async_std::task::block_on(async {
        zasync_executor_init!();
        // env_logger::try_init()?;

        let modes = [WhatAmI::Peer, WhatAmI::Client];

        let mut idx = 0;
        let base_port = 17450;
        let recipe_list = modes
            .map(|n1| modes.map(|n2| (n1, n2)))
            .concat()
            .into_iter()
            .flat_map(|(n1, n2)| MSG_SIZE.map(|s| (n1, n2, s)))
            .flat_map(|(node1_mode, node2_mode, msg_size)| {
                idx += 1;
                let ke = format!("pubsub_combination_keyexpr_{idx}");
                let locator = format!("tcp/127.0.0.1:{}", base_port + idx);

                let recipe = vec![
                    Node {
                        name: format!("RTR {}", WhatAmI::Router),
                        mode: WhatAmI::Router,
                        listen: vec![locator.clone()],
                        task: ConcurrentTask::from([SequentialTask::from([Task::Wait])]),
                        ..Default::default()
                    },
                    Node {
                        name: format!("Pub {node1_mode}"),
                        mode: node1_mode,
                        connect: vec![locator.clone()],
                        task: ConcurrentTask::from([
                            SequentialTask::from([Task::Pub(ke.clone(), msg_size)]),
                            SequentialTask::from([Task::Queryable(ke.clone(), msg_size)]),
                        ]),
                        ..Default::default()
                    },
                    Node {
                        name: format!("Sub {node2_mode}"),
                        mode: node2_mode,
                        connect: vec![locator],
                        task: ConcurrentTask::from([
                            SequentialTask::from([
                                Task::Sub(ke.clone(), msg_size),
                                Task::CheckPoint,
                            ]),
                            SequentialTask::from([
                                Task::Get(ke, msg_size),
                                Task::CheckPoint,
                            ]),
                        ]),
                        ..Default::default()
                    },
                ];

                // All permutations
                vec![
                    Recipe::new([recipe[0].clone(), recipe[1].clone(), recipe[2].clone()]),
                    Recipe::new([recipe[0].clone(), recipe[2].clone(), recipe[1].clone()]),
                    Recipe::new([recipe[1].clone(), recipe[0].clone(), recipe[2].clone()]),
                    Recipe::new([recipe[1].clone(), recipe[2].clone(), recipe[0].clone()]),
                    Recipe::new([recipe[2].clone(), recipe[0].clone(), recipe[1].clone()]),
                    Recipe::new([recipe[2].clone(), recipe[1].clone(), recipe[0].clone()]),
                ]
            });

        for recipe in recipe_list {
            println!("\n\n[Recipe]: {} begin running...", &recipe);
            recipe.run().await?;
            println!("[Recipe]: {} finished", &recipe);
        }

        // try_join_all(recipe_list.map(|r| async move {r.run().await?; Result::Ok(())} )).await?;

        println!("Pub/sub combination test passed.");
        Result::Ok(())
    })?;
    Ok(())
}

#[test]
fn p2p_combination() -> Result<()> {
    async_std::task::block_on(async {
        zasync_executor_init!();
        // env_logger::try_init()?;

        #[derive(Clone, Copy)]
        struct IsFirstListen(bool);

        let modes = [
            (WhatAmI::Client, WhatAmI::Peer, IsFirstListen(false)),
            (WhatAmI::Peer, WhatAmI::Client, IsFirstListen(true)),
            (WhatAmI::Peer, WhatAmI::Peer, IsFirstListen(true)),
            (WhatAmI::Peer, WhatAmI::Peer, IsFirstListen(false)),
        ];

        let mut idx = 0;
        let base_port = 17470;
        let recipe_list = modes
            .into_iter()
            .flat_map(|(n1, n2, who)| MSG_SIZE.map(|s| (n1, n2, who, s)))
            .map(|(node1_mode, node2_mode, who, msg_size)| {
                idx += 1;
                let ke = format!("p2p_combination_keyexpr_{idx}");

                let (node1_listen_connect, node2_listen_connect) = {
                    let locator = format!("tcp/127.0.0.1:{}", base_port + idx);
                    let listen = vec![locator];
                    let connect = vec![];

                    if let IsFirstListen(true) = who {
                        ((listen.clone(), connect.clone()), (connect, listen))
                    } else {
                        ((connect.clone(), listen.clone()), (listen, connect))
                    }
                };

                Recipe::new([
                    Node {
                        name: format!("Pub {node1_mode}"),
                        mode: node1_mode,
                        listen: node1_listen_connect.0,
                        connect: node1_listen_connect.1,
                        task: ConcurrentTask::from([
                            SequentialTask::from([Task::Pub(ke.clone(), msg_size)]),
                            SequentialTask::from([Task::Queryable(ke.clone(), msg_size)]),
                        ]),
                        ..Default::default()
                    },
                    Node {
                        name: format!("Sub {node2_mode}"),
                        mode: node2_mode,
                        listen: node2_listen_connect.0,
                        connect: node2_listen_connect.1,
                        task: ConcurrentTask::from([
                            SequentialTask::from([
                                Task::Sub(ke.clone(), msg_size),
                                Task::CheckPoint,
                            ]),
                            SequentialTask::from([
                                Task::Get(ke, msg_size),
                                Task::CheckPoint,
                            ]),
                        ]),
                        ..Default::default()
                    },
                ])
            });

        for recipe in recipe_list {
            println!("[Recipe]: {}", &recipe);
            recipe.run().await?;
        }

        println!("P2P pub/sub combination test passed.");
        Result::Ok(())
    })?;
    Ok(())
}
