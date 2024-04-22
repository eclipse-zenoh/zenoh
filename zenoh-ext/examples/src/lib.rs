// Copyright (c) 2024 ZettaScale Technology
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

//! Examples on using Zenoh.
//! See the code in ../examples/
//! Check ../README.md for usage.
//!
use zenoh::config::Config;

#[derive(clap::ValueEnum, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Wai {
    Peer,
    Client,
    Router,
}
impl core::fmt::Display for Wai {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        core::fmt::Debug::fmt(&self, f)
    }
}
#[derive(clap::Parser, Clone, PartialEq, Eq, Hash, Debug)]
pub struct CommonArgs {
    #[arg(short, long)]
    /// A configuration file.
    config: Option<String>,
    #[arg(short, long)]
    /// The Zenoh session mode [default: peer].
    mode: Option<Wai>,
    #[arg(short = 'e', long)]
    /// Endpoints to connect to.
    connect: Vec<String>,
    #[arg(short, long)]
    /// Endpoints to listen on.
    listen: Vec<String>,
}

impl From<CommonArgs> for Config {
    fn from(value: CommonArgs) -> Self {
        (&value).into()
    }
}
impl From<&CommonArgs> for Config {
    fn from(value: &CommonArgs) -> Self {
        let mut config = match &value.config {
            Some(path) => Config::from_file(path).unwrap(),
            None => Config::default(),
        };
        match value.mode {
            Some(Wai::Peer) => config.set_mode(Some(zenoh::scouting::WhatAmI::Peer)),
            Some(Wai::Client) => config.set_mode(Some(zenoh::scouting::WhatAmI::Client)),
            Some(Wai::Router) => config.set_mode(Some(zenoh::scouting::WhatAmI::Router)),
            None => Ok(None),
        }
        .unwrap();
        if !value.connect.is_empty() {
            config.connect.endpoints = value.connect.iter().map(|v| v.parse().unwrap()).collect();
        }
        if !value.listen.is_empty() {
            config.listen.endpoints = value.listen.iter().map(|v| v.parse().unwrap()).collect();
        }
        config
    }
}
