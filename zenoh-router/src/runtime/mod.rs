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
use crate::routing::router::Router;
use crate::runtime::orchestrator::SessionOrchestrator;
use async_std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};
use config::*;
use uhlc::HLC;
use zenoh_protocol::core::PeerId;
use zenoh_protocol::session::{SessionManager, SessionManagerConfig, SessionManagerOptionalConfig};
use zenoh_util::collections::{IntKeyProperties, KeyTranscoder};
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::{zerror, zerror2};

pub mod orchestrator;

mod adminspace;
pub use adminspace::AdminSpace;

pub struct RuntimeState {
    pub pid: PeerId,
    pub router: Arc<Router>,
    pub orchestrator: SessionOrchestrator,
}

#[derive(Clone)]
pub struct Runtime {
    state: Arc<RwLock<RuntimeState>>,
}

impl Runtime {
    pub async fn new(version: u8, config: RuntimeProperties, id: Option<&str>) -> ZResult<Runtime> {
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

        log::info!("Using PID: {}", pid);

        let whatami = parse_mode(config.get_or(&ZN_MODE_KEY, ZN_MODE_DEFAULT)).unwrap();
        let hlc = if config
            .get_or(&ZN_ADD_TIMESTAMP_KEY, ZN_ADD_TIMESTAMP_DEFAULT)
            .to_lowercase()
            == ZN_TRUE
        {
            Some(HLC::with_system_time(uhlc::ID::from(&pid)))
        } else {
            None
        };
        let mut router = Arc::new(Router::new(pid.clone(), whatami, hlc));

        let sm_config = SessionManagerConfig {
            version,
            whatami,
            id: pid.clone(),
            handler: router.clone(),
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
        let peers_autuconnect = config
            .get_or(&ZN_PEERS_AUTOCONNECT_KEY, ZN_PEERS_AUTOCONNECT_DEFAULT)
            .to_lowercase()
            == ZN_TRUE;
        if config
            .get_or(&ZN_LINK_STATE_KEY, ZN_LINK_STATE_DEFAULT)
            .to_lowercase()
            == ZN_TRUE
        {
            unsafe {
                Arc::get_mut_unchecked(&mut router)
                    .init_link_state(orchestrator.clone(), peers_autuconnect)
                    .await;
            }
        }
        match orchestrator.init(config, peers_autuconnect).await {
            Ok(()) => Ok(Runtime {
                state: Arc::new(RwLock::new(RuntimeState {
                    pid,
                    router,
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

pub mod config {
    use zenoh_protocol::core::whatami;

    /// `"true"`
    pub const ZN_TRUE: &str = "true";
    /// `"false"`
    pub const ZN_FALSE: &str = "false";

    /// The library mode.
    /// String key : `"mode"`.
    /// Accepted values : `"peer"`, `"client"`.
    /// Default value : `"peer"`.
    pub const ZN_MODE_KEY: u64 = 0x40;
    pub const ZN_MODE_DEFAULT: &str = "peer";

    /// The locator of a peer to connect to.
    /// String key : `"peer"`.
    /// Accepted values : `<locator>` (ex: `"tcp/10.10.10.10:7447"`).
    /// Default value : None.
    /// Multiple values accepted.
    pub const ZN_PEER_KEY: u64 = 0x41;

    /// A locator to listen on.
    /// String key : `"listener"`.
    /// Accepted values : `<locator>` (ex: `"tcp/10.10.10.10:7447"`).
    /// Default value : None.
    /// Multiple values accepted.
    pub const ZN_LISTENER_KEY: u64 = 0x42;

    /// The user name to use for authentication.
    /// String key : `"user"`.
    /// Accepted values : `<string>`.
    /// Default value : None.
    pub const ZN_USER_KEY: u64 = 0x43;

    /// The password to use for authentication.
    /// String key : `"password"`.
    /// Accepted values : `<string>`.
    /// Default value : None.
    pub const ZN_PASSWORD_KEY: u64 = 0x44;

    /// Activates/Desactivates multicast scouting.
    /// String key : `"multicast_scouting"`.
    /// Accepted values : `"true"`, `"false"`.
    /// Default value : `"true"`.
    pub const ZN_MULTICAST_SCOUTING_KEY: u64 = 0x45;
    pub const ZN_MULTICAST_SCOUTING_DEFAULT: &str = "true";

    /// The network interface to use for multicast scouting.
    /// String key : `"multicast_interface"`.
    /// Accepted values : `"auto"`, `<ip address>`, `<interface name>`.
    /// Default value : `"auto"`.
    pub const ZN_MULTICAST_INTERFACE_KEY: u64 = 0x46;
    pub const ZN_MULTICAST_INTERFACE_DEFAULT: &str = "auto";

    /// The multicast address and ports to use for multicast scouting.
    /// String key : `"multicast_address"`.
    /// Accepted values : `<ip address>:<port>`.
    /// Default value : `"224.0.0.224:7447"`.
    pub const ZN_MULTICAST_ADDRESS_KEY: u64 = 0x47;
    pub const ZN_MULTICAST_ADDRESS_DEFAULT: &str = "224.0.0.224:7447";

    /// In client mode, the period dedicated to scouting a router before failing.
    /// String key : `"scouting_timeout"`.
    /// Accepted values : `<float in seconds>`.
    /// Default value : `"3.0"`.
    pub const ZN_SCOUTING_TIMEOUT_KEY: u64 = 0x48;
    pub const ZN_SCOUTING_TIMEOUT_DEFAULT: &str = "3.0";

    /// In peer mode, the period dedicated to scouting first remote peers before doing anything else.
    /// String key : `"scouting_delay"`.
    /// Accepted values : `<float in seconds>`.
    /// Default value : `"0.2"`.
    pub const ZN_SCOUTING_DELAY_KEY: u64 = 0x49;
    pub const ZN_SCOUTING_DELAY_DEFAULT: &str = "0.2";

    /// Indicates if data messages should be timestamped.
    /// String key : `"add_timestamp"`.
    /// Accepted values : `"true"`, `"false"`.
    /// Default value : `"false"`.
    pub const ZN_ADD_TIMESTAMP_KEY: u64 = 0x4A;
    pub const ZN_ADD_TIMESTAMP_DEFAULT: &str = "false";

    /// Indicates if the link state protocol should run.
    /// String key : `"link_state"`.
    /// Accepted values : `"true"`, `"false"`.
    /// Default value : `"true"`.
    pub const ZN_LINK_STATE_KEY: u64 = 0x4B;
    pub const ZN_LINK_STATE_DEFAULT: &str = "true";

    /// Indicates if peers should connect to each other
    /// when they discover each other (through multicast
    /// or link_state protocol).
    /// String key : `"peers_autoconnect"`.
    /// Accepted values : `"true"`, `"false"`.
    /// Default value : `"true"`.
    pub const ZN_PEERS_AUTOCONNECT_KEY: u64 = 0x4C;
    pub const ZN_PEERS_AUTOCONNECT_DEFAULT: &str = "true";

    pub(crate) fn parse_mode(m: &str) -> Result<whatami::Type, ()> {
        match m {
            "peer" => Ok(whatami::PEER),
            "client" => Ok(whatami::CLIENT),
            "router" => Ok(whatami::ROUTER),
            _ => Err(()),
        }
    }
}

pub struct RuntimeTranscoder();
impl KeyTranscoder for RuntimeTranscoder {
    fn encode(key: &str) -> Option<u64> {
        match &key.to_lowercase()[..] {
            "mode" => Some(ZN_MODE_KEY),
            "peer" => Some(ZN_PEER_KEY),
            "listener" => Some(ZN_LISTENER_KEY),
            "user" => Some(ZN_USER_KEY),
            "password" => Some(ZN_PASSWORD_KEY),
            "multicast_scouting" => Some(ZN_MULTICAST_SCOUTING_KEY),
            "multicast_interface" => Some(ZN_MULTICAST_INTERFACE_KEY),
            "multicast_address" => Some(ZN_MULTICAST_ADDRESS_KEY),
            "scouting_timeout" => Some(ZN_SCOUTING_TIMEOUT_KEY),
            "scouting_delay" => Some(ZN_SCOUTING_DELAY_KEY),
            "add_timestamp" => Some(ZN_ADD_TIMESTAMP_KEY),
            "link_state" => Some(ZN_LINK_STATE_KEY),
            "peers_autoconnect" => Some(ZN_PEERS_AUTOCONNECT_KEY),
            _ => None,
        }
    }

    fn decode(key: u64) -> Option<String> {
        match key {
            0x40 => Some("mode".to_string()),
            0x41 => Some("peer".to_string()),
            0x42 => Some("listener".to_string()),
            0x43 => Some("user".to_string()),
            0x44 => Some("password".to_string()),
            0x45 => Some("multicast_scouting".to_string()),
            0x46 => Some("multicast_interface".to_string()),
            0x47 => Some("multicast_address".to_string()),
            0x48 => Some("scouting_timeout".to_string()),
            0x49 => Some("scouting_delay".to_string()),
            0x4A => Some("add_timestamp".to_string()),
            0x4B => Some("link_state".to_string()),
            0x4C => Some("peers_autoconnect".to_string()),
            _ => None,
        }
    }
}

pub type RuntimeProperties = IntKeyProperties<RuntimeTranscoder>;
