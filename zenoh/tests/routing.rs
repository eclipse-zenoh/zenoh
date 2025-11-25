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

#![cfg(feature = "internal_config")]

use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use itertools::Itertools;
use tokio_util::{sync::CancellationToken, task::TaskTracker};
use zenoh::{config::WhatAmI, qos::CongestionControl, Config, Result, Session};
use zenoh_config::{ModeDependentValue, WhatAmIMatcher};
use zenoh_core::ztimeout;
use zenoh_result::bail;

const TIMEOUT: Duration = Duration::from_secs(10);
const MSG_COUNT: usize = 50;
const LIVELINESSGET_DELAY: Duration = Duration::from_millis(10);

#[derive(Debug, Clone, PartialEq, Eq)]
enum Task {
    Pub(String, usize),
    Sub(String, usize),
    Queryable(String, usize),
    Get(String, usize),
    Liveliness(String),
    LivelinessGet(String),
    LivelinessLoop(String),
    LivelinessSub(String),
    Sleep(Duration),
    Wait,
    Checkpoint,
}

impl Task {
    async fn run(
        &self,
        session: Session,
        remaining_checkpoints: Arc<AtomicUsize>,
        token: CancellationToken,
    ) -> Result<()> {
        match self {
            // The Sub task checks if the incoming message matches the expected size until it receives enough counts.
            Self::Sub(ke, expected_size) => {
                let sub = ztimeout!(session.declare_subscriber(ke))?;
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
                        res = tokio::time::timeout(std::time::Duration::from_secs(1), async {session
                            .put(ke, vec![0u8; *payload_size])
                            .congestion_control(CongestionControl::Block)
                            .await
                        }) => {
                            let _ = res?;
                        }
                    }
                }
                println!("Pub task done.");
            }

            // The Queryable task keeps replying to requested messages until all checkpoints are finished.
            Self::Queryable(ke, payload_size) => {
                let queryable = ztimeout!(session.declare_queryable(ke))?;
                let payload = vec![0u8; *payload_size];

                loop {
                    tokio::select! {
                        _  = token.cancelled() => break,
                        query = queryable.recv_async() => {
                            ztimeout!(query?.reply(ke.to_owned(), payload.clone()))?;
                        },
                    }
                }
                println!("Queryable task done.");
            }

            // The Get task gets and checks if the incoming message matches the expected size until it receives enough counts.
            Self::Get(ke, expected_size) => {
                let mut counter = 0;
                while counter < MSG_COUNT {
                    tokio::select! {
                        _  = token.cancelled() => break,
                        replies = async { session.get(ke).timeout(Duration::from_secs(10)).await } => {
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

            // The Liveliness task.
            Self::Liveliness(ke) => {
                let _liveliness = ztimeout!(session.liveliness().declare_token(ke))?;

                token.cancelled().await;
                println!("Liveliness task done.");
            }

            // The LivelinessGet task.
            Self::LivelinessGet(ke) => {
                let mut counter = 0;
                while counter < MSG_COUNT {
                    tokio::select! {
                        _  = token.cancelled() => break,
                        replies = async { session.liveliness().get(ke).timeout(Duration::from_secs(10)).await } => {
                            let replies = replies?;
                            while let Ok(reply) = replies.recv_async().await {
                                if let Err(err) = reply.result() {
                                    tracing::warn!(
                                        "Sample got from {} failed to unwrap! Error: {:?}.",
                                        ke,
                                        err
                                    );
                                    continue;
                                }
                                counter += 1;
                            }
                            tokio::time::sleep(LIVELINESSGET_DELAY).await;
                        }
                    }
                }
                println!("LivelinessGet got sufficient amount of messages. Done.");
            }

            // The LivelinessLoop task.
            Self::LivelinessLoop(ke) => {
                let mut liveliness: Option<zenoh::liveliness::LivelinessToken> = None;

                loop {
                    match liveliness.take() {
                        Some(liveliness) => {
                            tokio::select! {
                                _  = token.cancelled() => break,
                                res = tokio::time::timeout(std::time::Duration::from_secs(1), async {liveliness.undeclare().await}) => {
                                    _ = res?;
                                }
                            }
                        }
                        None => {
                            tokio::select! {
                                _  = token.cancelled() => break,
                                res = tokio::time::timeout(std::time::Duration::from_secs(1), async {session.liveliness().declare_token(ke)
                                    .await
                                }) => {
                                    liveliness = res?.ok();
                                }
                            }
                        }
                    }
                }
                println!("LivelinessLoop task done.");
            }

            // The LivelinessSub task.
            Self::LivelinessSub(ke) => {
                let sub = ztimeout!(session.liveliness().declare_subscriber(ke))?;
                let mut counter = 0;
                loop {
                    tokio::select! {
                        _  = token.cancelled() => break,
                        res = tokio::time::timeout(TIMEOUT, sub.recv_async()) => {
                            if res?.is_ok() {
                                counter += 1;
                                if counter >= MSG_COUNT {
                                    println!("LivelinessSub received sufficient amount of messages. Done.");
                                    break;
                                }
                            } else {
                                println!("LivelinessSub recv error.");
                                break;
                            }
                        }
                    }
                }
                println!("LivelinessSub task done.");
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

impl std::fmt::Display for Node {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}({:#?})", self.name, self.warmup)
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
        let mut names = self.nodes.iter().map(|node| node.to_string());
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
                        if !node.listen.is_empty() {
                            config
                                .listen
                                .endpoints
                                .set(node.listen.iter().map(|x| x.parse().unwrap()).collect())
                                .unwrap();
                        }
                        if !node.connect.is_empty() {
                            config
                                .connect
                                .endpoints
                                .set(node.connect.iter().map(|x| x.parse().unwrap()).collect())
                                .unwrap();
                        }
                        config
                    };

                    // Warmup before the session starts
                    tokio::time::sleep(node.warmup).await;
                    println!("Node: {} starting...", &node.name);

                    // In case of client can't connect to some peers/routers
                    loop {
                        if let Ok(session) = ztimeout!(zenoh::open(config.clone())) {
                            break session;
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

                // Close the session once all the task associated with the node are done.
                ztimeout!(session.close())?;

                println!("Node: {} is closed.", &node.name);
                Result::Ok(())
            };
            recipe_join_set.spawn(recipe_task);
        }

        // All tasks of the recipe run together
        loop {
            tokio::select! {
                _ = tokio::time::sleep(TIMEOUT) => {
                    println!("Recipe {self} Timeout.");

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

        println!("Recipe {self} OK.");
        Ok(())
    }
}

// Two peers connecting to a common node (either in router or peer mode) can discover each other.
// And the message transmission should work even if the common node disappears after a while.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn gossip() -> Result<()> {
    zenoh::init_log_from_env_or("error");

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
                Task::Sleep(Duration::from_millis(4000)),
                Task::Pub(ke.clone(), msg_size),
            ]),
            SequentialTask::from([
                Task::Sleep(Duration::from_millis(4000)),
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
                Task::Sleep(Duration::from_millis(4000)),
                Task::Sub(ke.clone(), msg_size),
                Task::Checkpoint,
            ]),
            SequentialTask::from([
                Task::Sleep(Duration::from_millis(4000)),
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
            name: format!("Router {mode}"),
            mode: WhatAmI::Peer,
            listen: vec![locator.clone()],
            con_task: ConcurrentTask::from([SequentialTask::from([Task::Sleep(
                Duration::from_millis(3000),
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
    zenoh::init_log_from_env_or("error");
    let locator = String::from("tcp/127.0.0.1:17449");
    let ke = String::from("testKeyExprStaticFailoverBrokering");
    let msg_size = 8;

    let disable_autoconnect_config = || {
        let mut config = Config::default();
        config
            .scouting
            .gossip
            .set_autoconnect(Some(ModeDependentValue::Unique(WhatAmIMatcher::empty())))
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

const MSG_SIZE: [usize; 2] = [1_024, 131_072];
// Maximal recipes to run at once
const PARALLEL_RECIPES: usize = 4;

// All test cases varying in
// 1. Message size: 2 (sizes)
// 2. Mode: {Client, Peer} x {Client x Peer} x {Router} = 2 x 2 x 1 = 4 (cases)
// 3. Spawning order (delay_in_secs for node1, node2, and node3) = 6 (cases)
//
// Total cases = 2 x 4 x 6 = 48
#[tokio::test(flavor = "multi_thread", worker_threads = 9)]
async fn three_node_combination() -> Result<()> {
    zenoh::init_log_from_env_or("error");
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
                let ke_getliveliness =
                    format!("three_node_combination_keyexpr_getliveliness_{idx}");
                let ke_subliveliness =
                    format!("three_node_combination_keyexpr_subliveliness_{idx}");

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

                let (pub_node, queryable_node, liveliness_node, livelinessloop_node) = {
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

                    let mut queryable_node = base.clone();
                    queryable_node.name = format!("Queryable {node1_mode}");
                    queryable_node.con_task =
                        ConcurrentTask::from([SequentialTask::from([Task::Queryable(
                            ke_getqueryable.clone(),
                            msg_size,
                        )])]);
                    queryable_node.warmup += Duration::from_millis(rng.gen_range(0..500));

                    let mut liveliness_node = base.clone();
                    liveliness_node.name = format!("Liveliness {node1_mode}");
                    liveliness_node.con_task =
                        ConcurrentTask::from([SequentialTask::from([Task::Liveliness(
                            ke_getliveliness.clone(),
                        )])]);
                    liveliness_node.warmup += Duration::from_millis(rng.gen_range(0..500));

                    let mut livelinessloop_node = base;
                    livelinessloop_node.name = format!("LivelinessLoop {node1_mode}");
                    livelinessloop_node.con_task =
                        ConcurrentTask::from([SequentialTask::from([Task::LivelinessLoop(
                            ke_subliveliness.clone(),
                        )])]);
                    livelinessloop_node.warmup += Duration::from_millis(rng.gen_range(0..500));

                    (
                        pub_node,
                        queryable_node,
                        liveliness_node,
                        livelinessloop_node,
                    )
                };

                let (sub_node, get_node, livelinessget_node, livelinesssub_node) = {
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

                    let mut get_node = base.clone();
                    get_node.name = format!("Get {node2_mode}");
                    get_node.con_task = ConcurrentTask::from([SequentialTask::from([
                        Task::Get(ke_getqueryable, msg_size),
                        Task::Checkpoint,
                    ])]);
                    get_node.warmup += Duration::from_millis(rng.gen_range(0..500));

                    let mut livelinessget_node = base.clone();
                    livelinessget_node.name = format!("LivelinessGet {node2_mode}");
                    livelinessget_node.con_task = ConcurrentTask::from([SequentialTask::from([
                        Task::LivelinessGet(ke_getliveliness),
                        Task::Checkpoint,
                    ])]);
                    livelinessget_node.warmup += Duration::from_millis(rng.gen_range(0..500));

                    let mut livelinesssub_node = base;
                    livelinesssub_node.name = format!("LivelinessSub {node2_mode}");
                    livelinesssub_node.con_task = ConcurrentTask::from([SequentialTask::from([
                        Task::LivelinessSub(ke_subliveliness),
                        Task::Checkpoint,
                    ])]);
                    livelinesssub_node.warmup += Duration::from_millis(rng.gen_range(0..500));

                    (sub_node, get_node, livelinessget_node, livelinesssub_node)
                };

                (
                    Recipe::new([router_node.clone(), pub_node, sub_node]),
                    Recipe::new([router_node.clone(), queryable_node, get_node]),
                    Recipe::new([router_node.clone(), liveliness_node, livelinessget_node]),
                    Recipe::new([router_node, livelinessloop_node, livelinesssub_node]),
                )
            },
        )
        .collect();

    for chunks in recipe_list.chunks(PARALLEL_RECIPES).map(|x| x.to_vec()) {
        let mut join_set = tokio::task::JoinSet::new();
        for (pubsub, getqueryable, getliveliness, subliveliness) in chunks {
            join_set.spawn(async move {
                pubsub.run().await?;
                getqueryable.run().await?;
                getliveliness.run().await?;
                subliveliness.run().await?;
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
    zenoh::init_log_from_env_or("error");

    #[derive(Clone, Copy)]
    struct IsFirstListen(bool);

    let modes = [
        (WhatAmI::Client, WhatAmI::Peer, IsFirstListen(false)),
        (WhatAmI::Peer, WhatAmI::Client, IsFirstListen(true)),
        (WhatAmI::Peer, WhatAmI::Peer, IsFirstListen(true)),
        (WhatAmI::Peer, WhatAmI::Peer, IsFirstListen(false)),
    ];

    let mut idx = 0;
    // Ports going to be used: 17501 to 17509
    let base_port = 17500;
    let recipe_list: Vec<_> = modes
        .into_iter()
        .flat_map(|(n1, n2, who)| MSG_SIZE.map(|s| (n1, n2, who, s)))
        .map(|(node1_mode, node2_mode, who, msg_size)| {
            idx += 1;
            let ke_pubsub = format!("two_node_combination_keyexpr_pubsub_{idx}");
            let ke_getqueryable = format!("two_node_combination_keyexpr_getqueryable_{idx}");
            let ke_subliveliness = format!("two_node_combination_keyexpr_subliveliness_{idx}");
            let ke_getliveliness = format!("two_node_combination_keyexpr_getliveliness_{idx}");

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

            let (pub_node, queryable_node, liveliness_node, livelinessloop_node) = {
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

                let mut queryable_node = base.clone();
                queryable_node.name = format!("Queryable {node1_mode}");
                queryable_node.con_task =
                    ConcurrentTask::from([SequentialTask::from([Task::Queryable(
                        ke_getqueryable.clone(),
                        msg_size,
                    )])]);

                let mut liveliness_node = base.clone();
                liveliness_node.name = format!("Liveliness {node1_mode}");
                liveliness_node.con_task =
                    ConcurrentTask::from([SequentialTask::from([Task::Liveliness(
                        ke_getliveliness.clone(),
                    )])]);

                let mut livelinessloop_node = base;
                livelinessloop_node.name = format!("LivelinessLoop {node1_mode}");
                livelinessloop_node.con_task =
                    ConcurrentTask::from([SequentialTask::from([Task::LivelinessLoop(
                        ke_subliveliness.clone(),
                    )])]);

                (
                    pub_node,
                    queryable_node,
                    liveliness_node,
                    livelinessloop_node,
                )
            };

            let (sub_node, get_node, livelinessget_node, livelinesssub_node) = {
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

                let mut get_node = base.clone();
                get_node.name = format!("Get {node2_mode}");
                get_node.con_task = ConcurrentTask::from([SequentialTask::from([
                    Task::Get(ke_getqueryable, msg_size),
                    Task::Checkpoint,
                ])]);

                let mut livelinessget_node = base.clone();
                livelinessget_node.name = format!("LivelinessGet {node2_mode}");
                livelinessget_node.con_task = ConcurrentTask::from([SequentialTask::from([
                    Task::LivelinessGet(ke_getliveliness),
                    Task::Checkpoint,
                ])]);

                let mut livelinesssub_node = base;
                livelinesssub_node.name = format!("LivelinessSub {node2_mode}");
                livelinesssub_node.con_task = ConcurrentTask::from([SequentialTask::from([
                    Task::LivelinessSub(ke_subliveliness),
                    Task::Checkpoint,
                ])]);

                (sub_node, get_node, livelinessget_node, livelinesssub_node)
            };

            (
                Recipe::new([pub_node, sub_node]),
                Recipe::new([queryable_node, get_node]),
                Recipe::new([liveliness_node, livelinessget_node]),
                Recipe::new([livelinessloop_node, livelinesssub_node]),
            )
        })
        .collect();

    for chunks in recipe_list.chunks(PARALLEL_RECIPES).map(|x| x.to_vec()) {
        let task_tracker = TaskTracker::new();
        for (pubsub, getqueryable, getlivelienss, subliveliness) in chunks {
            task_tracker.spawn(async move {
                pubsub.run().await?;
                getqueryable.run().await?;
                getlivelienss.run().await?;
                subliveliness.run().await?;
                Result::Ok(())
            });
        }
        task_tracker.close();
        task_tracker.wait().await;
    }

    println!("Two-node combination test passed.");
    Result::Ok(())
}

#[ignore = "This is a flaky test"]
#[tokio::test(flavor = "multi_thread", worker_threads = 9)]
async fn three_node_combination_multicast() -> Result<()> {
    zenoh::try_init_log_from_env();
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
    // Ports going to be used: 18511 .. 18535
    let base_port = 18510;

    let recipe_list: Vec<_> = modes
        .map(|n1| modes.map(|n2| (n1, n2)))
        .concat()
        .into_iter()
        .flat_map(|(n1, n2)| [256].map(|s| (n1, n2, s)))
        .flat_map(|(n1, n2, s)| delay_in_secs.map(|d| (n1, n2, s, d)))
        .map(
            |(node1_mode, node2_mode, msg_size, (delay1, delay2, delay3))| {
                idx += 1;
                let unicast_locator = format!("tcp/127.0.0.1:{}", base_port + idx);
                let multicast_locator = format!("udp/224.0.0.1:{}", base_port + idx);

                let ke_pubsub = format!("three_node_combination_multicast_keyexpr_pubsub_{idx}");

                use rand::Rng;
                let mut rng = rand::thread_rng();

                let router_node = Node {
                    name: format!("Router {}", WhatAmI::Router),
                    mode: WhatAmI::Router,
                    listen: vec![unicast_locator.clone(), multicast_locator.clone()],
                    con_task: ConcurrentTask::from([SequentialTask::from([Task::Wait])]),
                    warmup: Duration::from_secs(delay1)
                        + Duration::from_millis(rng.gen_range(0..500)),
                    ..Default::default()
                };

                let pub_node = {
                    let base = match node1_mode {
                        WhatAmI::Client => Node {
                            mode: node1_mode,
                            connect: vec![unicast_locator.clone()],
                            warmup: Duration::from_secs(delay2),
                            ..Default::default()
                        },
                        _ => Node {
                            mode: node1_mode,
                            listen: vec![multicast_locator.clone()],
                            warmup: Duration::from_secs(delay2),
                            ..Default::default()
                        },
                    };

                    let mut pub_node = base.clone();
                    pub_node.name = format!("Pub {node1_mode}");
                    pub_node.con_task = ConcurrentTask::from([SequentialTask::from([Task::Pub(
                        ke_pubsub.clone(),
                        msg_size,
                    )])]);
                    pub_node.warmup += Duration::from_millis(rng.gen_range(0..500));
                    pub_node
                };

                let sub_node = {
                    let base = match node2_mode {
                        WhatAmI::Client => Node {
                            mode: node2_mode,
                            connect: vec![unicast_locator.clone()],
                            warmup: Duration::from_secs(delay3),
                            ..Default::default()
                        },
                        _ => Node {
                            mode: node2_mode,
                            listen: vec![multicast_locator.clone()],
                            warmup: Duration::from_secs(delay3),
                            ..Default::default()
                        },
                    };

                    let mut sub_node = base.clone();
                    sub_node.name = format!("Sub {node2_mode}");
                    sub_node.con_task = ConcurrentTask::from([SequentialTask::from([
                        Task::Sub(ke_pubsub, msg_size),
                        Task::Checkpoint,
                    ])]);
                    sub_node.warmup += Duration::from_millis(rng.gen_range(0..500));
                    sub_node
                };

                Recipe::new([router_node.clone(), pub_node, sub_node])
            },
        )
        .collect();

    for chunks in recipe_list.chunks(PARALLEL_RECIPES).map(|x| x.to_vec()) {
        let mut join_set = tokio::task::JoinSet::new();
        for pubsub in chunks {
            join_set.spawn(async move {
                pubsub.run().await?;
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

#[tokio::test(flavor = "multi_thread", worker_threads = 9)]
async fn peer_linkstate() -> Result<()> {
    zenoh_util::try_init_log_from_env();
    let delay_in_secs = [
        (0, 1, 2),
        (0, 2, 1),
        (1, 2, 0),
        (1, 0, 2),
        (2, 0, 1),
        (2, 1, 0),
    ];

    let mut idx = 0;
    // Ports going to be used: 17551 to 17598
    let base_port = 17550;

    let linkstate_config = || {
        let mut config = Config::default();
        config
            .routing
            .peer
            .set_mode(Some("linkstate".to_string()))
            .unwrap();
        config
            .scouting
            .gossip
            .set_autoconnect(Some(ModeDependentValue::Unique(WhatAmIMatcher::empty())))
            .unwrap();
        Some(config)
    };

    let recipe_list: Vec<_> = delay_in_secs
        .into_iter()
        .map(|d| (1024, d))
        .map(|(msg_size, (delay1, delay2, delay3))| {
            idx += 1;
            let locator = format!("tcp/127.0.0.1:{}", base_port + idx);

            let ke_pubsub = format!("peer_linkstate_keyexpr_pubsub_{idx}");
            let ke_getqueryable = format!("peer_linkstate_keyexpr_getqueryable_{idx}");
            let ke_subliveliness = format!("peer_linkstate_keyexpr_subliveliness_{idx}");
            let ke_getliveliness = format!("peer_linkstate_keyexpr_getliveliness_{idx}");

            use rand::Rng;
            let mut rng = rand::thread_rng();

            let dummy_node = Node {
                name: "Dummy Peer".to_string(),
                mode: WhatAmI::Peer,
                listen: vec![locator.clone()],
                con_task: ConcurrentTask::from([SequentialTask::from([Task::Wait])]),
                warmup: Duration::from_secs(delay1) + Duration::from_millis(rng.gen_range(0..500)),
                config: linkstate_config(),
                ..Default::default()
            };

            let (pub_node, queryable_node, liveliness_node, livelinessloop_node) = {
                let base = Node {
                    mode: WhatAmI::Peer,
                    connect: vec![locator.clone()],
                    warmup: Duration::from_secs(delay2),
                    config: linkstate_config(),
                    ..Default::default()
                };

                let mut pub_node = base.clone();
                pub_node.name = "Pub Peer".to_string();
                pub_node.con_task = ConcurrentTask::from([SequentialTask::from([Task::Pub(
                    ke_pubsub.clone(),
                    msg_size,
                )])]);
                pub_node.warmup += Duration::from_millis(rng.gen_range(0..500));

                let mut queryable_node = base.clone();
                queryable_node.name = "Queryable Peer".to_string();
                queryable_node.con_task =
                    ConcurrentTask::from([SequentialTask::from([Task::Queryable(
                        ke_getqueryable.clone(),
                        msg_size,
                    )])]);
                queryable_node.warmup += Duration::from_millis(rng.gen_range(0..500));

                let mut liveliness_node = base.clone();
                liveliness_node.name = "Liveliness Peer".to_string();
                liveliness_node.con_task =
                    ConcurrentTask::from([SequentialTask::from([Task::Liveliness(
                        ke_getliveliness.clone(),
                    )])]);
                liveliness_node.warmup += Duration::from_millis(rng.gen_range(0..500));

                let mut livelinessloop_node = base;
                livelinessloop_node.name = "LivelinessLoop Peer".to_string();
                livelinessloop_node.con_task =
                    ConcurrentTask::from([SequentialTask::from([Task::LivelinessLoop(
                        ke_subliveliness.clone(),
                    )])]);
                livelinessloop_node.warmup += Duration::from_millis(rng.gen_range(0..500));

                (
                    pub_node,
                    queryable_node,
                    liveliness_node,
                    livelinessloop_node,
                )
            };

            let (sub_node, get_node, livelinessget_node, livelinesssub_node) = {
                let base = Node {
                    mode: WhatAmI::Peer,
                    connect: vec![locator],
                    warmup: Duration::from_secs(delay3),
                    config: linkstate_config(),
                    ..Default::default()
                };

                let mut sub_node = base.clone();
                sub_node.name = "Sub Peer".to_string();
                sub_node.con_task = ConcurrentTask::from([SequentialTask::from([
                    Task::Sub(ke_pubsub, msg_size),
                    Task::Checkpoint,
                ])]);
                sub_node.warmup += Duration::from_millis(rng.gen_range(0..500));

                let mut get_node = base.clone();
                get_node.name = "Get Peer".to_string();
                get_node.con_task = ConcurrentTask::from([SequentialTask::from([
                    Task::Get(ke_getqueryable, msg_size),
                    Task::Checkpoint,
                ])]);
                get_node.warmup += Duration::from_millis(rng.gen_range(0..500));

                let mut livelinessget_node = base.clone();
                livelinessget_node.name = "LivelinessGet Peer".to_string();
                livelinessget_node.con_task = ConcurrentTask::from([SequentialTask::from([
                    Task::LivelinessGet(ke_getliveliness),
                    Task::Checkpoint,
                ])]);
                livelinessget_node.warmup += Duration::from_millis(rng.gen_range(0..500));

                let mut livelinesssub_node = base;
                livelinesssub_node.name = "LivelinessSub Peer".to_string();
                livelinesssub_node.con_task = ConcurrentTask::from([SequentialTask::from([
                    Task::LivelinessSub(ke_subliveliness),
                    Task::Checkpoint,
                ])]);
                livelinesssub_node.warmup += Duration::from_millis(rng.gen_range(0..500));

                (sub_node, get_node, livelinessget_node, livelinesssub_node)
            };

            (
                Recipe::new([dummy_node.clone(), pub_node, sub_node]),
                Recipe::new([dummy_node.clone(), queryable_node, get_node]),
                Recipe::new([dummy_node.clone(), liveliness_node, livelinessget_node]),
                Recipe::new([dummy_node, livelinessloop_node, livelinesssub_node]),
            )
        })
        .collect();

    for chunks in recipe_list.chunks(PARALLEL_RECIPES).map(|x| x.to_vec()) {
        let mut join_set = tokio::task::JoinSet::new();
        for (pubsub, getqueryable, getlivelienss, subliveliness) in chunks {
            join_set.spawn(async move {
                pubsub.run().await?;
                getqueryable.run().await?;
                getlivelienss.run().await?;
                subliveliness.run().await?;
                Result::Ok(())
            });
        }

        while let Some(res) = join_set.join_next().await {
            res??;
        }
    }

    println!("Peer linkstate test passed.");
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 9)]
async fn router_linkstate() -> Result<()> {
    zenoh_util::try_init_log_from_env();
    let delay_in_secs = [
        (0, 1, 2),
        (0, 2, 1),
        (1, 2, 0),
        (1, 0, 2),
        (2, 0, 1),
        (2, 1, 0),
    ];

    let mut idx = 0;
    // Ports going to be used: 17601 to 17648
    let base_port = 17600;

    let recipe_list: Vec<_> = delay_in_secs
        .into_iter()
        .map(|d| (1024, d))
        .map(|(msg_size, (delay1, delay2, delay3))| {
            idx += 1;
            let locator1 = format!("tcp/127.0.0.1:{}", base_port + (idx * 3));
            let locator2 = format!("tcp/127.0.0.1:{}", base_port + (idx * 3) + 1);
            let locator3 = format!("tcp/127.0.0.1:{}", base_port + (idx * 3) + 2);

            let ke_pubsub = format!("router_linkstate_keyexpr_pubsub_{idx}");
            let ke_getqueryable = format!("router_linkstate_keyexpr_getqueryable_{idx}");
            let ke_subliveliness = format!("router_linkstate_keyexpr_subliveliness_{idx}");
            let ke_getliveliness = format!("router_linkstate_keyexpr_getliveliness_{idx}");

            let router1_node = Node {
                name: "Router 1".to_string(),
                mode: WhatAmI::Router,
                listen: vec![locator1.clone()],
                con_task: ConcurrentTask::from([SequentialTask::from([Task::Wait])]),
                warmup: Duration::from_secs(delay1),
                ..Default::default()
            };

            let router2_node = Node {
                name: "Router 2".to_string(),
                mode: WhatAmI::Router,
                listen: vec![locator2.clone()],
                connect: vec![locator1.clone()],
                con_task: ConcurrentTask::from([SequentialTask::from([Task::Wait])]),
                warmup: Duration::from_secs(delay2),
                ..Default::default()
            };

            let router3_node = Node {
                name: "Router 3".to_string(),
                mode: WhatAmI::Router,
                listen: vec![locator3.clone()],
                connect: vec![locator1.clone()],
                con_task: ConcurrentTask::from([SequentialTask::from([Task::Wait])]),
                warmup: Duration::from_secs(delay3),
                ..Default::default()
            };

            let (pub_node, queryable_node, liveliness_node, livelinessloop_node) = {
                let base = Node {
                    mode: WhatAmI::Client,
                    connect: vec![locator1.clone()],
                    warmup: Duration::from_secs(delay1) + Duration::from_millis(500),
                    ..Default::default()
                };

                let mut pub_node = base.clone();
                pub_node.name = "Pub Client".to_string();
                pub_node.con_task = ConcurrentTask::from([SequentialTask::from([Task::Pub(
                    ke_pubsub.clone(),
                    msg_size,
                )])]);

                let mut queryable_node = base.clone();
                queryable_node.name = "Queryable Client".to_string();
                queryable_node.con_task =
                    ConcurrentTask::from([SequentialTask::from([Task::Queryable(
                        ke_getqueryable.clone(),
                        msg_size,
                    )])]);

                let mut liveliness_node = base.clone();
                liveliness_node.name = "Liveliness Client".to_string();
                liveliness_node.con_task =
                    ConcurrentTask::from([SequentialTask::from([Task::Liveliness(
                        ke_getliveliness.clone(),
                    )])]);

                let mut livelinessloop_node = base;
                livelinessloop_node.name = "LivelinessLoop Client".to_string();
                livelinessloop_node.con_task =
                    ConcurrentTask::from([SequentialTask::from([Task::LivelinessLoop(
                        ke_subliveliness.clone(),
                    )])]);

                (
                    pub_node,
                    queryable_node,
                    liveliness_node,
                    livelinessloop_node,
                )
            };

            let (sub_node, get_node, livelinessget_node, livelinesssub_node) = {
                let base = Node {
                    mode: WhatAmI::Client,
                    connect: vec![locator3],
                    warmup: Duration::from_secs(delay3) + Duration::from_millis(500),
                    ..Default::default()
                };

                let mut sub_node = base.clone();
                sub_node.name = "Sub Client".to_string();
                sub_node.con_task = ConcurrentTask::from([SequentialTask::from([
                    Task::Sub(ke_pubsub, msg_size),
                    Task::Checkpoint,
                ])]);

                let mut get_node = base.clone();
                get_node.name = "Get Client".to_string();
                get_node.con_task = ConcurrentTask::from([SequentialTask::from([
                    Task::Get(ke_getqueryable, msg_size),
                    Task::Checkpoint,
                ])]);

                let mut livelinessget_node = base.clone();
                livelinessget_node.name = "LivelinessGet Client".to_string();
                livelinessget_node.con_task = ConcurrentTask::from([SequentialTask::from([
                    Task::LivelinessGet(ke_getliveliness),
                    Task::Checkpoint,
                ])]);

                let mut livelinesssub_node = base;
                livelinesssub_node.name = "LivelinessSub Client".to_string();
                livelinesssub_node.con_task = ConcurrentTask::from([SequentialTask::from([
                    Task::LivelinessSub(ke_subliveliness),
                    Task::Checkpoint,
                ])]);

                (sub_node, get_node, livelinessget_node, livelinesssub_node)
            };

            (
                Recipe::new([
                    router1_node.clone(),
                    router2_node.clone(),
                    router3_node.clone(),
                    pub_node,
                    sub_node,
                ]),
                Recipe::new([
                    router1_node.clone(),
                    router2_node.clone(),
                    router3_node.clone(),
                    queryable_node,
                    get_node,
                ]),
                Recipe::new([
                    router1_node.clone(),
                    router2_node.clone(),
                    router3_node.clone(),
                    liveliness_node,
                    livelinessget_node,
                ]),
                Recipe::new([
                    router1_node,
                    router2_node,
                    router3_node,
                    livelinessloop_node,
                    livelinesssub_node,
                ]),
            )
        })
        .collect();

    for chunks in recipe_list.chunks(1).map(|x| x.to_vec()) {
        let mut join_set = tokio::task::JoinSet::new();
        for (pubsub, getqueryable, getlivelienss, subliveliness) in chunks {
            join_set.spawn(async move {
                pubsub.run().await?;
                getqueryable.run().await?;
                getlivelienss.run().await?;
                subliveliness.run().await?;
                Result::Ok(())
            });
        }

        while let Some(res) = join_set.join_next().await {
            res??;
        }
    }

    println!("Router linkstate test passed.");
    Ok(())
}
