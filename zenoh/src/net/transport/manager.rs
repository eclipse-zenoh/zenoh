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
use super::defaults::*;
use super::multicast::manager::{TransportManagerConfigMulticast, TransportManagerStateMulticast};
use super::protocol::core::{whatami, PeerId, WhatAmI, ZInt};
#[cfg(feature = "zero-copy")]
use super::protocol::io::SharedMemoryReader;
use super::unicast::manager::{TransportManagerConfigUnicast, TransportManagerStateUnicast};
use super::unicast::TransportUnicast;
use super::TransportEventHandler;
use crate::net::link::{Locator, LocatorProperty, LocatorProtocol};
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

/// # Examples
/// ```
/// use async_std::sync::Arc;
/// use zenoh::net::protocol::core::{PeerId, WhatAmI, whatami};
/// use zenoh::net::transport::*;
/// use zenoh_util::core::ZResult;
///
/// // Create my transport handler to be notified when a new transport is initiated with me
/// #[derive(Default)]
/// struct MySH;
///
/// impl TransportEventHandler for MySH {
///     fn new_unicast(&self,
///         _transport: TransportUnicast
///     ) -> ZResult<Arc<dyn TransportUnicastEventHandler>> {
///         Ok(Arc::new(DummyTransportUnicastEventHandler::default()))
///     }
///
///     fn new_multicast(&self,
///         _transport: TransportMulticast
///     ) -> ZResult<Arc<dyn TransportMulticastEventHandler>> {
///         Ok(Arc::new(DummyTransportMulticastEventHandler::default()))
///     }
/// }
///
/// // Create the default TransportManager
/// let config = TransportManagerConfig::builder()
///         .build(Arc::new(MySH::default()));
/// let manager = TransportManager::new(config);
///
/// // Create the TransportManager with custom configuration
/// // Configure the unicast transports parameters
/// let unicast = TransportManagerConfigUnicast::builder()
///         .lease(1_000)                   // Set the link lease to 1s
///         .keep_alive(100)                // Set the link keep alive interval to 100ms
///         .open_timeout(1_000)            // Set an open timeout to 1s
///         .open_pending(10)               // Set to 10 the number of simultanous pending incoming transports
///         .max_sessions(5)                // Allow max 5 transports open
///         .max_links(2)                   // Allow max 2 links per transport
///         .build();
/// let config = TransportManagerConfig::builder()
///         .pid(PeerId::rand())
///         .whatami(whatami::PEER)
///         .batch_size(1_024)              // Use a batch size of 1024 bytes
///         .sn_resolution(128)             // Use a sequence number resolution of 128
///         .unicast(unicast)               // Configure unicast parameters
///         .build(Arc::new(MySH::default()));
/// let manager = TransportManager::new(config);
/// ```

pub struct TransportManagerConfig {
    pub version: u8,
    pub pid: PeerId,
    pub whatami: WhatAmI,
    pub sn_resolution: ZInt,
    pub batch_size: usize,
    pub unicast: TransportManagerConfigUnicast,
    pub multicast: TransportManagerConfigMulticast,
    pub locator_property: HashMap<LocatorProtocol, LocatorProperty>,
    pub handler: Arc<dyn TransportEventHandler>,
}

impl TransportManagerConfig {
    pub fn builder() -> TransportManagerConfigBuilder {
        TransportManagerConfigBuilder::default()
    }
}

pub struct TransportManagerConfigBuilder {
    version: u8,
    pid: PeerId,
    whatami: WhatAmI,
    sn_resolution: ZInt,
    batch_size: usize,
    unicast: TransportManagerConfigUnicast,
    multicast: TransportManagerConfigMulticast,
    locator_property: HashMap<LocatorProtocol, LocatorProperty>,
}

impl TransportManagerConfigBuilder {
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

    pub fn locator_property(mut self, mut locator_property: Vec<LocatorProperty>) -> Self {
        let mut hm = HashMap::new();
        for lp in locator_property.drain(..) {
            hm.insert(lp.get_proto(), lp);
        }
        self.locator_property = hm;
        self
    }

    pub fn unicast(mut self, unicast: TransportManagerConfigUnicast) -> Self {
        self.unicast = unicast;
        self
    }

    pub fn build(self, handler: Arc<dyn TransportEventHandler>) -> TransportManagerConfig {
        TransportManagerConfig {
            version: self.version,
            pid: self.pid,
            whatami: self.whatami,
            sn_resolution: self.sn_resolution,
            batch_size: self.batch_size,
            unicast: self.unicast,
            multicast: self.multicast,
            locator_property: self.locator_property,
            handler,
        }
    }

    pub async fn from_properties(
        mut self,
        properties: &ConfigProperties,
    ) -> ZResult<TransportManagerConfigBuilder> {
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
            self = self.version(zparse!(v)?);
        }
        if let Some(v) = properties.get(&ZN_PEER_ID_KEY) {
            self = self.pid(zparse!(v)?);
        }
        if let Some(v) = properties.get(&ZN_MODE_KEY) {
            self = self.whatami(whatami::parse(v)?);
        }
        if let Some(v) = properties.get(&ZN_SEQ_NUM_RESOLUTION_KEY) {
            self = self.sn_resolution(zparse!(v)?);
        }
        if let Some(v) = properties.get(&ZN_BATCH_SIZE_KEY) {
            self = self.batch_size(zparse!(v)?);
        }
        self = self.locator_property(LocatorProperty::from_properties(properties).await?);
        self = self.unicast(
            TransportManagerConfigUnicast::builder()
                .from_properties(properties)
                .await?
                .build(),
        );

        Ok(self)
    }
}

impl Default for TransportManagerConfigBuilder {
    fn default() -> Self {
        Self {
            version: ZN_VERSION,
            pid: PeerId::rand(),
            whatami: ZN_DEFAULT_WHATAMI,
            sn_resolution: ZN_DEFAULT_SEQ_NUM_RESOLUTION,
            batch_size: ZN_DEFAULT_BATCH_SIZE,
            locator_property: HashMap::new(),
            unicast: TransportManagerConfigUnicast::default(),
            multicast: TransportManagerConfigMulticast::default(),
        }
    }
}

pub struct TransportManagerState {
    pub unicast: TransportManagerStateUnicast,
    pub multicast: TransportManagerStateMulticast,
}

impl Default for TransportManagerState {
    fn default() -> TransportManagerState {
        TransportManagerState {
            unicast: TransportManagerStateUnicast::default(),
            multicast: TransportManagerStateMulticast::default(),
        }
    }
}

#[derive(Clone)]
pub struct TransportManager {
    pub(crate) config: Arc<TransportManagerConfig>,
    pub(crate) state: Arc<TransportManagerState>,
    pub(crate) prng: AsyncArc<AsyncMutex<PseudoRng>>,
    pub(crate) cipher: Arc<BlockCipher>,
    #[cfg(feature = "zero-copy")]
    pub(crate) shmr: Arc<RwLock<SharedMemoryReader>>,
}

impl TransportManager {
    pub fn new(config: TransportManagerConfig) -> TransportManager {
        // Initialize the PRNG and the Cipher
        let mut prng = PseudoRng::from_entropy();
        let mut key = [0u8; BlockCipher::BLOCK_SIZE];
        prng.fill_bytes(&mut key);
        let cipher = BlockCipher::new(key);

        TransportManager {
            config: Arc::new(config),
            state: Arc::new(TransportManagerState::default()),
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
            // @TODO: multicast
            unimplemented!();
        } else {
            self.add_listener_unicast(locator).await
        }
    }

    pub async fn del_listener(&self, locator: &Locator) -> ZResult<()> {
        if locator.is_multicast() {
            // @TODO: multicast
            unimplemented!();
        } else {
            self.del_listener_unicast(locator).await
        }
    }

    pub fn get_listeners(&self) -> Vec<Locator> {
        self.get_listeners_unicast()
        // @TODO: multicast
    }

    pub fn get_locators(&self) -> Vec<Locator> {
        self.get_locators_unicast()
        // @TODO: multicast
    }

    /*************************************/
    /*             TRANSPORT             */
    /*************************************/
    pub fn get_transport(&self, peer: &PeerId) -> Option<TransportUnicast> {
        self.get_transport_unicast(peer)
        // @TODO: multicast
    }

    pub fn get_transports(&self) -> Vec<TransportUnicast> {
        self.get_transports_unicast()
        // @TODO: multicast
    }

    pub async fn open_transport(&self, locator: &Locator) -> ZResult<TransportUnicast> {
        if locator.is_multicast() {
            // @TODO: multicast
            unimplemented!();
        } else {
            self.open_transport_unicast(locator).await
        }
    }
}
