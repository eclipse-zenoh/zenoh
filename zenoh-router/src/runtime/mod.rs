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
use std::collections::HashMap;
use std::io::Cursor;
use uhlc::HLC;
use zenoh_protocol::core::{whatami, PeerId};
use zenoh_util::properties::runtime::*;
// #[cfg(feature = "transport_tls")]
use zenoh_protocol::link::tls::{internal::pemfile, ClientConfig, NoClientAuth, ServerConfig};
use zenoh_protocol::link::LocatorProperty;
use zenoh_protocol::session::authenticator::{PeerAuthenticator, UserPasswordAuthenticator};
use zenoh_protocol::session::{SessionManager, SessionManagerConfig, SessionManagerOptionalConfig};
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::properties::Properties;
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

pub(crate) fn parse_mode(m: &str) -> Result<whatami::Type, ()> {
    match m {
        "peer" => Ok(whatami::PEER),
        "client" => Ok(whatami::CLIENT),
        "router" => Ok(whatami::ROUTER),
        _ => Err(()),
    }
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
        let mut router = Arc::new(Router::new(pid.clone(), whatami, hlc));

        let sm_config = SessionManagerConfig {
            version,
            whatami,
            id: pid.clone(),
            handler: router.clone(),
        };
        let sm_opt_config = build_opt_config_from_properties(&config).await?;

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
