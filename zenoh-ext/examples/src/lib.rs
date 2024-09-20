//! Examples on using Zenoh.
//! See the code in ../examples/
//! Check ../README.md for usage.
//!
use zenoh::{config::WhatAmI, Config};

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
            Some(Wai::Peer) => config.set_mode(Some(WhatAmI::Peer)),
            Some(Wai::Client) => config.set_mode(Some(WhatAmI::Client)),
            Some(Wai::Router) => config.set_mode(Some(WhatAmI::Router)),
            None => Ok(None),
        }
        .unwrap();
        if !value.connect.is_empty() {
            config
                .connect
                .endpoints
                .set(value.connect.iter().map(|v| v.parse().unwrap()).collect())
                .unwrap();
        }
        if !value.listen.is_empty() {
            config
                .listen
                .endpoints
                .set(value.listen.iter().map(|v| v.parse().unwrap()).collect())
                .unwrap();
        }
        config
    }
}
