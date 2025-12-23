//! Examples on using Zenoh.
//! See the code in ../examples/
//! Check ../README.md for usage.
//!

use serde_json::json;
use zenoh::{config::WhatAmI, Config};

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
    mode: Option<WhatAmI>,
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
        if let Some(mode) = args.mode {
            config
                .insert_json5("mode", &json!(mode.to_str()).to_string())
                .unwrap();
        }

        if !args.connect.is_empty() {
            config
                .insert_json5("connect/endpoints", &json!(args.connect).to_string())
                .unwrap();
        }
        if !args.listen.is_empty() {
            config
                .insert_json5("listen/endpoints", &json!(args.listen).to_string())
                .unwrap();
        }
        if args.no_multicast_scouting {
            config
                .insert_json5("scouting/multicast/enabled", &json!(false).to_string())
                .unwrap();
        }
        if args.enable_shm {
            #[cfg(feature = "shared-memory")]
            config
                .insert_json5("transport/shared_memory/enabled", &json!(true).to_string())
                .unwrap();
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

#[cfg(all(feature = "shared-memory", feature = "unstable"))]
pub mod shm {
    pub fn print_sample_info(mut sample: zenoh::sample::Sample) {
        let kind = sample.kind();
        let key_str = sample.key_expr().as_str().to_owned();

        // Print overall payload information
        let (payload_type, payload) = handle_bytes(sample.payload_mut());
        print!(">> [Subscriber] Received {kind} ('{key_str}': '{payload}') [{payload_type}] ",);

        // Print attachment information
        if let Some(att) = sample.attachment_mut() {
            let (attachment_type, attachment) = handle_bytes(att);
            print!(" ({attachment_type}: {attachment})");
        }

        println!();
    }

    fn handle_bytes(bytes: &mut zenoh::bytes::ZBytes) -> (&str, std::borrow::Cow<'_, str>) {
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

            // if Zenoh is built with SHM support and with SHM API we can detect the exact buffer type
            #[cfg(all(feature = "shared-memory", feature = "unstable"))]
            match bytes.as_shm_mut() {
                // try to mutate SHM buffer to get it's mutability property
                Some(shm) => match <&mut zenoh::shm::zshmmut>::try_from(shm) {
                    Ok(_shm_mut) => "SHM (MUT)",
                    Err(_) => "SHM (IMMUT)",
                },
                None => "RAW",
            }
        };

        // In order to indicate the real underlying buffer type the code above is written ^^^
        // Sample is SHM-agnostic: Sample handling code works both with SHM and RAW data transparently.
        // In other words, the common application compiled with "shared-memory" feature will be able to
        // handle incoming SHM data without any changes in the application code.
        //
        // Refer to z_bytes.rs to see how to deserialize different types of message
        let bytes_string = bytes
            .try_to_string()
            .unwrap_or_else(|e| e.to_string().into());

        (bytes_type, bytes_string)
    }
}
