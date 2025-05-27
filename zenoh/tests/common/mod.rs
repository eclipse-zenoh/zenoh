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
#![allow(dead_code)] // because every test doesn't use the whole common features
use std::{
    future::{ready, IntoFuture, Ready},
    net::TcpListener,
    ops::{Deref, DerefMut},
};

use serde::Serialize;
use zenoh::{
    config::{Locator, WhatAmI},
    Resolvable, Session, Wait,
};

pub fn open_mode(mode: WhatAmI) -> TestOpenBuilder {
    TestOpenBuilder::new(mode)
}

pub fn open_client() -> TestOpenBuilder {
    open_mode(WhatAmI::Client)
}

pub fn open_peer() -> TestOpenBuilder {
    open_mode(WhatAmI::Peer)
}

pub fn open_router() -> TestOpenBuilder {
    open_mode(WhatAmI::Router)
}

#[derive(Debug)]
pub struct TestOpenBuilder(zenoh_config::Config);

impl TestOpenBuilder {
    pub fn new(mode: WhatAmI) -> Self {
        let mut config = zenoh_config::Config::default();
        config.set_mode(Some(mode)).unwrap();
        config.scouting.multicast.set_enabled(Some(false)).unwrap();
        if mode != WhatAmI::Client {
            let endpoints = vec!["tcp/127.0.0.1:0".parse().unwrap()];
            config.listen.endpoints.set(endpoints).unwrap();
        }
        Self(config)
    }

    pub fn with_json5(mut self, key: &str, value: &str) -> Self {
        self.insert_json5(key, value).unwrap();
        self
    }

    pub fn with(self, key: &str, value: impl Serialize) -> Self {
        self.with_json5(key, &serde_json::to_string(&value).unwrap())
    }

    pub fn connect_to(self, to: impl ConnectTo) -> Self {
        self.with("connect/endpoints", to.locators())
    }

    pub fn connect_to_protocol(self, to: impl ConnectTo, protocol: &str) -> Self {
        self.with(
            "connect/endpoints",
            to.locators()
                .into_iter()
                .filter(|loc| loc.protocol().as_str() == protocol)
                .collect::<Vec<_>>(),
        )
    }
}

impl Clone for TestOpenBuilder {
    fn clone(&self) -> Self {
        let mut cfg = self.0.clone();
        cfg.set_id(Default::default()).unwrap();
        Self(cfg)
    }
}

impl Deref for TestOpenBuilder {
    type Target = zenoh_config::Config;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for TestOpenBuilder {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Resolvable for TestOpenBuilder {
    type To = Session;
}

impl IntoFuture for TestOpenBuilder {
    type Output = Session;
    type IntoFuture = Ready<Session>;

    fn into_future(self) -> Self::IntoFuture {
        ready(zenoh::open(self.0).wait().unwrap())
    }
}

impl Wait for TestOpenBuilder {
    fn wait(self) -> Self::To {
        zenoh::open(self.0).wait().unwrap()
    }
}

pub struct TcpForward {
    connect: Locator,
    listener: TcpListener,
    task: Option<tokio::task::JoinHandle<()>>,
}

impl TcpForward {
    pub fn with_locator(connect: Locator) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let mut this = Self {
            connect,
            listener,
            task: None,
        };
        this.enable();
        this
    }

    pub fn connect(to: &Session) -> Self {
        Self::with_locator(to.info().locators().wait()[0].clone())
    }

    pub fn locator(&self) -> Locator {
        format!("tcp/{}", self.listener.local_addr().unwrap())
            .parse()
            .unwrap()
    }

    pub fn enable(&mut self) {
        if self.task.is_some() {
            return;
        }
        let listener =
            tokio::net::TcpListener::from_std(self.listener.try_clone().unwrap()).unwrap();
        let connect = self
            .connect
            .as_str()
            .strip_prefix("tcp/")
            .unwrap()
            .to_string();
        self.task = Some(tokio::spawn(async move {
            loop {
                let (mut listen_stream, _) = listener.accept().await.unwrap();
                let mut connect_stream = tokio::net::TcpStream::connect(&connect).await.unwrap();
                tokio::io::copy_bidirectional(&mut listen_stream, &mut connect_stream)
                    .await
                    .unwrap();
            }
        }))
    }

    pub fn disable(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

pub trait ConnectTo {
    fn locators(self) -> Vec<Locator>;
}

impl ConnectTo for &Session {
    fn locators(self) -> Vec<Locator> {
        self.info().locators().wait()
    }
}

impl ConnectTo for &TcpForward {
    fn locators(self) -> Vec<Locator> {
        [self.locator()].into()
    }
}

impl<C: ConnectTo, const N: usize> ConnectTo for [C; N] {
    fn locators(self) -> Vec<Locator> {
        self.into_iter().flat_map(ConnectTo::locators).collect()
    }
}
