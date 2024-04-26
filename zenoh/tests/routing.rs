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
use std::str::FromStr;
use std::sync::atomic::Ordering;
use std::sync::{atomic::AtomicUsize, Arc};
use std::time::Duration;
use tokio_util::{sync::CancellationToken, task::TaskTracker};
use zenoh::config::{Config, ModeDependentValue};
use zenoh::prelude::r#async::*;
use zenoh::Result;
use zenoh_core::ztimeout;
use zenoh_protocol::core::{WhatAmI, WhatAmIMatcher};
use zenoh_result::bail;

const TIMEOUT: Duration = Duration::from_secs(10);
const MSG_COUNT: usize = 50;
const MSG_SIZE: [usize; 2] = [1_024, 131_072];
// Maximal recipes to run at once
const PARALLEL_RECIPES: usize = 4;

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
        token: CancellationToken,
    ) -> Result<()> {
        match self {
            // The Sub task checks if the incoming message matches the expected size until it receives enough counts.
            Self::Sub(ke, expected_size) => {
                let sub = ztimeout!(session.declare_subscriber(ke).res_async())?;
                let mut counter = 0;
                loop {
                    tokio::select! {
                        _  = token.cancelled() => break,
                        res = sub.recv_async() => {
                            if let Ok(sample) = res {
                                let recv_size = sample.payload().len();
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
                    }
                }
                println!("Sub task done.");
            }

            // The Pub task keeps putting messages until all checkpoints are finished.
            Self::Pub(ke, payload_size) => {
                loop {
                    tokio::select! {
                        _  = token.cancelled() => break,

                        // WARN: this won't yield after a timeout since the put is a blocking call
                        res = tokio::time::timeout(std::time::Duration::from_secs(1), session
                            .put(ke, vec![0u8; *payload_size])
                            .congestion_control(CongestionControl::Block)
                            .res()) => {
                            let _ = res?;
                        }
                    }
                }
                println!("Pub task done.");
            }

            // The Get task gets and checks if the incoming message matches the expected size until it receives enough counts.
            Self::Get(ke, expected_size) => {
                let mut counter = 0;
                while counter < MSG_COUNT {
                    tokio::select! {
                        _  = token.cancelled() => break,
                        replies = session.get(ke).timeout(Duration::from_secs(10)).res() => {
                            let replies = replies?;
                            while let Ok(reply) = replies.recv_async().await {
                                match reply.result() {
                                    Ok(sample) => {
                                        let recv_size = sample.payload().len();
                                        if recv_size != *expected_size {
                                            bail!("Received payload size {recv_size} mismatches the expected {expected_size}");
                                        }
                                    }

                                    Err(err) => {
                                        tracing::warn!(
                                            "Sample got from {} failed to unwrap! Error: {:?}.",
                                            ke,
                                            err
                                        );
                                        continue;
                                    }
                                }
                                counter += 1;
                            }
                        }
                    }
                }
                println!("Get got sufficient amount of messages. Done.");
            }

            // The Queryable task keeps replying to requested messages until all checkpoints are finished.
            Self::Queryable(ke, payload_size) => {
                let queryable = ztimeout!(session.declare_queryable(ke).res_async())?;
                let payload = vec![0u8; *payload_size];

                loop {
                    tokio::select! {
                        _  = token.cancelled() => break,
                        query = queryable.recv_async() => {
                            ztimeout!(query?.reply(ke.to_owned(), payload.clone()).res_async())?;
                        },
                    }
                }
                println!("Queryable task done.");
            }

            // Make the zenoh session sleep for a while.
            Self::Sleep(dur) => {
                tokio::time::sleep(*dur).await;
            }

            // Mark one checkpoint is finished.
            Self::Checkpoint => {
                if remaining_checkpoints.fetch_sub(1, Ordering::Relaxed) <= 1 {
                    token.cancel();
                    println!("The end of the recipe.");
                }
            }

            // Wait until all checkpoints are done
            Self::Wait => {
                token.cancelled().await;
            }
        }
        Ok(())
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
    token: CancellationToken,
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
        Self {
            nodes,
            token: CancellationToken::new(),
        }
    }

    fn num_checkpoints(&self) -> usize {
        self.nodes.iter().map(|node| node.num_checkpoints()).sum()
    }

    async fn run(&self) -> Result<()> {
        let num_checkpoints = self.num_checkpoints();
        let remaining_checkpoints = Arc::new(AtomicUsize::new(num_checkpoints));
        println!(
            "Recipe {} begin testing with {} checkpoint(s).",
            &self, &num_checkpoints
        );

        let mut recipe_join_set = tokio::task::JoinSet::new();

        // All concurrent tasks to run
        for node in self.nodes.clone() {
            // All nodes share the same checkpoint counter
            let remaining_checkpoints = remaining_checkpoints.clone();
            let token = self.token.clone();

            let recipe_task = async move {
                // Initiate
                let session = {
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
                            .set_endpoints(
                                node.connect.iter().map(|x| x.parse().unwrap()).collect(),
                            )
                            .unwrap();
                        config
                    };

                    // Warmup before the session starts
                    tokio::time::sleep(node.warmup).await;
                    println!("Node: {} starting...", &node.name);

                    // In case of client can't connect to some peers/routers
                    loop {
                        if let Ok(session) = ztimeout!(zenoh::open(config.clone()).res_async()) {
                            break session.into_arc();
                        } else {
                            tokio::time::sleep(Duration::from_secs(1)).await;
                        }
                    }
                };

                let mut node_join_set = tokio::task::JoinSet::new();
                for seq_tasks in node.con_task.into_iter() {
                    let token = token.clone();

                    // The tasks share the same session and checkpoint counter
                    let session = session.clone();
                    let remaining_checkpoints = remaining_checkpoints.clone();
                    node_join_set.spawn(async move {
                        // Tasks in seq_tasks would execute serially
                        for task in seq_tasks {
                            task.run(
                                session.clone(),
                                remaining_checkpoints.clone(),
                                token.clone(),
                            )
                            .await?;
                        }
                        Result::Ok(())
                    });
                }

                while let Some(res) = node_join_set.join_next().await {
                    res??;
                }
                // node_task_tracker.close();
                // node_task_tracker.wait().await;

                // Close the session once all the task assoicated with the node are done.
                ztimeout!(Arc::try_unwrap(session).unwrap().close().res_async())?;

                println!("Node: {} is closed.", &node.name);
                Result::Ok(())
            };
            recipe_join_set.spawn(recipe_task);
        }

        // All tasks of the recipe run together
        loop {
            tokio::select! {
                _ = tokio::time::sleep(TIMEOUT) => {
                    dbg!("Timeout");

                    // Termination
                    remaining_checkpoints.swap(0, Ordering::Relaxed);
                    self.token.cancel();
                    while let Some(res) = recipe_join_set.join_next().await {
                        res??;
                    }
                    bail!("Timeout");
                },
                res = recipe_join_set.join_next() => {
                    if let Some(res) = res {
                        res??;
                    } else {
                        break
                    }
                }
            }
        }

        Ok(())
    }
}

// Two peers connecting to a common node (either in router or peer mode) can discover each other.
// And the message transmission should work even if the common node disappears after a while.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn gossip() -> Result<()> {
    zenoh_util::try_init_log_from_env();

    let locator = String::from("tcp/127.0.0.1:17446");
    let ke = String::from("testKeyExprGossip");
    let msg_size = 8;

    // node1 in peer mode playing pub and queryable
    let node1 = Node {
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
    // node2 in peer mode playing sub and get
    let node2 = Node {
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

    // Recipes:
    // - node1: Peer, node2: Peer, node3: Peer
    // - node1: Peer, node2: Peer, node3: Router
    for mode in [WhatAmI::Peer, WhatAmI::Router] {
        let node3 = Node {
            name: format!("Router {}", mode),
            mode: WhatAmI::Peer,
            listen: vec![locator.clone()],
            con_task: ConcurrentTask::from([SequentialTask::from([Task::Sleep(
                Duration::from_millis(1000),
            )])]),
            ..Default::default()
        };
        Recipe::new([node1.clone(), node2.clone(), node3])
            .run()
            .await?;
    }

    println!("Gossip test passed.");
    Result::Ok(())
}

// Simulate two peers connecting to a router but not directly reachable to each other can exchange messages via the brokering by the router.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn static_failover_brokering() -> Result<()> {
    zenoh_util::try_init_log_from_env();
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
}

// All test cases varying in
// 1. Message size: 2 (sizes)
// 2. Mode: {Client, Peer} x {Client x Peer} x {Router} = 2 x 2 x 1 = 4 (cases)
// 3. Spawning order (delay_in_secs for node1, node2, and node3) = 6 (cases)
//
// Total cases = 2 x 4 x 6 = 48
#[tokio::test(flavor = "multi_thread", worker_threads = 9)]
async fn three_node_combination() -> Result<()> {
    zenoh_util::try_init_log_from_env();
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

    let recipe_list: Vec<_> = modes
        .map(|n1| modes.map(|n2| (n1, n2)))
        .concat()
        .into_iter()
        .flat_map(|(n1, n2)| [1024].map(|s| (n1, n2, s)))
        .flat_map(|(n1, n2, s)| delay_in_secs.map(|d| (n1, n2, s, d)))
        .map(
            |(node1_mode, node2_mode, msg_size, (delay1, delay2, delay3))| {
                idx += 1;
                let locator = format!("tcp/127.0.0.1:{}", base_port + idx);

                let ke_pubsub = format!("three_node_combination_keyexpr_pubsub_{idx}");
                let ke_getqueryable = format!("three_node_combination_keyexpr_getqueryable_{idx}");

                use rand::Rng;
                let mut rng = rand::thread_rng();

                let router_node = Node {
                    name: format!("Router {}", WhatAmI::Router),
                    mode: WhatAmI::Router,
                    listen: vec![locator.clone()],
                    con_task: ConcurrentTask::from([SequentialTask::from([Task::Wait])]),
                    warmup: Duration::from_secs(delay1)
                        + Duration::from_millis(rng.gen_range(0..500)),
                    ..Default::default()
                };

                let (pub_node, queryable_node) = {
                    let base = Node {
                        mode: node1_mode,
                        connect: vec![locator.clone()],
                        warmup: Duration::from_secs(delay2),
                        ..Default::default()
                    };

                    let mut pub_node = base.clone();
                    pub_node.name = format!("Pub {node1_mode}");
                    pub_node.con_task = ConcurrentTask::from([SequentialTask::from([Task::Pub(
                        ke_pubsub.clone(),
                        msg_size,
                    )])]);
                    pub_node.warmup += Duration::from_millis(rng.gen_range(0..500));

                    let mut queryable_node = base;
                    queryable_node.name = format!("Queryable {node1_mode}");
                    queryable_node.con_task =
                        ConcurrentTask::from([SequentialTask::from([Task::Queryable(
                            ke_getqueryable.clone(),
                            msg_size,
                        )])]);
                    queryable_node.warmup += Duration::from_millis(rng.gen_range(0..500));

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
                    sub_node.warmup += Duration::from_millis(rng.gen_range(0..500));

                    let mut get_node = base;
                    get_node.name = format!("Get {node2_mode}");
                    get_node.con_task = ConcurrentTask::from([SequentialTask::from([
                        Task::Get(ke_getqueryable, msg_size),
                        Task::Checkpoint,
                    ])]);
                    get_node.warmup += Duration::from_millis(rng.gen_range(0..500));

                    (sub_node, get_node)
                };

                (
                    Recipe::new([router_node.clone(), pub_node, sub_node]),
                    Recipe::new([router_node, queryable_node, get_node]),
                )
            },
        )
        .collect();

    for chunks in recipe_list.chunks(4).map(|x| x.to_vec()) {
        let mut join_set = tokio::task::JoinSet::new();
        for (pubsub, getqueryable) in chunks {
            join_set.spawn(async move {
                pubsub.run().await?;
                getqueryable.run().await?;
                Result::Ok(())
            });
        }

        while let Some(res) = join_set.join_next().await {
            res??;
        }
    }

    println!("Three-node combination test passed.");
    Ok(())
}

// All test cases varying in
// 1. Message size: 2 (sizes)
// 2. Mode: {Client, Peer} x {Client, Peer} x {IsFirstListen} = 2 x 2 x 2 = 8 (modes)
//
// Total cases = 2 x 8 = 16
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn two_node_combination() -> Result<()> {
    zenoh_util::try_init_log_from_env();

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
    let recipe_list: Vec<_> = modes
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
        })
        .collect();

    for chunks in recipe_list.chunks(PARALLEL_RECIPES).map(|x| x.to_vec()) {
        let task_tracker = TaskTracker::new();
        for (pubsub, getqueryable) in chunks {
            task_tracker.spawn(async move {
                pubsub.run().await?;
                getqueryable.run().await?;
                Result::Ok(())
            });
        }
        task_tracker.close();
        task_tracker.wait().await;
    }

    println!("Two-node combination test passed.");
    Result::Ok(())
}
