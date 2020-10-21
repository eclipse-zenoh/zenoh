//
// Copyright (c) 2017, 2020 ADLINK Technology Inc.
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ADLINK zenoh team, <zenoh@adlink-labs.tech>
//
use crate::routing::broker::Broker;
use crate::runtime::orchestrator::SessionOrchestrator;
use async_std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};
use config::*;
use prelude::*;
use uhlc::HLC;
use zenoh_protocol::core::{PeerId, Properties};
use zenoh_protocol::session::{SessionManager, SessionManagerConfig, SessionManagerOptionalConfig};
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::{zerror, zerror2};

pub mod orchestrator;

mod adminspace;
pub use adminspace::AdminSpace;

pub struct RuntimeState {
    pub pid: PeerId,
    pub broker: Arc<Broker>,
    pub orchestrator: SessionOrchestrator,
}

#[derive(Clone)]
pub struct Runtime {
    state: Arc<RwLock<RuntimeState>>,
}

impl Runtime {
    pub async fn new(version: u8, config: Properties, id: Option<&str>) -> ZResult<Runtime> {
        let pid = if let Some(s) = id {
            let vec = hex::decode(s).map_err(|e| {
                zerror2!(ZErrorKind::Other {
                    descr: format!("Invalid id: {} - {}", s, e)
                })
            })?;
            let size = vec.len();
            if size > PeerId::MAX_SIZE {
                return zerror!(ZErrorKind::Other {
                    descr: format!("Invalid id size: {} ({} bytes max)", size, PeerId::MAX_SIZE)
                });
            }
            let mut id = [0u8; PeerId::MAX_SIZE];
            id[..size].copy_from_slice(vec.as_slice());
            PeerId::new(size, id)
        } else {
            PeerId::from(uuid::Uuid::new_v4())
        };

        log::debug!("Using PID: {}", pid);

        let whatami = parse_mode(config.last_or(ZN_MODE_KEY, ZN_MODE_DEFAULT)).unwrap();
        let hlc = if config
            .last_or(ZN_ADD_TIMESTAMP_KEY, ZN_ADD_TIMESTAMP_DEFAULT)
            .is_true()
        {
            Some(HLC::with_system_time(uhlc::ID::from(&pid)))
        } else {
            None
        };
        let broker = Arc::new(Broker::new(hlc));

        let sm_config = SessionManagerConfig {
            version,
            whatami,
            id: pid.clone(),
            handler: broker.clone(),
        };

        let sm_opt_config = SessionManagerOptionalConfig {
            lease: None,
            keep_alive: None,
            sn_resolution: None,
            batch_size: None,
            timeout: None,
            retries: None,
            max_sessions: None,
            max_links: None,
        };

        let session_manager = SessionManager::new(sm_config, Some(sm_opt_config));
        let mut orchestrator = SessionOrchestrator::new(session_manager, whatami);
        match orchestrator.init(config).await {
            Ok(()) => Ok(Runtime {
                state: Arc::new(RwLock::new(RuntimeState {
                    pid,
                    broker,
                    orchestrator,
                })),
            }),
            Err(err) => Err(err),
        }
    }

    pub async fn read(&self) -> RwLockReadGuard<'_, RuntimeState> {
        self.state.read().await
    }

    pub async fn write(&self) -> RwLockWriteGuard<'_, RuntimeState> {
        self.state.write().await
    }

    pub async fn close(&self) -> ZResult<()> {
        self.write().await.orchestrator.close().await
    }

    pub async fn get_pid_str(&self) -> String {
        self.read().await.pid.to_string()
    }
}

/// Constants and helpers to build the configuration [Properties](Properties)
/// to pass to [open](../fn.open.html).
pub mod config {
    use zenoh_protocol::core::{Properties, ZInt};

    /// `"true"`
    pub const ZN_TRUE: &[u8] = b"true";
    /// `"false"`
    pub const ZN_FALSE: &[u8] = b"false";

    /// The library mode.
    /// Accepted values : `"peer"`, `"client"`.
    /// Default value : `"peer"`.
    pub const ZN_MODE_KEY: ZInt = 0x50;
    pub const ZN_MODE_DEFAULT: &[u8] = b"peer";

    /// The locator of a peer to connect to.
    /// Accepted values : `<locator>` (ex: `"tcp/10.10.10.10:7447"`).
    /// Default value : None.
    /// Multiple values accepted.
    pub const ZN_PEER_KEY: ZInt = 0x51;

    /// A locator to listen on.
    /// Accepted values : `<locator>` (ex: `"tcp/10.10.10.10:7447"`).
    /// Default value : None.
    /// Multiple values accepted.
    pub const ZN_LISTENER_KEY: ZInt = 0x52;

    /// The user name to use for authentication.
    /// Accepted values : `<string>`.
    /// Default value : None.
    pub const ZN_USER_KEY: ZInt = 0x53;

    /// The password to use for authentication.
    /// Accepted values : `<string>`.
    /// Default value : None.
    pub const ZN_PASSWORD_KEY: ZInt = 0x54;

    /// Activates/Desactivates multicast scouting.
    /// Accepted values : `"true"`, `"false"`.
    /// Default value : `"true"`.
    pub const ZN_MULTICAST_SCOUTING_KEY: ZInt = 0x55;
    pub const ZN_MULTICAST_SCOUTING_DEFAULT: &[u8] = b"true";

    /// The network interface to use for multicast scouting.
    /// Accepted values : `"auto"`, `<ip address>`, `<interface name>`.
    /// Default value : `"auto"`.
    pub const ZN_MULTICAST_INTERFACE_KEY: ZInt = 0x56;
    pub const ZN_MULTICAST_INTERFACE_DEFAULT: &[u8] = b"auto";

    /// The multicast address and ports to use for multicast scouting.
    /// Accepted values : `<ip address>:<port>`.
    /// Default value : `"224.0.0.224:7447"`.
    pub const ZN_MULTICAST_ADDRESS_KEY: ZInt = 0x57;
    pub const ZN_MULTICAST_ADDRESS_DEFAULT: &[u8] = b"224.0.0.224:7447";

    /// In client mode, the period dedicated to scouting a router before failing.
    /// Accepted values : `<float in seconds>`.
    /// Default value : `"3.0"`.
    pub const ZN_SCOUTING_TIMEOUT_KEY: ZInt = 0x58;
    pub const ZN_SCOUTING_TIMEOUT_DEFAULT: &[u8] = b"3.0";

    /// In peer mode, the period dedicated to scouting first remote peers before doing anything else.
    /// Accepted values : `<float in seconds>`.
    /// Default value : `"0.2"`.
    pub const ZN_SCOUTING_DELAY_KEY: ZInt = 0x59;
    pub const ZN_SCOUTING_DELAY_DEFAULT: &[u8] = b"0.2";

    /// Indicates if data messages should be timestamped.
    /// Accepted values : `"true"`, `"false"`.
    /// Default value : `"false"`.
    pub const ZN_ADD_TIMESTAMP_KEY: ZInt = 0x5A;
    pub const ZN_ADD_TIMESTAMP_DEFAULT: &[u8] = b"false";

    /// Indicates if local writes/queries should reach local subscribers/queryables.
    /// Accepted values : `"true"`, `"false"`.
    /// Default value : `"true"`.
    pub const ZN_LOCAL_ROUTING_KEY: ZInt = 0x5B;
    pub const ZN_LOCAL_ROUTING_DEFAULT: &[u8] = b"true";

    /// Creates an empty zenoh net Session configuration.
    pub fn empty() -> Properties {
        vec![]
    }

    /// Creates a default zenoh net Session configuration.
    ///
    /// The returned configuration contains :
    ///  - `(ZN_MODE_KEY, "peer")`
    pub fn default() -> Properties {
        peer()
    }

    /// Creates a default `'peer'` mode zenoh net Session configuration.
    ///
    /// The returned configuration contains :
    ///  - `(ZN_MODE_KEY, "peer")`
    pub fn peer() -> Properties {
        vec![(ZN_MODE_KEY, b"peer".to_vec())]
    }

    /// Creates a default `'client'` mode zenoh net Session configuration.
    ///
    /// The returned configuration contains :
    ///  - `(ZN_MODE_KEY, "client")`
    ///
    /// If the given peer locator is not `None`, the returned configuration also contains :
    ///  - `(ZN_PEER_KEY, <peer>)`
    pub fn client(peer: Option<String>) -> Properties {
        let mut result = vec![(ZN_MODE_KEY, b"client".to_vec())];
        if let Some(peer) = peer {
            result.push((ZN_PEER_KEY, peer.as_bytes().to_vec()));
        }
        result
    }

    pub fn key_to_string(key: ZInt) -> String {
        match key {
            0x50 => "ZN_MODE_KEY".to_string(),
            0x51 => "ZN_PEER_KEY".to_string(),
            0x52 => "ZN_LISTENER_KEY".to_string(),
            0x53 => "ZN_USER_KEY".to_string(),
            0x54 => "ZN_PASSWORD_KEY".to_string(),
            0x55 => "ZN_MULTICAST_SCOUTING_KEY".to_string(),
            0x56 => "ZN_MULTICAST_INTERFACE_KEY".to_string(),
            0x57 => "ZN_MULTICAST_ADDRESS_KEY".to_string(),
            0x58 => "ZN_SCOUTING_TIMEOUT_KEY".to_string(),
            0x59 => "ZN_SCOUTING_DELAY_KEY".to_string(),
            0x5A => "ZN_ADD_TIMESTAMP_KEY".to_string(),
            0x5B => "ZN_LOCAL_ROUTING_KEY".to_string(),
            key => key.to_string(),
        }
    }

    pub fn to_string(config: &[(u64, Vec<u8>)]) -> String {
        format!(
            "[{}]",
            config
                .iter()
                .map(|(k, v)| {
                    format!("({}, {})", key_to_string(*k), String::from_utf8_lossy(v))
                })
                .collect::<Vec<String>>()
                .join(", ")
        )
    }
}

pub mod prelude {
    use zenoh_protocol::core::{whatami, Properties, ZInt};

    pub fn parse_mode(m: &[u8]) -> Result<whatami::Type, ()> {
        match m {
            b"peer" => Ok(whatami::PEER),
            b"client" => Ok(whatami::CLIENT),
            b"router" => Ok(whatami::ROUTER),
            _ => Err(()),
        }
    }

    pub trait PropertyValue {
        fn is_true(&self) -> bool;
    }

    impl PropertyValue for [u8] {
        #[inline]
        fn is_true(&self) -> bool {
            String::from_utf8_lossy(self).to_lowercase() == "true"
        }
    }

    pub struct PropertiesFilter<'a> {
        iter: std::slice::Iter<'a, (u64, Vec<u8>)>,
        key: ZInt,
    }

    impl<'a> Iterator for PropertiesFilter<'a> {
        type Item = &'a [u8];

        fn next(&mut self) -> Option<&'a [u8]> {
            loop {
                match &self.iter.next() {
                    Some((k, v)) => {
                        if *k == self.key {
                            return Some(&v[..]);
                        } else {
                            continue;
                        }
                    }
                    _ => return None,
                }
            }
        }
    }

    pub trait PropertiesUtils {
        fn get(&self, key: ZInt) -> PropertiesFilter;
        #[inline]
        fn last(&self, key: ZInt) -> Option<&[u8]> {
            self.get(key).last()
        }
        #[inline]
        fn last_or<'a>(&'a self, key: ZInt, default: &'a [u8]) -> &'a [u8] {
            self.last(key).or(Some(default)).unwrap()
        }
        fn last_or_str<'a>(&'a self, key: ZInt, default: &'a [u8]) -> &str {
            std::str::from_utf8(self.last(key).or(Some(default)).unwrap()).unwrap()
        }
    }

    impl PropertiesUtils for Properties {
        #[inline]
        fn get(&self, key: ZInt) -> PropertiesFilter {
            PropertiesFilter {
                iter: self.iter(),
                key,
            }
        }
    }
}
