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
use async_std::fs;
use async_std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};
use config::*;
use std::collections::HashMap;
use std::io::Cursor;
use uhlc::HLC;
use zenoh_protocol::core::PeerId;
// #[cfg(feature = "transport_tls")]
use zenoh_protocol::link::tls::{internal::pemfile, ClientConfig, NoClientAuth, ServerConfig};
use zenoh_protocol::link::LocatorProperty;
use zenoh_protocol::session::authenticator::{PeerAuthenticator, UserPasswordAuthenticator};
use zenoh_protocol::session::{SessionManager, SessionManagerConfig, SessionManagerOptionalConfig};
use zenoh_util::collections::{IntKeyProperties, KeyTranscoder, Properties};
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

async fn build_user_password_peer_authenticator(
    config: &RuntimeProperties,
) -> ZResult<Option<UserPasswordAuthenticator>> {
    if let Some(user) = config.get(&ZN_USER_KEY) {
        if let Some(password) = config.get(&ZN_PASSWORD_KEY) {
            // We have both user password parameter defined. Check if we
            // need to build the user-password lookup dictionary for incoming
            // connections, e.g. on the router.
            let mut lookup: HashMap<Vec<u8>, Vec<u8>> = HashMap::new();
            if let Some(dict) = config.get(&ZN_USER_PASSWORD_DICTIONARY_KEY) {
                let content = fs::read_to_string(dict).await.map_err(|e| {
                    zerror2!(ZErrorKind::Other {
                        descr: format!("Invalid user-password dictionary file: {}", e)
                    })
                })?;
                // Populate the user-password dictionary
                let mut ps = Properties::from(content);
                for (user, password) in ps.drain() {
                    lookup.insert(user.into(), password.into());
                }
            }
            // Create the UserPassword Authenticator based on provided info
            let upa = UserPasswordAuthenticator::new(
                lookup,
                (user.to_string().into(), password.to_string().into()),
            );
            log::debug!("User-password authentication is enabled");

            return Ok(Some(upa));
        }
    }
    Ok(None)
}

// #[cfg(feature = "transport_tls")]
async fn build_tls_locator_property(
    config: &RuntimeProperties,
) -> ZResult<Option<LocatorProperty>> {
    let mut client_config: Option<ClientConfig> = None;
    if let Some(tls_ca_certificate) = config.get(&ZN_TLS_ROOT_CA_CERTIFICATE_KEY) {
        let ca = fs::read(tls_ca_certificate).await.map_err(|e| {
            zerror2!(ZErrorKind::Other {
                descr: format!("Invalid TLS CA certificate file: {}", e)
            })
        })?;
        let mut cc = ClientConfig::new();
        let _ = cc
            .root_store
            .add_pem_file(&mut Cursor::new(ca))
            .map_err(|_| {
                zerror2!(ZErrorKind::Other {
                    descr: "Invalid TLS CA certificate file".to_string()
                })
            })?;
        client_config = Some(cc);
        log::debug!("TLS client is configured");
    }

    let mut server_config: Option<ServerConfig> = None;
    if let Some(tls_server_private_key) = config.get(&ZN_TLS_SERVER_PRIVATE_KEY_KEY) {
        if let Some(tls_server_certificate) = config.get(&ZN_TLS_SERVER_CERTIFICATE_KEY) {
            let pkey = fs::read(tls_server_private_key).await.map_err(|e| {
                zerror2!(ZErrorKind::Other {
                    descr: format!("Invalid TLS private key file: {}", e)
                })
            })?;
            let mut keys = pemfile::rsa_private_keys(&mut Cursor::new(pkey)).unwrap();

            let cert = fs::read(tls_server_certificate).await.map_err(|e| {
                zerror2!(ZErrorKind::Other {
                    descr: format!("Invalid TLS server certificate file: {}", e)
                })
            })?;
            let certs = pemfile::certs(&mut Cursor::new(cert)).unwrap();

            let mut sc = ServerConfig::new(NoClientAuth::new());
            sc.set_single_cert(certs, keys.remove(0)).unwrap();
            server_config = Some(sc);
            log::debug!("TLS server is configured");
        }
    }

    if client_config.is_none() && server_config.is_none() {
        Ok(None)
    } else {
        Ok(Some((client_config, server_config).into()))
    }
}

async fn build_opt_config_from_properties(
    config: &RuntimeProperties,
) -> ZResult<SessionManagerOptionalConfig> {
    let mut peer_authenticator: Vec<PeerAuthenticator> = vec![];
    let mut locator_property: Vec<LocatorProperty> = vec![];

    let mut upa = build_user_password_peer_authenticator(config).await?;
    if let Some(upa) = upa.take() {
        peer_authenticator.push(Arc::new(upa));
    }

    // #[cfg(feature = "transport_tls")]
    {
        let mut tls = build_tls_locator_property(config).await?;
        if let Some(tls) = tls.take() {
            locator_property.push(tls);
        }
    }

    let opt_config = SessionManagerOptionalConfig {
        lease: None,
        keep_alive: None,
        sn_resolution: None,
        batch_size: None,
        timeout: None,
        retries: None,
        max_sessions: None,
        max_links: None,
        peer_authenticator: if peer_authenticator.is_empty() {
            None
        } else {
            Some(peer_authenticator)
        },
        link_authenticator: None,
        locator_property: if locator_property.is_empty() {
            None
        } else {
            Some(locator_property)
        },
    };
    Ok(opt_config)
}

#[derive(Clone)]
pub struct Runtime {
    state: Arc<RwLock<RuntimeState>>,
}

impl Runtime {
    pub async fn new(version: u8, config: RuntimeProperties, id: Option<&str>) -> ZResult<Runtime> {
        let pid = if let Some(s) = id {
            // filter-out '-' characters (in case s has UUID format)
            let s = s.replace('-', "");
            let vec = hex::decode(&s).map_err(|e| {
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
        let mut router = Arc::new(Router::new(whatami, hlc));

        let sm_config = SessionManagerConfig {
            version,
            whatami,
            id: pid.clone(),
            handler: router.clone(),
        };
        let sm_opt_config = build_opt_config_from_properties(&config).await?;

        let session_manager = SessionManager::new(sm_config, Some(sm_opt_config));
        let mut orchestrator = SessionOrchestrator::new(session_manager, whatami);
        if config
            .get_or(&ZN_LINK_STATE_KEY, ZN_LINK_STATE_DEFAULT)
            .to_lowercase()
            == ZN_TRUE
        {
            unsafe {
                Arc::get_mut_unchecked(&mut router)
                    .init_link_state(pid.clone(), orchestrator.clone())
                    .await;
            }
        }
        match orchestrator.init(config).await {
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
    pub const ZN_MODE_STR: &str = "mode";
    pub const ZN_MODE_DEFAULT: &str = "peer";

    /// The locator of a peer to connect to.
    /// String key : `"peer"`.
    /// Accepted values : `<locator>` (ex: `"tcp/10.10.10.10:7447"`).
    /// Default value : None.
    /// Multiple values accepted.
    pub const ZN_PEER_KEY: u64 = 0x41;
    pub const ZN_PEER_STR: &str = "peer";

    /// A locator to listen on.
    /// String key : `"listener"`.
    /// Accepted values : `<locator>` (ex: `"tcp/10.10.10.10:7447"`).
    /// Default value : None.
    /// Multiple values accepted.
    pub const ZN_LISTENER_KEY: u64 = 0x42;
    pub const ZN_LISTENER_STR: &str = "listener";

    /// The user name to use for authentication.
    /// String key : `"user"`.
    /// Accepted values : `<string>`.
    /// Default value : None.
    pub const ZN_USER_KEY: u64 = 0x43;
    pub const ZN_USER_STR: &str = "user";

    /// The password to use for authentication.
    /// String key : `"password"`.
    /// Accepted values : `<string>`.
    /// Default value : None.
    pub const ZN_PASSWORD_KEY: u64 = 0x44;
    pub const ZN_PASSWORD_STR: &str = "password";

    /// Activates/Desactivates multicast scouting.
    /// String key : `"multicast_scouting"`.
    /// Accepted values : `"true"`, `"false"`.
    /// Default value : `"true"`.
    pub const ZN_MULTICAST_SCOUTING_KEY: u64 = 0x45;
    pub const ZN_MULTICAST_SCOUTING_STR: &str = "multicast_scouting";
    pub const ZN_MULTICAST_SCOUTING_DEFAULT: &str = "true";

    /// The network interface to use for multicast scouting.
    /// String key : `"multicast_interface"`.
    /// Accepted values : `"auto"`, `<ip address>`, `<interface name>`.
    /// Default value : `"auto"`.
    pub const ZN_MULTICAST_INTERFACE_KEY: u64 = 0x46;
    pub const ZN_MULTICAST_INTERFACE_STR: &str = "multicast_interface";
    pub const ZN_MULTICAST_INTERFACE_DEFAULT: &str = "auto";

    /// The multicast address and ports to use for multicast scouting.
    /// String key : `"multicast_address"`.
    /// Accepted values : `<ip address>:<port>`.
    /// Default value : `"224.0.0.224:7447"`.
    pub const ZN_MULTICAST_ADDRESS_KEY: u64 = 0x47;
    pub const ZN_MULTICAST_ADDRESS_STR: &str = "multicast_address";
    pub const ZN_MULTICAST_ADDRESS_DEFAULT: &str = "224.0.0.224:7447";

    /// In client mode, the period dedicated to scouting a router before failing.
    /// String key : `"scouting_timeout"`.
    /// Accepted values : `<float in seconds>`.
    /// Default value : `"3.0"`.
    pub const ZN_SCOUTING_TIMEOUT_KEY: u64 = 0x48;
    pub const ZN_SCOUTING_TIMEOUT_STR: &str = "scouting_timeout";
    pub const ZN_SCOUTING_TIMEOUT_DEFAULT: &str = "3.0";

    /// In peer mode, the period dedicated to scouting first remote peers before doing anything else.
    /// String key : `"scouting_delay"`.
    /// Accepted values : `<float in seconds>`.
    /// Default value : `"0.2"`.
    pub const ZN_SCOUTING_DELAY_KEY: u64 = 0x49;
    pub const ZN_SCOUTING_DELAY_STR: &str = "scouting_delay";
    pub const ZN_SCOUTING_DELAY_DEFAULT: &str = "0.2";

    /// Indicates if data messages should be timestamped.
    /// String key : `"add_timestamp"`.
    /// Accepted values : `"true"`, `"false"`.
    /// Default value : `"false"`.
    pub const ZN_ADD_TIMESTAMP_KEY: u64 = 0x4A;
    pub const ZN_ADD_TIMESTAMP_STR: &str = "add_timestamp";
    pub const ZN_ADD_TIMESTAMP_DEFAULT: &str = "false";

    /// Indicates if the link state protocol should run.
    /// String key : `"link_state"`.
    /// Accepted values : `"true"`, `"false"`.
    /// Default value : `"true"`.
    pub const ZN_LINK_STATE_KEY: u64 = 0x4B;
    pub const ZN_LINK_STATE_STR: &str = "link_state";
    pub const ZN_LINK_STATE_DEFAULT: &str = "true";

    /// The file path containing the user password dictionary.
    /// String key : `"user_password_dictionary"`.
    /// Accepted values : `<file path>`.
    /// Default value : None.
    pub const ZN_USER_PASSWORD_DICTIONARY_KEY: u64 = 0x4C;
    pub const ZN_USER_PASSWORD_DICTIONARY_STR: &str = "user_password_dictionary";

    /// The file path containing the TLS server private key.
    /// String key : `"tls_private_key"`.
    /// Accepted values : `<file path>`.
    /// Default value : None.
    pub const ZN_TLS_SERVER_PRIVATE_KEY_KEY: u64 = 0x4D;
    pub const ZN_TLS_SERVER_PRIVATE_KEY_STR: &str = "tls_server_private_key";

    /// The file path containing the TLS server certificate.
    /// String key : `"tls_private_key"`.
    /// Accepted values : `<file path>`.
    /// Default value : None.
    pub const ZN_TLS_SERVER_CERTIFICATE_KEY: u64 = 0x4E;
    pub const ZN_TLS_SERVER_CERTIFICATE_STR: &str = "tls_server_certificate";

    /// The file path containing the TLS root CA certificate.
    /// String key : `"tls_private_key"`.
    /// Accepted values : `<file path>`.
    /// Default value : None.
    pub const ZN_TLS_ROOT_CA_CERTIFICATE_KEY: u64 = 0x4F;
    pub const ZN_TLS_ROOT_CA_CERTIFICATE_STR: &str = "tls_root_ca_certificate";

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
            ZN_MODE_STR => Some(ZN_MODE_KEY),
            ZN_PEER_STR => Some(ZN_PEER_KEY),
            ZN_LISTENER_STR => Some(ZN_LISTENER_KEY),
            ZN_USER_STR => Some(ZN_USER_KEY),
            ZN_PASSWORD_STR => Some(ZN_PASSWORD_KEY),
            ZN_MULTICAST_SCOUTING_STR => Some(ZN_MULTICAST_SCOUTING_KEY),
            ZN_MULTICAST_INTERFACE_STR => Some(ZN_MULTICAST_INTERFACE_KEY),
            ZN_MULTICAST_ADDRESS_STR => Some(ZN_MULTICAST_ADDRESS_KEY),
            ZN_SCOUTING_TIMEOUT_STR => Some(ZN_SCOUTING_TIMEOUT_KEY),
            ZN_SCOUTING_DELAY_STR => Some(ZN_SCOUTING_DELAY_KEY),
            ZN_ADD_TIMESTAMP_STR => Some(ZN_ADD_TIMESTAMP_KEY),
            ZN_LINK_STATE_STR => Some(ZN_LINK_STATE_KEY),
            ZN_USER_PASSWORD_DICTIONARY_STR => Some(ZN_USER_PASSWORD_DICTIONARY_KEY),
            ZN_TLS_SERVER_PRIVATE_KEY_STR => Some(ZN_TLS_SERVER_PRIVATE_KEY_KEY),
            ZN_TLS_SERVER_CERTIFICATE_STR => Some(ZN_TLS_SERVER_CERTIFICATE_KEY),
            ZN_TLS_ROOT_CA_CERTIFICATE_STR => Some(ZN_TLS_ROOT_CA_CERTIFICATE_KEY),
            _ => None,
        }
    }

    fn decode(key: u64) -> Option<String> {
        match key {
            ZN_MODE_KEY => Some(ZN_MODE_STR.to_string()),
            ZN_PEER_KEY => Some(ZN_PEER_STR.to_string()),
            ZN_LISTENER_KEY => Some(ZN_LISTENER_STR.to_string()),
            ZN_USER_KEY => Some(ZN_USER_STR.to_string()),
            ZN_PASSWORD_KEY => Some(ZN_PASSWORD_STR.to_string()),
            ZN_MULTICAST_SCOUTING_KEY => Some(ZN_MULTICAST_SCOUTING_STR.to_string()),
            ZN_MULTICAST_INTERFACE_KEY => Some(ZN_MULTICAST_INTERFACE_STR.to_string()),
            ZN_MULTICAST_ADDRESS_KEY => Some(ZN_MULTICAST_ADDRESS_STR.to_string()),
            ZN_SCOUTING_TIMEOUT_KEY => Some(ZN_SCOUTING_TIMEOUT_STR.to_string()),
            ZN_SCOUTING_DELAY_KEY => Some(ZN_SCOUTING_DELAY_STR.to_string()),
            ZN_ADD_TIMESTAMP_KEY => Some(ZN_ADD_TIMESTAMP_STR.to_string()),
            ZN_LINK_STATE_KEY => Some(ZN_LINK_STATE_STR.to_string()),
            ZN_USER_PASSWORD_DICTIONARY_KEY => Some(ZN_USER_PASSWORD_DICTIONARY_STR.to_string()),
            ZN_TLS_SERVER_PRIVATE_KEY_KEY => Some(ZN_TLS_SERVER_PRIVATE_KEY_STR.to_string()),
            ZN_TLS_SERVER_CERTIFICATE_KEY => Some(ZN_TLS_SERVER_CERTIFICATE_STR.to_string()),
            ZN_TLS_ROOT_CA_CERTIFICATE_KEY => Some(ZN_TLS_ROOT_CA_CERTIFICATE_STR.to_string()),
            _ => None,
        }
    }
}

pub type RuntimeProperties = IntKeyProperties<RuntimeTranscoder>;
