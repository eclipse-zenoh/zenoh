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
use std::sync::Arc;
use std::time::Duration;

use async_std::prelude::FutureExt;
use zenoh::config::{whatami::WhatAmI, Config};
use zenoh::prelude::r#async::*;
use zenoh::Result;
use zenoh_result::bail;

const TIMEOUT: Duration = Duration::from_secs(10);

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
    EditConnection(Vec<String>),
}

#[derive(Debug)]
struct Node {
    name: String,
    mode: WhatAmI,
    listen: Vec<String>,
    connect: Vec<String>,
    task: Vec<Task>,
}

impl Default for Node {
    fn default() -> Self {
        Self {
            name: "TestNode".into(),
            mode: WhatAmI::Peer,
            listen: vec![],
            connect: vec![],
            task: vec![],
        }
    }
}

async fn run_recipe(receipe: impl IntoIterator<Item = Node>) -> Result<()> {
    let futures = receipe.into_iter().map(|node| async move {
        dbg!(node.name);

        // Load the config and build up a session
        let mut config = Config::default();
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
            // Sleep one second and retry
            async_std::task::sleep(Duration::from_secs(1)).await;
            zenoh::open(config).res_async().await?.into_arc()
        };

        // Each node consists of a specified session associated with tasks to run
        let futs = node.task.into_iter().map(|task| {

            // Each session spawns several given task(s)
            let c_session = session.clone();

            match task {

                // Subscription task
                Task::Sub(topic, expected_size) => {
                    async_std::task::spawn(async move {
                        let sub =
                            ztimeout!(c_session.declare_subscriber(&topic).res_async())?;
                        let mut counter = 0;
                        while let Ok(sample) = sub.recv_async().await {
                            let recv_size = sample.value.payload.len();
                            if recv_size != expected_size {
                                bail!("Received payload size {recv_size} mismatches the expected {expected_size}");
                            }
                            counter += 1;
                            println!("Received : {:?}", sample);
                            if counter >= 5 {
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
                            async_std::task::sleep(Duration::from_millis(300)).await;
                            ztimeout!(c_session
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

                // Edit the connections after a while
                Task::EditConnection(locators) => async_std::task::spawn(async move {
                    todo!()
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
        let locator = String::from("tcp/127.0.0.1:17448");
        let topic = String::from("testTopic");

        // Gossip test
        let recipe = [
            Node {
                name: "C".into(),
                mode: WhatAmI::Peer,
                listen: vec![locator.clone()],
                task: vec![Task::Sleep(Duration::from_secs(30))],
                ..Default::default()
            },
            Node {
                name: "A".into(),
                connect: vec![locator.clone()],
                mode: WhatAmI::Peer,
                task: vec![Task::Pub(topic.clone(), 8)],
                ..Default::default()
            },
            Node {
                name: "B".into(),
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
fn failover() -> Result<()> {
    todo!()
}
