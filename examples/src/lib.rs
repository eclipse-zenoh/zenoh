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
    #[arg(long)]
    /// Allows arbitrary configuration changes as column-separated KEY:VALUE pairs, where:
    ///   - KEY must be a valid config path.
    ///   - VALUE must be a valid JSON5 string that can be deserialized to the expected type for the KEY field.
    ///
    /// Example: `--cfg='transport/unicast/max_links:2'`
    #[arg(long)]
    cfg: Vec<String>,
    #[arg(short, long)]
    /// The Zenoh session mode [default: peer].
    mode: Option<Wai>,
    #[arg(short = 'e', long)]
    /// Endpoints to connect to.
    connect: Vec<String>,
    #[arg(short, long)]
    /// Endpoints to listen on.
    listen: Vec<String>,
    #[arg(long)]
    /// Disable the multicast-based scouting mechanism.
    no_multicast_scouting: bool,
    #[arg(long)]
    /// Enable shared-memory feature.
    enable_shm: bool,
}

impl From<CommonArgs> for Config {
    fn from(value: CommonArgs) -> Self {
        (&value).into()
    }
}

impl From<&CommonArgs> for Config {
    fn from(args: &CommonArgs) -> Self {
        let mut config = match &args.config {
            Some(path) => Config::from_file(path).unwrap(),
            None => Config::default(),
        };
        match args.mode {
            Some(Wai::Peer) => config.set_mode(Some(WhatAmI::Peer)),
            Some(Wai::Client) => config.set_mode(Some(WhatAmI::Client)),
            Some(Wai::Router) => config.set_mode(Some(WhatAmI::Router)),
            None => Ok(None),
        }
        .unwrap();
        if !args.connect.is_empty() {
            config
                .connect
                .endpoints
                .set(args.connect.iter().map(|v| v.parse().unwrap()).collect())
                .unwrap();
        }
        if !args.listen.is_empty() {
            config
                .listen
                .endpoints
                .set(args.listen.iter().map(|v| v.parse().unwrap()).collect())
                .unwrap();
        }
        if args.no_multicast_scouting {
            config.scouting.multicast.set_enabled(Some(false)).unwrap();
        }
        if args.enable_shm {
            #[cfg(feature = "shared-memory")]
            config.transport.shared_memory.set_enabled(true).unwrap();
            #[cfg(not(feature = "shared-memory"))]
            {
                eprintln!("`--enable-shm` argument: SHM cannot be enabled, because Zenoh is compiled without shared-memory feature!");
                std::process::exit(-1);
            }
        }
        for json in &args.cfg {
            if let Some((key, value)) = json.split_once(':') {
                if let Err(err) = config.insert_json5(key, value) {
                    eprintln!("`--cfg` argument: could not parse `{json}`: {err}");
                    std::process::exit(-1);
                }
            } else {
                eprintln!("`--cfg` argument: expected KEY:VALUE pair, got {json}");
                std::process::exit(-1);
            }
        }
        config
    }
}
