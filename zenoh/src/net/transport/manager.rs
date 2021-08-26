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
use super::multicast::manager::{TransportManagerConfigMulticast, TransportManagerStateMulticast};
use super::protocol::core::{whatami, PeerId, WhatAmI, ZInt};
#[cfg(feature = "zero-copy")]
use super::protocol::io::SharedMemoryReader;
use super::protocol::proto::defaults::{BATCH_SIZE, SEQ_NUM_RES, VERSION};
use super::unicast::manager::{TransportManagerConfigUnicast, TransportManagerStateUnicast};
use super::unicast::TransportUnicast;
use super::TransportEventHandler;
use crate::net::link::{EndPoint, Locator, LocatorConfig, LocatorProtocol};
use async_std::sync::{Arc as AsyncArc, Mutex as AsyncMutex};
use rand::{RngCore, SeedableRng};
use std::collections::HashMap;
use std::sync::Arc;
#[cfg(feature = "zero-copy")]
use std::sync::RwLock;
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::crypto::{BlockCipher, PseudoRng};
use zenoh_util::properties::{config::*, Properties};
use zenoh_util::zparse;

/// # Examples
/// ```
/// use async_std::sync::Arc;
/// use std::time::Duration;
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
///         .lease(Duration::from_secs(1))
///         .keep_alive(Duration::from_millis(100))
///         .open_timeout(Duration::from_secs(1))
///         .open_pending(10)   // Set to 10 the number of simultanous pending incoming transports
///         .max_sessions(5)    // Allow max 5 transports open
///         .max_links(2)       // Allow max 2 links per transport
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
    pub batch_size: u16,
    pub defrag_buff_size: usize,
    pub link_rx_buff_size: usize,
    pub unicast: TransportManagerConfigUnicast,
    pub multicast: TransportManagerConfigMulticast,
    pub endpoint: HashMap<LocatorProtocol, Properties>,
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
    batch_size: u16,
    defrag_buff_size: usize,
    link_rx_buff_size: usize,
    unicast: TransportManagerConfigUnicast,
    multicast: TransportManagerConfigMulticast,
    endpoint: HashMap<LocatorProtocol, Properties>,
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

    pub fn batch_size(mut self, batch_size: u16) -> Self {
        self.batch_size = batch_size;
        self
    }

    pub fn defrag_buff_size(mut self, defrag_buff_size: usize) -> Self {
        self.defrag_buff_size = defrag_buff_size;
        self
    }

    pub fn link_rx_buff_size(mut self, link_rx_buff_size: usize) -> Self {
        self.link_rx_buff_size = link_rx_buff_size;
        self
    }

    pub fn endpoint(mut self, endpoint: HashMap<LocatorProtocol, Properties>) -> Self {
        self.endpoint = endpoint;
        self
    }

    pub fn unicast(mut self, unicast: TransportManagerConfigUnicast) -> Self {
        self.unicast = unicast;
        self
    }

    pub fn multicast(mut self, multicast: TransportManagerConfigMulticast) -> Self {
        self.multicast = multicast;
        self
    }

    pub fn build(self, handler: Arc<dyn TransportEventHandler>) -> TransportManagerConfig {
        TransportManagerConfig {
            version: self.version,
            pid: self.pid,
            whatami: self.whatami,
            sn_resolution: self.sn_resolution,
            batch_size: self.batch_size,
            defrag_buff_size: self.defrag_buff_size,
            link_rx_buff_size: self.link_rx_buff_size,
            unicast: self.unicast,
            multicast: self.multicast,
            endpoint: self.endpoint,
            handler,
        }
    }

    pub async fn from_config(
        mut self,
        properties: &ConfigProperties,
    ) -> ZResult<TransportManagerConfigBuilder> {
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
        if let Some(v) = properties.get(&ZN_DEFRAG_BUFF_SIZE_KEY) {
            self = self.defrag_buff_size(zparse!(v)?);
        }
        if let Some(v) = properties.get(&ZN_LINK_RX_BUFF_SIZE_KEY) {
            self = self.link_rx_buff_size(zparse!(v)?);
        }

        self = self.endpoint(LocatorConfig::from_config(properties)?);
        self = self.unicast(
            TransportManagerConfigUnicast::builder()
                .from_config(properties)
                .await?
                .build(),
        );

        Ok(self)
    }
}

impl Default for TransportManagerConfigBuilder {
    fn default() -> Self {
        Self {
            version: VERSION,
            pid: PeerId::rand(),
            whatami: whatami::parse(ZN_MODE_DEFAULT).unwrap(),
            sn_resolution: SEQ_NUM_RES,
            batch_size: BATCH_SIZE,
            defrag_buff_size: zparse!(ZN_DEFRAG_BUFF_SIZE_DEFAULT).unwrap(),
            link_rx_buff_size: zparse!(ZN_LINK_RX_BUFF_SIZE_DEFAULT).unwrap(),
            endpoint: HashMap::new(),
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
        self.config.pid
    }

    /*************************************/
    /*              LISTENER             */
    /*************************************/
    pub async fn add_listener(&self, endpoint: EndPoint) -> ZResult<Locator> {
        if endpoint.locator.address.is_multicast() {
            // @TODO: multicast
            unimplemented!();
        } else {
            self.add_listener_unicast(endpoint).await
        }
    }

    pub async fn del_listener(&self, endpoint: &EndPoint) -> ZResult<()> {
        if endpoint.locator.address.is_multicast() {
            // @TODO: multicast
            unimplemented!();
        } else {
            self.del_listener_unicast(endpoint).await
        }
    }

    pub fn get_listeners(&self) -> Vec<EndPoint> {
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

    pub async fn open_transport(&self, endpoint: EndPoint) -> ZResult<TransportUnicast> {
        if endpoint.locator.address.is_multicast() {
            // @TODO: multicast
            unimplemented!();
        } else {
            self.open_transport_unicast(endpoint).await
        }
    }
}
