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
use async_std::prelude::FutureExt;
use futures::future::try_join_all;
use futures::FutureExt as _;
use std::str::FromStr;
use std::sync::atomic::Ordering;
use std::sync::{atomic::AtomicUsize, Arc};
use std::time::Duration;
use zenoh::config::{Config, ModeDependentValue};
use zenoh::prelude::r#async::*;
use zenoh::{value::Value, Result};
use zenoh_core::zasync_executor_init;
use zenoh_protocol::core::{WhatAmI, WhatAmIMatcher};
use zenoh_result::{bail, zerror};

const TIMEOUT: Duration = Duration::from_secs(360);
const MSG_COUNT: usize = 50;
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
    Checkpoint,
}

impl Task {
    async fn run(
        &self,
        session: Arc<Session>,
        remaining_checkpoints: Arc<AtomicUsize>,
    ) -> Result<()> {
        match self {
            // The Sub task checks if the incoming message matches the expected size until it receives enough counts.
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
                        println!("Sub received sufficient amount of messages. Done.");
                        break;
                    }
                }
            }

            // The Pub task keeps putting messages until all checkpoints are finished.
            Self::Pub(ke, payload_size) => {
                let value: Value = vec![0u8; *payload_size].into();
                while remaining_checkpoints.load(Ordering::Relaxed) > 0 {
                    ztimeout!(session
                        .put(ke, value.clone())
                        .congestion_control(CongestionControl::Block)
                        .res_async())?;
                }
                println!("Pub task done.");
            }

            // The Get task gets and checks if the incoming message matches the expected size until it receives enough counts.
            Self::Get(ke, expected_size) => {
                let mut counter = 0;
                while counter < MSG_COUNT {
                    let replies =
                        ztimeout!(session.get(ke).timeout(Duration::from_secs(10)).res_async())?;
                    while let Ok(reply) = replies.recv_async().await {
                        match reply.sample {
                            Ok(sample) => {
                                let recv_size = sample.value.payload.len();
                                if recv_size != *expected_size {
                                    bail!("Received payload size {recv_size} mismatches the expected {expected_size}");
                                }
                            }

                            Err(err) => {
                                log::warn!(
                                    "Sample got from {} failed to unwrap! Error: {}.",
                                    ke,
                                    err
                                );
                                continue;
                            }
                        }
                        counter += 1;
                    }
                }
                println!("Get got sufficient amount of messages. Done.");
            }

            // The Queryable task keeps replying to requested messages until all checkpoints are finished.
            Self::Queryable(ke, payload_size) => {
                let queryable = session.declare_queryable(ke).res_async().await?;
                let payload = vec![0u8; *payload_size];

                loop {
                    futures::select! {
                        query = queryable.recv_async() => {
                            query?.reply(KeyExpr::try_from(ke.to_owned())?, payload.clone()).res_async().await?;
                        },

                        _ = async_std::task::sleep(Duration::from_millis(100)).fuse() => {
                            if remaining_checkpoints.load(Ordering::Relaxed) == 0 {
                                break;
                            }
                        }
                    }
                }
                println!("Queryable task done.");
            }

            // Make the zenoh session sleep for a while.
            Self::Sleep(dur) => {
                async_std::task::sleep(*dur).await;
            }

            // Mark one checkpoint is finished.
            Self::Checkpoint => {
                if remaining_checkpoints.fetch_sub(1, Ordering::Relaxed) <= 1 {
                    println!("The end of the recipe.");
                }
            }

            // Wait until all checkpoints are done
            Self::Wait => {
                while remaining_checkpoints.load(Ordering::Relaxed) > 0 {
                    async_std::task::sleep(Duration::from_millis(100)).await;
                }
            }
        }
        Result::Ok(())
    }
}

// A sequential task consists of several tasks
type SequentialTask = Vec<Task>;
// A concurrent task consists of several sequential tasks
type ConcurrentTask = Vec<SequentialTask>;

// Each node represents one zenoh session
#[derive(Debug, Clone)]
struct Node {
    name: String,
    mode: WhatAmI,
    listen: Vec<String>,
    connect: Vec<String>,
    con_task: ConcurrentTask,
    config: Option<Config>,
    warmup: Duration,
}

impl Node {
    fn num_checkpoints(&self) -> usize {
        self.con_task
            .iter()
            .map(|seq_tasks| {
                seq_tasks
                    .iter()
                    .filter(|task| **task == Task::Checkpoint)
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
            con_task: vec![],
            config: None,
            warmup: Duration::from_secs(0),
        }
    }
}

// A recipe consists of several nodes (zenoh sessions) assigned with corresponding tasks
#[derive(Debug, Clone)]
struct Recipe {
    nodes: Vec<Node>,
}

// Display the Recipe as [NodeName1, NodeName2, ...]
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
        let num_checkpoints = self.num_checkpoints();
        let remaining_checkpoints = Arc::new(AtomicUsize::new(num_checkpoints));
        println!(
            "Recipe {} beging testing with {} checkpoint(s).",
            &self, &num_checkpoints
        );

        // All concurrent tasks to run
        let futures = self.nodes.clone().into_iter().map(move |node| {
            let receipe_name = self.to_string();
            // All nodes share the same checkpoint counter
            let remaining_checkpoints = remaining_checkpoints.clone();

            async move {
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

                // Warmup before the session starts
                async_std::task::sleep(node.warmup).await;
                println!("Node: {} starting...", &node.name);

                // In case of client can't connect to some peers/routers
                let session = loop {
                    if let Ok(session) = zenoh::open(config.clone()).res_async().await {
                        break session.into_arc();
                    } else {
                        async_std::task::sleep(Duration::from_secs(1)).await;
                    }
                };

                // Each node consists of a specified session associated with tasks to run
                let node_tasks = node.con_task.into_iter().map(|seq_tasks| {
                    // The tasks share the same session and checkpoint counter
                    let session = session.clone();
                    let remaining_checkpoints = remaining_checkpoints.clone();

                    async_std::task::spawn(async move {
                        // Tasks in seq_tasks would execute serially
                        for t in seq_tasks {
                            t.run(session.clone(), remaining_checkpoints.clone())
                                .await?;
                        }
                        Result::Ok(())
                    })
                });

                // All tasks of the node run together
                try_join_all(node_tasks.into_iter().map(async_std::task::spawn))
                    .await
                    .map_err(|e| zerror!("The recipe {} failed due to {}", receipe_name, &e))?;

                // Close the session once all the task assoicated with the node are done.
                Arc::try_unwrap(session)
                    .unwrap()
                    .close()
                    .res_async()
                    .await?;

                println!("Node: {} is closed.", &node.name);
                Result::Ok(())
            }
        });

        // All tasks of the recipe run together
        try_join_all(futures.into_iter().map(async_std::task::spawn))
            .timeout(TIMEOUT)
            .await
            .map_err(|e| format!("The recipe: {} failed due to {}", &self, e))??;
        Ok(())
    }
}

// Two peers connecting to a common node (either in router or peer mode) can discover each other.
// And the message transmission should work even if the common node disappears after a while.
#[test]
fn gossip() -> Result<()> {
    env_logger::try_init().unwrap_or_default();
    async_std::task::block_on(async {
        zasync_executor_init!();

        let locator = String::from("tcp/127.0.0.1:17446");
        let ke = String::from("testKeyExprGossip");
        let msg_size = 8;

        let peer1 = Node {
            name: format!("Pub & Queryable {}", WhatAmI::Peer),
            connect: vec![locator.clone()],
            mode: WhatAmI::Peer,
            con_task: ConcurrentTask::from([
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
        };
        let peer2 = Node {
            name: format!("Sub & Get {}", WhatAmI::Peer),
            mode: WhatAmI::Peer,
            connect: vec![locator.clone()],
            con_task: ConcurrentTask::from([
                SequentialTask::from([
                    Task::Sleep(Duration::from_millis(2000)),
                    Task::Sub(ke.clone(), msg_size),
                    Task::Checkpoint,
                ]),
                SequentialTask::from([
                    Task::Sleep(Duration::from_millis(2000)),
                    Task::Get(ke, msg_size),
                    Task::Checkpoint,
                ]),
            ]),
            ..Default::default()
        };

        for mode in [WhatAmI::Peer, WhatAmI::Router] {
            Recipe::new([
                Node {
                    name: format!("Router {}", mode),
                    mode: WhatAmI::Peer,
                    listen: vec![locator.clone()],
                    con_task: ConcurrentTask::from([SequentialTask::from([Task::Sleep(
                        Duration::from_millis(1000),
                    )])]),
                    ..Default::default()
                },
                peer1.clone(),
                peer2.clone(),
            ])
            .run()
            .await?;
        }

        println!("Gossip test passed.");
        Result::Ok(())
    })?;
    Ok(())
}

// Simulate two peers connecting to a router but not directly reachable to each other can exchange messages via the brokering by the router.
#[test]
fn static_failover_brokering() -> Result<()> {
    env_logger::try_init().unwrap_or_default();
    async_std::task::block_on(async {
        zasync_executor_init!();

        let locator = String::from("tcp/127.0.0.1:17449");
        let ke = String::from("testKeyExprStaticFailoverBrokering");
        let msg_size = 8;

        let disable_autoconnect_config = || {
            let mut config = Config::default();
            config
                .scouting
                .gossip
                .set_autoconnect(Some(ModeDependentValue::Unique(
                    WhatAmIMatcher::from_str("").unwrap(),
                )))
                .unwrap();
            Some(config)
        };

        let recipe = Recipe::new([
            Node {
                name: format!("Router {}", WhatAmI::Router),
                mode: WhatAmI::Router,
                listen: vec![locator.clone()],
                con_task: ConcurrentTask::from([SequentialTask::from([Task::Wait])]),
                ..Default::default()
            },
            Node {
                name: format!("Pub & Queryable {}", WhatAmI::Peer),
                mode: WhatAmI::Peer,
                connect: vec![locator.clone()],
                config: disable_autoconnect_config(),
                con_task: ConcurrentTask::from([
                    SequentialTask::from([Task::Pub(ke.clone(), msg_size)]),
                    SequentialTask::from([Task::Queryable(ke.clone(), msg_size)]),
                ]),
                ..Default::default()
            },
            Node {
                name: format!("Sub & Get {}", WhatAmI::Peer),
                mode: WhatAmI::Peer,
                connect: vec![locator.clone()],
                config: disable_autoconnect_config(),
                con_task: ConcurrentTask::from([
                    SequentialTask::from([Task::Sub(ke.clone(), msg_size), Task::Checkpoint]),
                    SequentialTask::from([Task::Get(ke.clone(), msg_size), Task::Checkpoint]),
                ]),
                ..Default::default()
            },
        ]);
        recipe.run().await?;
        println!("Static failover brokering test passed.");
        Result::Ok(())
    })?;
    Ok(())
}

// All test cases varying in
// 1. Message size
// 2. Mode: peer or client
// 3. Spawning order
#[test]
fn three_node_combination() -> Result<()> {
    env_logger::try_init().unwrap_or_default();
    async_std::task::block_on(async {
        zasync_executor_init!();

        let modes = [WhatAmI::Peer, WhatAmI::Client];
        let delay_in_secs = [
            (0, 1, 2),
            (0, 2, 1),
            (1, 2, 0),
            (1, 0, 2),
            (2, 0, 1),
            (2, 1, 0),
        ];

        let mut idx = 0;
        // Ports going to be used: 17451 to 17498
        let base_port = 17450;

        let recipe_list = modes
            .map(|n1| modes.map(|n2| (n1, n2)))
            .concat()
            .into_iter()
            .flat_map(|(n1, n2)| MSG_SIZE.map(|s| (n1, n2, s)))
            .flat_map(|(n1, n2, s)| delay_in_secs.map(|d| (n1, n2, s, d)))
            .map(
                |(node1_mode, node2_mode, msg_size, (delay1, delay2, delay3))| {
                    idx += 1;
                    let locator = format!("tcp/127.0.0.1:{}", base_port + idx);

                    let ke_pubsub = format!("three_node_combination_keyexpr_pubsub_{idx}");
                    let ke_getqueryable =
                        format!("three_node_combination_keyexpr_getqueryable_{idx}");

                    let router_node = Node {
                        name: format!("Router {}", WhatAmI::Router),
                        mode: WhatAmI::Router,
                        listen: vec![locator.clone()],
                        con_task: ConcurrentTask::from([SequentialTask::from([Task::Wait])]),
                        warmup: Duration::from_secs(delay1),
                        ..Default::default()
                    };

                    let (pub_node, queryable_node) =
                        {
                            let base = Node {
                                mode: node1_mode,
                                connect: vec![locator.clone()],
                                warmup: Duration::from_secs(delay2),
                                ..Default::default()
                            };

                            let mut pub_node = base.clone();
                            pub_node.name = format!("Pub {node1_mode}");
                            pub_node.con_task =
                                ConcurrentTask::from([SequentialTask::from([Task::Pub(
                                    ke_pubsub.clone(),
                                    msg_size,
                                )])]);

                            let mut queryable_node = base;
                            queryable_node.name = format!("Queryable {node1_mode}");
                            queryable_node.con_task = ConcurrentTask::from([SequentialTask::from(
                                [Task::Queryable(ke_getqueryable.clone(), msg_size)],
                            )]);

                            (pub_node, queryable_node)
                        };

                    let (sub_node, get_node) = {
                        let base = Node {
                            mode: node2_mode,
                            connect: vec![locator],
                            warmup: Duration::from_secs(delay3),
                            ..Default::default()
                        };

                        let mut sub_node = base.clone();
                        sub_node.name = format!("Sub {node2_mode}");
                        sub_node.con_task = ConcurrentTask::from([SequentialTask::from([
                            Task::Sub(ke_pubsub, msg_size),
                            Task::Checkpoint,
                        ])]);

                        let mut get_node = base;
                        get_node.name = format!("Get {node2_mode}");
                        get_node.con_task = ConcurrentTask::from([SequentialTask::from([
                            Task::Get(ke_getqueryable, msg_size),
                            Task::Checkpoint,
                        ])]);

                        (sub_node, get_node)
                    };

                    (
                        Recipe::new([router_node.clone(), pub_node, sub_node]),
                        Recipe::new([router_node, queryable_node, get_node]),
                    )
                },
            );

        for (pubsub, getqueryable) in recipe_list {
            pubsub.run().await?;
            getqueryable.run().await?;
        }

        println!("Three-node combination test passed.");
        Result::Ok(())
    })?;
    Ok(())
}

// All test cases varying in
// 1. Message size
// 2. Mode
#[test]
fn two_node_combination() -> Result<()> {
    async_std::task::block_on(async {
        zasync_executor_init!();

        #[derive(Clone, Copy)]
        struct IsFirstListen(bool);

        let modes = [
            (WhatAmI::Client, WhatAmI::Peer, IsFirstListen(false)),
            (WhatAmI::Peer, WhatAmI::Client, IsFirstListen(true)),
            (WhatAmI::Peer, WhatAmI::Peer, IsFirstListen(true)),
            (WhatAmI::Peer, WhatAmI::Peer, IsFirstListen(false)),
        ];

        let mut idx = 0;
        // Ports going to be used: 17500 to 17508
        let base_port = 17500;
        let recipe_list = modes
            .into_iter()
            .flat_map(|(n1, n2, who)| MSG_SIZE.map(|s| (n1, n2, who, s)))
            .map(|(node1_mode, node2_mode, who, msg_size)| {
                idx += 1;
                let ke_pubsub = format!("two_node_combination_keyexpr_pubsub_{idx}");
                let ke_getqueryable = format!("two_node_combination_keyexpr_getqueryable_{idx}");

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

                let (pub_node, queryable_node) = {
                    let base = Node {
                        mode: node1_mode,
                        listen: node1_listen_connect.0,
                        connect: node1_listen_connect.1,
                        ..Default::default()
                    };

                    let mut pub_node = base.clone();
                    pub_node.name = format!("Pub {node1_mode}");
                    pub_node.con_task = ConcurrentTask::from([SequentialTask::from([Task::Pub(
                        ke_pubsub.clone(),
                        msg_size,
                    )])]);

                    let mut queryable_node = base;
                    queryable_node.name = format!("Queryable {node1_mode}");
                    queryable_node.con_task =
                        ConcurrentTask::from([SequentialTask::from([Task::Queryable(
                            ke_getqueryable.clone(),
                            msg_size,
                        )])]);

                    (pub_node, queryable_node)
                };

                let (sub_node, get_node) = {
                    let base = Node {
                        mode: node2_mode,
                        listen: node2_listen_connect.0,
                        connect: node2_listen_connect.1,
                        ..Default::default()
                    };

                    let mut sub_node = base.clone();
                    sub_node.name = format!("Sub {node2_mode}");
                    sub_node.con_task = ConcurrentTask::from([SequentialTask::from([
                        Task::Sub(ke_pubsub, msg_size),
                        Task::Checkpoint,
                    ])]);

                    let mut get_node = base;
                    get_node.name = format!("Get {node2_mode}");
                    get_node.con_task = ConcurrentTask::from([SequentialTask::from([
                        Task::Get(ke_getqueryable, msg_size),
                        Task::Checkpoint,
                    ])]);

                    (sub_node, get_node)
                };

                (
                    Recipe::new([pub_node, sub_node]),
                    Recipe::new([queryable_node, get_node]),
                )
            });

        for (pubsub, getqueryable) in recipe_list {
            pubsub.run().await?;
            getqueryable.run().await?;
        }

        println!("Two-node combination test passed.");
        Result::Ok(())
    })?;
    Ok(())
}
