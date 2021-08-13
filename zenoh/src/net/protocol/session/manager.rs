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
use super::core::{PeerId, WhatAmI, ZInt};
use super::defaults::*;
#[cfg(feature = "zero-copy")]
use super::io::SharedMemoryReader;
use super::link::{Locator, LocatorProperty, LocatorProtocol};
use super::unicast::manager::{SessionManagerConfigUnicast, SessionManagerStateUnicast};
use super::{Session, SessionHandler};
use async_std::sync::{Arc as AsyncArc, Mutex as AsyncMutex};
use rand::{RngCore, SeedableRng};
use std::collections::HashMap;
use std::sync::Arc;
#[cfg(feature = "zero-copy")]
use std::sync::RwLock;
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::crypto::{BlockCipher, PseudoRng};
use zenoh_util::properties::config::ConfigProperties;
use zenoh_util::properties::config::*;

// / # Examples
// / ```
// / use async_std::sync::Arc;
// / use zenoh::net::protocol::core::{PeerId, WhatAmI, whatami};
// / use zenoh::net::protocol::session::{DummySessionEventHandler, SessionEventHandler, Session, SessionHandler, SessionManager, SessionManagerConfig, SessionManagerOptionalConfig};
// /
// / use zenoh_util::core::ZResult;
// /
// / // Create my session handler to be notified when a new session is initiated with me
// / struct MySH;
// /
// / impl MySH {
// /     fn new() -> MySH {
// /         MySH
// /     }
// / }
// /
// / impl SessionHandler for MySH {
// /     fn new_session(&self,
// /         _session: Session
// /     ) -> ZResult<Arc<dyn SessionEventHandler>> {
// /         Ok(Arc::new(DummySessionEventHandler::new()))
// /     }
// / }
// /
// / // Create the SessionManager
// / let config = SessionManagerConfig {
// /     version: 0,
// /     whatami: whatami::PEER,
// /     id: PeerId::from(uuid::Uuid::new_v4()),
// /     handler: Arc::new(MySH::new())
// / };
// / let manager = SessionManager::new(config, None);
// /
// / // Create the SessionManager with optional configuration
// / let config = SessionManagerConfig {
// /     version: 0,
// /     whatami: whatami::PEER,
// /     id: PeerId::from(uuid::Uuid::new_v4()),
// /     handler: Arc::new(MySH::new())
// / };
// / // Setting a value to None means to use the default value
// / let opt_config = SessionManagerOptionalConfig {
// /     lease: Some(1_000),             // Set the default lease to 1s
// /     keep_alive: Some(100),          // Set the default keep alive interval to 100ms
// /     sn_resolution: None,            // Use the default sequence number resolution
// /     open_timeout: None,             // Use the default open timeout
// /     open_incoming_pending: None,    // Use the default amount of pending incoming sessions
// /     batch_size: None,               // Use the default batch size
// /     max_sessions: Some(5),          // Accept any number of sessions
// /     max_links: None,                // Allow any number of links in a single session
// /     peer_authenticator: None,       // Accept any incoming session
// /     link_authenticator: None,       // Accept any incoming link
// /     locator_property: None,         // No specific link property
// / };
// / let manager_opt = SessionManager::new(config, Some(opt_config));
// / ```

pub struct SessionManagerConfig {
    pub version: u8,
    pub pid: PeerId,
    pub whatami: WhatAmI,
    pub sn_resolution: ZInt,
    pub batch_size: usize,
    pub unicast: SessionManagerConfigUnicast,
    pub locator_property: HashMap<LocatorProtocol, LocatorProperty>,
    pub handler: Arc<dyn SessionHandler>,
}

impl SessionManagerConfig {
    pub fn builder() -> SessionManagerConfigBuilder {
        SessionManagerConfigBuilder::default()
    }
}

pub struct SessionManagerConfigBuilder {
    pub version: u8,
    pub pid: PeerId,
    pub whatami: WhatAmI,
    pub sn_resolution: ZInt,
    pub batch_size: usize,
    pub unicast: SessionManagerConfigUnicast,
    pub locator_property: HashMap<LocatorProtocol, LocatorProperty>,
}

impl SessionManagerConfigBuilder {
    pub fn version(mut self, version: u8) -> Self {
        self.version = version;
        self
    }

    pub fn pid(mut self, pid: PeerId) -> Self {
        self.pid = pid;
        self
    }

    pub fn whatami(mut self, whatami: WhatAmI) -> Self {
        self.whatami = whatami;
        self
    }

    pub fn sn_resolution(mut self, sn_resolution: ZInt) -> Self {
        self.sn_resolution = sn_resolution;
        self
    }

    pub fn batch_size(mut self, batch_size: usize) -> Self {
        self.batch_size = batch_size;
        self
    }

    pub fn locator_property(
        mut self,
        locator_property: HashMap<LocatorProtocol, LocatorProperty>,
    ) -> Self {
        self.locator_property = locator_property;
        self
    }

    pub fn unicast(mut self, unicast: SessionManagerConfigUnicast) -> Self {
        self.unicast = unicast;
        self
    }

    pub fn build(self, handler: Arc<dyn SessionHandler>) -> SessionManagerConfig {
        SessionManagerConfig {
            version: self.version,
            pid: self.pid,
            whatami: self.whatami,
            sn_resolution: self.sn_resolution,
            batch_size: self.batch_size,
            unicast: self.unicast,
            locator_property: self.locator_property,
            handler,
        }
    }

    pub async fn from_properties(
        mut self,
        properties: &ConfigProperties,
    ) -> ZResult<SessionManagerConfigBuilder> {
        macro_rules! zparse {
            ($str:expr) => {
                $str.parse().map_err(|_| {
                    let e = format!(
                        "Failed to read configuration: {} is not a valid value",
                        $str
                    );
                    log::warn!("{}", e);
                    zerror2!(ZErrorKind::ValueDecodingFailed { descr: e })
                })
            };
        }

        if let Some(v) = properties.get(&ZN_VERSION_KEY) {
            self.version = zparse!(v)?;
        }
        if let Some(v) = properties.get(&ZN_PEER_ID_KEY) {
            self.pid = zparse!(v)?;
        }
        if let Some(v) = properties.get(&ZN_MODE_KEY) {
            self.whatami = zparse!(v)?;
        }
        if let Some(v) = properties.get(&ZN_SEQ_NUM_RESOLUTION_KEY) {
            self.sn_resolution = zparse!(v)?;
        }
        if let Some(v) = properties.get(&ZN_BATCH_SIZE_KEY) {
            self.batch_size = zparse!(v)?;
        }

        self.locator_property =
            LocatorProperty::vec_into_hashmap(LocatorProperty::from_properties(properties).await?);

        self.unicast = SessionManagerConfigUnicast::from_properties(properties).await?;

        Ok(self)
    }
}

impl Default for SessionManagerConfigBuilder {
    fn default() -> Self {
        Self {
            version: ZN_VERSION,
            pid: PeerId::rand(),
            whatami: ZN_DEFAULT_WHATAMI,
            sn_resolution: ZN_DEFAULT_SEQ_NUM_RESOLUTION,
            batch_size: ZN_DEFAULT_BATCH_SIZE,
            locator_property: HashMap::new(),
            unicast: SessionManagerConfigUnicast::default(),
        }
    }
}

pub struct SessionManagerState {
    pub unicast: SessionManagerStateUnicast,
}

impl Default for SessionManagerState {
    fn default() -> SessionManagerState {
        SessionManagerState {
            unicast: SessionManagerStateUnicast::default(),
        }
    }
}

#[derive(Clone)]
pub struct SessionManager {
    pub(crate) config: Arc<SessionManagerConfig>,
    pub(crate) state: Arc<SessionManagerState>,
    pub(crate) prng: AsyncArc<AsyncMutex<PseudoRng>>,
    pub(crate) cipher: Arc<BlockCipher>,
    #[cfg(feature = "zero-copy")]
    pub(crate) shmr: Arc<RwLock<SharedMemoryReader>>,
}

impl SessionManager {
    pub fn new(config: SessionManagerConfig) -> SessionManager {
        // Initialize the PRNG and the Cipher
        let mut prng = PseudoRng::from_entropy();
        let mut key = [0u8; BlockCipher::BLOCK_SIZE];
        prng.fill_bytes(&mut key);
        let cipher = BlockCipher::new(key);

        SessionManager {
            config: Arc::new(config),
            state: Arc::new(SessionManagerState::default()),
            prng: AsyncArc::new(AsyncMutex::new(prng)),
            cipher: Arc::new(cipher),
            #[cfg(feature = "zero-copy")]
            shmr: Arc::new(RwLock::new(SharedMemoryReader::new())),
        }
    }

    pub fn pid(&self) -> PeerId {
        self.config.pid.clone()
    }

    /*************************************/
    /*              LISTENER             */
    /*************************************/
    pub async fn add_listener(&self, locator: &Locator) -> ZResult<Locator> {
        if locator.is_multicast() {
            // @TODO multicast
            unimplemented!("");
        } else {
            self.add_listener_unicast(locator).await
        }
    }

    pub async fn del_listener(&self, locator: &Locator) -> ZResult<()> {
        if locator.is_multicast() {
            // @TODO multicast
            Ok(())
        } else {
            self.del_listener_unicast(locator).await
        }
    }

    pub fn get_listeners(&self) -> Vec<Locator> {
        self.get_listeners_unicast()
        // @TODO multicast
    }

    pub fn get_locators(&self) -> Vec<Locator> {
        self.get_locators_unicast()
        // @TODO multicast
    }

    /*************************************/
    /*              SESSION              */
    /*************************************/
    pub fn get_session(&self, peer: &PeerId) -> Option<Session> {
        if let Some(s) = self.get_session_unicast(peer) {
            return Some(s.into());
        }
        // @TODO multicast
        None
    }

    pub fn get_sessions(&self) -> Vec<Session> {
        self.get_sessions_unicast()
            .drain(..)
            .map(|s| s.into())
            .collect()
        // @TODO multicast
    }

    pub async fn open_session(&self, locator: &Locator) -> ZResult<Session> {
        if locator.is_multicast() {
            // @TODO multicast
            unimplemented!("");
        } else {
            let s = self.open_session_unicast(locator).await?;
            Ok(s.into())
        }
    }
}
