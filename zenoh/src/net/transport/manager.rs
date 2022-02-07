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
use super::multicast::manager::{
    TransportManagerBuilderMulticast, TransportManagerConfigMulticast,
    TransportManagerStateMulticast,
};
use super::protocol::core::{Version, WhatAmI, ZenohId};
#[cfg(feature = "shared-memory")]
use super::protocol::io::SharedMemoryReader;
use super::protocol::message::defaults::BATCH_SIZE;
use super::protocol::VERSION;
use super::unicast::manager::{
    TransportManagerBuilderUnicast, TransportManagerConfigUnicast, TransportManagerStateUnicast,
};
use super::unicast::TransportUnicast;
use super::TransportEventHandler;
use crate::config::Config;
use crate::net::link::{EndPoint, Locator, LocatorConfig, LocatorProtocol};
use crate::net::protocol::core::SeqNumBytes;
use async_std::sync::{Arc as AsyncArc, Mutex as AsyncMutex};
use rand::{RngCore, SeedableRng};
use std::collections::HashMap;
use std::convert::TryFrom;
use std::sync::Arc;
#[cfg(feature = "shared-memory")]
use std::sync::RwLock;
use zenoh_util::core::Result as ZResult;
use zenoh_util::crypto::{BlockCipher, PseudoRng};
use zenoh_util::properties::{config::*, Properties};
use zenoh_util::zparse;

/// # Examples
/// ```
/// use async_std::sync::Arc;
/// use std::time::Duration;
/// use zenoh::net::protocol::core::{SeqNumBytes, ZenohId, WhatAmI, whatami};
/// use zenoh::net::transport::*;
/// use zenoh::Result as ZResult;
///
/// // Create my transport handler to be notified when a new transport is initiated with me
/// #[derive(Default)]
/// struct MySH;
///
/// impl TransportEventHandler for MySH {
///     fn new_unicast(&self,
///         _peer: TransportPeer,
///         _transport: TransportUnicast
///     ) -> ZResult<Arc<dyn TransportPeerEventHandler>> {
///         Ok(Arc::new(DummyTransportPeerEventHandler::default()))
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
/// let manager = TransportManager::builder()
///         .build(Arc::new(MySH::default()))
///         .unwrap();
///
/// // Create the TransportManager with custom configuration
/// // Configure the unicast transports parameters
/// let unicast = TransportManager::config_unicast()
///         .lease(Duration::from_secs(1))
///         .keep_alive(Duration::from_millis(100))
///         .open_timeout(Duration::from_secs(1))
///         .open_pending(10)   // Set to 10 the number of simultanous pending incoming transports        
///         .max_links(1)    // Allow max 1 inbound link per transport
///         .max_sessions(5);   // Allow max 5 transports open
/// let manager = TransportManager::builder()
///         .zid(ZenohId::rand())
///         .whatami(WhatAmI::Peer)
///         .batch_size(1_024)              // Use a batch size of 1024 bytes
///         .sn_bytes(SeqNumBytes::Four)    // Use max 4 bytes for the SN: 2^28 sequence numbers
///         .unicast(unicast)               // Configure unicast parameters
///         .build(Arc::new(MySH::default()))
///         .unwrap();
/// ```

pub struct TransportManagerConfig {
    pub version: Version,
    pub zid: ZenohId,
    pub whatami: WhatAmI,
    pub sn_bytes: SeqNumBytes,
    pub batch_size: u16,
    pub defrag_buff_size: usize,
    pub link_rx_buff_size: usize,
    pub unicast: TransportManagerConfigUnicast,
    pub multicast: TransportManagerConfigMulticast,
    pub endpoint: HashMap<LocatorProtocol, Properties>,
    pub handler: Arc<dyn TransportEventHandler>,
}

pub struct TransportManagerState {
    pub unicast: TransportManagerStateUnicast,
    pub multicast: TransportManagerStateMulticast,
}

pub struct TransportManagerParams {
    config: TransportManagerConfig,
    state: TransportManagerState,
}

pub struct TransportManagerBuilder {
    version: Version,
    zid: ZenohId,
    whatami: WhatAmI,
    sn_bytes: SeqNumBytes,
    batch_size: u16,
    defrag_buff_size: usize,
    link_rx_buff_size: usize,
    unicast: TransportManagerBuilderUnicast,
    multicast: TransportManagerBuilderMulticast,
    endpoint: HashMap<LocatorProtocol, Properties>,
}

impl TransportManagerBuilder {
    pub fn zid(mut self, zid: ZenohId) -> Self {
        self.zid = zid;
        self
    }

    pub fn whatami(mut self, whatami: WhatAmI) -> Self {
        self.whatami = whatami;
        self
    }

    pub fn sn_bytes(mut self, sn_bytes: SeqNumBytes) -> Self {
        self.sn_bytes = sn_bytes;
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

    pub fn unicast(mut self, unicast: TransportManagerBuilderUnicast) -> Self {
        self.unicast = unicast;
        self
    }

    pub fn multicast(mut self, multicast: TransportManagerBuilderMulticast) -> Self {
        self.multicast = multicast;
        self
    }

    pub async fn from_config(mut self, properties: &Config) -> ZResult<TransportManagerBuilder> {
        if let Some(v) = properties.id() {
            self = self.zid(zparse!(v)?);
        }
        if let Some(v) = properties.mode() {
            self = self.whatami(*v);
        }
        if let Some(v) = properties.transport().sequence_number_resolution() {
            let b = u8::try_from(*v).map_err(|_| zerror!("Invalid SN Bytes"))?;
            let b = SeqNumBytes::try_from(b).map_err(|_| zerror!("Invalid SN Bytes"))?;
            self = self.sn_bytes(b);
        }
        if let Some(v) = properties.transport().link().batch_size() {
            self = self.batch_size(*v);
        }
        if let Some(v) = properties.transport().link().defrag_buffer_size() {
            self = self.defrag_buff_size(*v);
        }
        if let Some(v) = properties.transport().link().rx_buff_size() {
            self = self.link_rx_buff_size(*v);
        }
        self = self.endpoint(LocatorConfig::from_config(properties)?);
        self = self.unicast(
            TransportManager::config_unicast()
                .from_config(properties)
                .await?,
        );
        self = self.multicast(
            TransportManagerBuilderMulticast::default()
                .from_config(properties)
                .await?,
        );

        Ok(self)
    }

    pub fn build(self, handler: Arc<dyn TransportEventHandler>) -> ZResult<TransportManager> {
        let unicast = self.unicast.build()?;
        let multicast = self.multicast.build()?;

        let config = TransportManagerConfig {
            version: self.version,
            zid: self.zid,
            whatami: self.whatami,
            sn_bytes: self.sn_bytes,
            batch_size: self.batch_size,
            defrag_buff_size: self.defrag_buff_size,
            link_rx_buff_size: self.link_rx_buff_size,
            unicast: unicast.config,
            multicast: multicast.config,
            endpoint: self.endpoint,
            handler,
        };

        let state = TransportManagerState {
            unicast: unicast.state,
            multicast: multicast.state,
        };

        let params = TransportManagerParams { config, state };

        Ok(TransportManager::new(params))
    }
}

impl Default for TransportManagerBuilder {
    fn default() -> Self {
        Self {
            version: VERSION,
            zid: ZenohId::rand(),
            whatami: ZN_MODE_DEFAULT.parse().unwrap(),
            sn_bytes: SeqNumBytes::default(),
            batch_size: BATCH_SIZE,
            defrag_buff_size: zparse!(ZN_DEFRAG_BUFF_SIZE_DEFAULT).unwrap(),
            link_rx_buff_size: zparse!(ZN_LINK_RX_BUFF_SIZE_DEFAULT).unwrap(),
            endpoint: HashMap::new(),
            unicast: TransportManagerBuilderUnicast::default(),
            multicast: TransportManagerBuilderMulticast::default(),
        }
    }
}

#[derive(Clone)]
pub struct TransportManager {
    pub config: Arc<TransportManagerConfig>,
    pub(crate) state: Arc<TransportManagerState>,
    pub(crate) prng: AsyncArc<AsyncMutex<PseudoRng>>,
    pub(crate) cipher: Arc<BlockCipher>,
    #[cfg(feature = "shared-memory")]
    pub(crate) shmr: Arc<RwLock<SharedMemoryReader>>,
}

impl TransportManager {
    pub fn new(params: TransportManagerParams) -> TransportManager {
        // Initialize the PRNG and the Cipher
        let mut prng = PseudoRng::from_entropy();
        let mut key = [0_u8; BlockCipher::BLOCK_SIZE];
        prng.fill_bytes(&mut key);
        let cipher = BlockCipher::new(key);

        TransportManager {
            config: Arc::new(params.config),
            state: Arc::new(params.state),
            prng: AsyncArc::new(AsyncMutex::new(prng)),
            cipher: Arc::new(cipher),
            #[cfg(feature = "shared-memory")]
            shmr: Arc::new(RwLock::new(SharedMemoryReader::new())),
        }
    }

    pub fn builder() -> TransportManagerBuilder {
        TransportManagerBuilder::default()
    }

    pub fn zid(&self) -> ZenohId {
        self.config.zid
    }

    pub async fn close(&self) {
        log::trace!("TransportManager::clear())");
        self.close_unicast().await;
        self.close_multicast().await;
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
    pub fn get_transport(&self, peer: &ZenohId) -> Option<TransportUnicast> {
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
