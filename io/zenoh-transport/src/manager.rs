//
// Copyright (c) 2023 ZettaScale Technology
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ZettaScale Zenoh Team, <zenoh@zettascale.tech>
//
use super::multicast::manager::{
    TransportManagerBuilderMulticast, TransportManagerConfigMulticast,
    TransportManagerStateMulticast,
};
use super::unicast::manager::{
    TransportManagerBuilderUnicast, TransportManagerConfigUnicast, TransportManagerStateUnicast,
};
use super::unicast::TransportUnicast;
use super::TransportEventHandler;
use async_std::sync::Mutex as AsyncMutex;
use rand::{RngCore, SeedableRng};
use std::collections::HashMap;
use std::sync::Arc;
#[cfg(feature = "shared-memory")]
use std::sync::RwLock;
use std::time::Duration;
use zenoh_cfg_properties::{config::*, Properties};
use zenoh_config::{Config, QueueConf, QueueSizeConf};
use zenoh_core::zparse;
use zenoh_crypto::{BlockCipher, PseudoRng};
use zenoh_link::NewLinkChannelSender;
use zenoh_protocol::{
    core::{EndPoint, Locator, Priority, WhatAmI, ZInt, ZenohId},
    defaults::{BATCH_SIZE, SEQ_NUM_RES, VERSION},
};
use zenoh_result::{bail, ZResult};
#[cfg(feature = "shared-memory")]
use zenoh_shm::SharedMemoryReader;

/// # Examples
/// ```
/// use std::sync::Arc;
/// use std::time::Duration;
/// use zenoh_protocol::core::{ZenohId, WhatAmI, whatami};
/// use zenoh_transport::*;
/// use zenoh_result::ZResult;
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
///         .keep_alive(4)      // Send a KeepAlive every 250 ms
///         .accept_timeout(Duration::from_secs(1))
///         .accept_pending(10) // Set to 10 the number of simultanous pending incoming transports        
///         .max_links(1)       // Allow max 1 inbound link per transport
///         .max_sessions(5);   // Allow max 5 transports open
/// let manager = TransportManager::builder()
///         .zid(ZenohId::rand())
///         .whatami(WhatAmI::Peer)
///         .batch_size(1_024)              // Use a batch size of 1024 bytes
///         .sn_resolution(128)             // Use a sequence number resolution of 128
///         .unicast(unicast)               // Configure unicast parameters
///         .build(Arc::new(MySH::default()))
///         .unwrap();
/// ```

pub struct TransportManagerConfig {
    pub version: u8,
    pub zid: ZenohId,
    pub whatami: WhatAmI,
    pub sn_resolution: ZInt,
    pub batch_size: u16,
    pub queue_size: [usize; Priority::NUM],
    pub queue_backoff: Duration,
    pub defrag_buff_size: usize,
    pub link_rx_buffer_size: usize,
    pub unicast: TransportManagerConfigUnicast,
    pub multicast: TransportManagerConfigMulticast,
    pub endpoint: HashMap<String, Properties>,
    pub handler: Arc<dyn TransportEventHandler>,
    pub tx_threads: usize,
    pub protocols: Vec<String>,
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
    version: u8,
    zid: ZenohId,
    whatami: WhatAmI,
    sn_resolution: ZInt,
    batch_size: u16,
    queue_size: QueueSizeConf,
    queue_backoff: Duration,
    defrag_buff_size: usize,
    link_rx_buffer_size: usize,
    unicast: TransportManagerBuilderUnicast,
    multicast: TransportManagerBuilderMulticast,
    endpoint: HashMap<String, Properties>,
    tx_threads: usize,
    protocols: Option<Vec<String>>,
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

    pub fn sn_resolution(mut self, sn_resolution: ZInt) -> Self {
        self.sn_resolution = sn_resolution;
        self
    }

    pub fn batch_size(mut self, batch_size: u16) -> Self {
        self.batch_size = batch_size;
        self
    }

    pub fn queue_size(mut self, queue_size: QueueSizeConf) -> Self {
        self.queue_size = queue_size;
        self
    }

    pub fn queue_backoff(mut self, queue_backoff: Duration) -> Self {
        self.queue_backoff = queue_backoff;
        self
    }

    pub fn defrag_buff_size(mut self, defrag_buff_size: usize) -> Self {
        self.defrag_buff_size = defrag_buff_size;
        self
    }

    pub fn link_rx_buffer_size(mut self, link_rx_buffer_size: usize) -> Self {
        self.link_rx_buffer_size = link_rx_buffer_size;
        self
    }

    pub fn endpoint(mut self, endpoint: HashMap<String, Properties>) -> Self {
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

    pub fn tx_threads(mut self, num: usize) -> Self {
        self.tx_threads = num;
        self
    }

    pub fn protocols(mut self, protocols: Option<Vec<String>>) -> Self {
        self.protocols = protocols;
        self
    }

    pub async fn from_config(mut self, config: &Config) -> ZResult<TransportManagerBuilder> {
        self = self.zid(*config.id());
        if let Some(v) = config.mode() {
            self = self.whatami(*v);
        }

        self = self.sn_resolution(
            config
                .transport()
                .link()
                .tx()
                .sequence_number_resolution()
                .unwrap(),
        );
        self = self.batch_size(config.transport().link().tx().batch_size().unwrap());
        self = self.defrag_buff_size(config.transport().link().rx().max_message_size().unwrap());
        self = self.link_rx_buffer_size(config.transport().link().rx().buffer_size().unwrap());
        self = self.queue_size(config.transport().link().tx().queue().size().clone());
        self = self.tx_threads(config.transport().link().tx().threads().unwrap());
        self = self.protocols(config.transport().link().protocols().clone());

        let (c, errors) = zenoh_link::LinkConfigurator::default()
            .configurations(config)
            .await;
        if !errors.is_empty() {
            use std::fmt::Write;
            let mut formatter = String::from("Some protocols reported configuration errors:\r\n");
            for (proto, err) in errors {
                write!(&mut formatter, "\t{proto}: {err}\r\n")?;
            }
            bail!("{}", formatter);
        }
        self = self.endpoint(c);
        self = self.unicast(
            TransportManagerBuilderUnicast::default()
                .from_config(config)
                .await?,
        );
        self = self.multicast(
            TransportManagerBuilderMulticast::default()
                .from_config(config)
                .await?,
        );

        Ok(self)
    }

    pub fn build(self, handler: Arc<dyn TransportEventHandler>) -> ZResult<TransportManager> {
        let unicast = self.unicast.build()?;
        let multicast = self.multicast.build()?;

        let mut queue_size = [0; Priority::NUM];
        queue_size[Priority::Control as usize] = *self.queue_size.control();
        queue_size[Priority::RealTime as usize] = *self.queue_size.real_time();
        queue_size[Priority::InteractiveHigh as usize] = *self.queue_size.interactive_high();
        queue_size[Priority::InteractiveLow as usize] = *self.queue_size.interactive_low();
        queue_size[Priority::DataHigh as usize] = *self.queue_size.data_high();
        queue_size[Priority::Data as usize] = *self.queue_size.data();
        queue_size[Priority::DataLow as usize] = *self.queue_size.data_low();
        queue_size[Priority::Background as usize] = *self.queue_size.background();

        let config = TransportManagerConfig {
            version: self.version,
            zid: self.zid,
            whatami: self.whatami,
            sn_resolution: self.sn_resolution,
            batch_size: self.batch_size,
            queue_size,
            queue_backoff: self.queue_backoff,
            defrag_buff_size: self.defrag_buff_size,
            link_rx_buffer_size: self.link_rx_buffer_size,
            unicast: unicast.config,
            multicast: multicast.config,
            endpoint: self.endpoint,
            handler,
            tx_threads: self.tx_threads,
            protocols: self.protocols.unwrap_or_else(|| {
                zenoh_link::PROTOCOLS
                    .iter()
                    .map(|x| x.to_string())
                    .collect()
            }),
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
        let queue = QueueConf::default();
        let backoff = queue.backoff().unwrap();
        Self {
            version: VERSION,
            zid: ZenohId::rand(),
            whatami: ZN_MODE_DEFAULT.parse().unwrap(),
            sn_resolution: SEQ_NUM_RES,
            batch_size: BATCH_SIZE,
            queue_size: queue.size,
            queue_backoff: Duration::from_nanos(backoff),
            defrag_buff_size: zparse!(ZN_DEFRAG_BUFF_SIZE_DEFAULT).unwrap(),
            link_rx_buffer_size: zparse!(ZN_LINK_RX_BUFF_SIZE_DEFAULT).unwrap(),
            endpoint: HashMap::new(),
            unicast: TransportManagerBuilderUnicast::default(),
            multicast: TransportManagerBuilderMulticast::default(),
            tx_threads: 1,
            protocols: None,
        }
    }
}

#[derive(Clone)]
pub(crate) struct TransportExecutor {
    executor: Arc<async_executor::Executor<'static>>,
    sender: async_std::channel::Sender<()>,
}

impl TransportExecutor {
    fn new(num_threads: usize) -> Self {
        let (sender, receiver) = async_std::channel::bounded(1);
        let executor = Arc::new(async_executor::Executor::new());
        for i in 0..num_threads {
            let exec = executor.clone();
            let recv = receiver.clone();
            std::thread::Builder::new()
                .name(format!("zenoh-tx-{}", i))
                .spawn(move || async_std::task::block_on(exec.run(recv.recv())))
                .unwrap();
        }
        Self { executor, sender }
    }

    async fn stop(&self) {
        let _ = self.sender.send(()).await;
    }

    pub(crate) fn spawn<T: Send + 'static>(
        &self,
        future: impl core::future::Future<Output = T> + Send + 'static,
    ) -> async_executor::Task<T> {
        self.executor.spawn(future)
    }
}

#[derive(Clone)]
pub struct TransportManager {
    pub config: Arc<TransportManagerConfig>,
    pub(crate) state: Arc<TransportManagerState>,
    pub(crate) prng: Arc<AsyncMutex<PseudoRng>>,
    pub(crate) cipher: Arc<BlockCipher>,
    #[cfg(feature = "shared-memory")]
    pub(crate) shmr: Arc<RwLock<SharedMemoryReader>>,
    pub(crate) locator_inspector: zenoh_link::LocatorInspector,
    pub(crate) new_unicast_link_sender: NewLinkChannelSender,
    pub(crate) tx_executor: TransportExecutor,
}

impl TransportManager {
    pub fn new(params: TransportManagerParams) -> TransportManager {
        // Initialize the PRNG and the Cipher
        let mut prng = PseudoRng::from_entropy();
        let mut key = [0_u8; BlockCipher::BLOCK_SIZE];
        prng.fill_bytes(&mut key);
        let cipher = BlockCipher::new(key);

        // @TODO: this should be moved into the unicast module
        let (new_unicast_link_sender, new_unicast_link_receiver) = flume::unbounded();

        let tx_threads = params.config.tx_threads;
        let this = TransportManager {
            config: Arc::new(params.config),
            state: Arc::new(params.state),
            prng: Arc::new(AsyncMutex::new(prng)),
            cipher: Arc::new(cipher),
            #[cfg(feature = "shared-memory")]
            shmr: Arc::new(RwLock::new(SharedMemoryReader::new())),
            locator_inspector: Default::default(),
            new_unicast_link_sender,
            tx_executor: TransportExecutor::new(tx_threads),
        };

        // @TODO: this should be moved into the unicast module
        async_std::task::spawn({
            let this = this.clone();
            async move {
                while let Ok(link) = new_unicast_link_receiver.recv_async().await {
                    this.handle_new_link_unicast(link).await;
                }
            }
        });

        this
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
        self.tx_executor.stop().await;
    }

    /*************************************/
    /*              LISTENER             */
    /*************************************/
    pub async fn add_listener(&self, endpoint: EndPoint) -> ZResult<Locator> {
        let p = endpoint.protocol();
        if !self
            .config
            .protocols
            .iter()
            .any(|x| x.as_str() == p.as_str())
        {
            bail!(
                "Unsupported protocol: {}. Supported protocols are: {:?}",
                p,
                self.config.protocols
            );
        }

        if self
            .locator_inspector
            .is_multicast(&endpoint.to_locator())
            .await?
        {
            // @TODO: multicast
            bail!("Unimplemented");
        } else {
            self.add_listener_unicast(endpoint).await
        }
    }

    pub async fn del_listener(&self, endpoint: &EndPoint) -> ZResult<()> {
        if self
            .locator_inspector
            .is_multicast(&endpoint.to_locator())
            .await?
        {
            // @TODO: multicast
            bail!("Unimplemented");
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
        let p = endpoint.protocol();
        if !self
            .config
            .protocols
            .iter()
            .any(|x| x.as_str() == p.as_str())
        {
            bail!(
                "Unsupported protocol: {}. Supported protocols are: {:?}",
                p,
                self.config.protocols
            );
        }

        if self
            .locator_inspector
            .is_multicast(&endpoint.to_locator())
            .await?
        {
            // @TODO: multicast
            bail!("Unimplemented");
        } else {
            self.open_transport_unicast(endpoint).await
        }
    }
}
