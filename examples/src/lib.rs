//! Examples on using Zenoh.
//! See the code in ../examples/
//! Check ../README.md for usage.
//!
#[cfg(all(feature = "shared-memory", feature = "unstable"))]
use zenoh::shm::zshm;
use zenoh::{bytes::ZBytes, config::Config, query::Query, sample::Sample};

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
    #[arg(long)]
    /// Disable the multicast-based scouting mechanism.
    no_multicast_scouting: bool,
    #[arg(long)]
    /// Disable the multicast-based scouting mechanism.
    enable_shm: bool,
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
            Some(Wai::Peer) => config.set_mode(Some(zenoh::config::WhatAmI::Peer)),
            Some(Wai::Client) => config.set_mode(Some(zenoh::config::WhatAmI::Client)),
            Some(Wai::Router) => config.set_mode(Some(zenoh::config::WhatAmI::Router)),
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
        if value.no_multicast_scouting {
            config.scouting.multicast.set_enabled(Some(false)).unwrap();
        }
        if value.enable_shm {
            #[cfg(feature = "shared-memory")]
            config.transport.shared_memory.set_enabled(true).unwrap();
            #[cfg(not(feature = "shared-memory"))]
            {
                println!("enable-shm argument: SHM cannot be enabled, because Zenoh is compiled without shared-memory feature!");
                std::process::exit(-1);
            }
        }
        config
    }
}

pub fn receive_query(query: &Query, entity_name: &str) {
    // Print overall payload information
    match query.payload() {
        Some(payload) => {
            let (payload_type, payload) = handle_bytes(payload);
            print!(
                "{} >> [{}] Received Query ('{}': '{}')",
                entity_name,
                payload_type,
                query.selector(),
                payload
            );
        }
        None => {
            print!("{} >> Received Query '{}'", entity_name, query.selector());
        }
    };

    // Print attachment information
    print_attachment(query.attachment());

    println!();
}

pub fn receive_sample(sample: &Sample, entity_name: &str) {
    // Print overall payload information
    let (payload_type, payload) = handle_bytes(sample.payload());
    print!(
        "{} >> [{}] Received {} ('{}': '{}')",
        entity_name,
        payload_type,
        sample.kind(),
        sample.key_expr().as_str(),
        payload
    );

    // Print attachment information
    print_attachment(sample.attachment());

    println!();
}

fn print_attachment(attachment: Option<&ZBytes>) {
    if let Some(att) = attachment {
        let (attachment_type, attachment) = handle_bytes(att);
        print!(" ({}: {})", attachment_type, attachment);
    }
}

fn handle_bytes(bytes: &ZBytes) -> (&str, String) {
    // Determine buffer type for indication purpose
    let bytes_type = {
        // if Zenoh is built without SHM support, the only buffer type it can receive is RAW
        #[cfg(not(feature = "shared-memory"))]
        {
            "RAW"
        }

        // if Zenoh is built with SHM support but without SHM API (that is unstable), it can
        // receive buffers of any type, but there is no way to detect the buffer type
        #[cfg(all(feature = "shared-memory", not(feature = "unstable")))]
        {
            "UNKNOWN"
        }

        // if Zenoh is built with SHM support and with SHM API  we can detect the exact buffer type
        #[cfg(all(feature = "shared-memory", feature = "unstable"))]
        match bytes.deserialize::<&zshm>() {
            Ok(_) => "SHM",
            Err(_) => "RAW",
        }
    };

    // In order to indicate the real underlying buffer type the code above is written ^^^
    // Sample is SHM-agnostic: Sample handling code works both with SHM and RAW data transparently.
    // In other words, the common application compiled with "shared-memory" feature will be able to
    // handle incoming SHM data without any changes in the application code.
    //
    // Refer to z_bytes.rs to see how to deserialize different types of message
    let bytes_string = bytes
        .deserialize::<String>()
        .unwrap_or_else(|e| format!("{}", e));

    (bytes_type, bytes_string)
}
