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
use std::{collections::HashMap, sync::Arc, time::Duration};

use rand::{RngCore, SeedableRng};
use tokio::sync::Mutex as AsyncMutex;
use zenoh_config::{Config, LinkRxConf, QueueConf, QueueSizeConf};
use zenoh_crypto::{BlockCipher, PseudoRng};
use zenoh_link::NewLinkChannelSender;
use zenoh_protocol::{
    core::{EndPoint, Field, Locator, Priority, Resolution, WhatAmI, ZenohIdProto},
    transport::BatchSize,
    VERSION,
};
use zenoh_result::{bail, ZResult};
#[cfg(feature = "shared-memory")]
use zenoh_shm::api::client_storage::GLOBAL_CLIENT_STORAGE;
#[cfg(feature = "shared-memory")]
use zenoh_shm::reader::ShmReader;
use zenoh_task::TaskController;

use super::{
    unicast::manager::{
        TransportManagerBuilderUnicast, TransportManagerConfigUnicast, TransportManagerStateUnicast,
    },
    TransportEventHandler,
};
use crate::multicast::manager::{
    TransportManagerBuilderMulticast, TransportManagerConfigMulticast,
    TransportManagerStateMulticast,
};

/// # Examples
/// ```
/// use std::sync::Arc;
/// use std::time::Duration;
/// use zenoh_protocol::core::{ZenohIdProto, Resolution, Field, Bits, WhatAmI, whatami};
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
///         _transport: unicast::TransportUnicast
///     ) -> ZResult<Arc<dyn TransportPeerEventHandler>> {
///         Ok(Arc::new(DummyTransportPeerEventHandler))
///     }
///
///     fn new_multicast(
///         &self,
///         _transport: multicast::TransportMulticast,
///     ) -> ZResult<Arc<dyn TransportMulticastEventHandler>> {
///         Ok(Arc::new(DummyTransportMulticastEventHandler))
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
///         .accept_pending(10) // Set to 10 the number of simultaneous pending incoming transports
///         .max_sessions(5);   // Allow max 5 transports open
/// let mut resolution = Resolution::default();
/// resolution.set(Field::FrameSN, Bits::U8);
/// let manager = TransportManager::builder()
///         .zid(ZenohIdProto::rand().into())
///         .whatami(WhatAmI::Peer)
///         .batch_size(1_024)              // Use a batch size of 1024 bytes
///         .resolution(resolution)         // Use a sequence number resolution of 128
///         .unicast(unicast)               // Configure unicast parameters
///         .build(Arc::new(MySH::default()))
///         .unwrap();
/// ```

pub struct TransportManagerConfig {
    pub version: u8,
    pub zid: ZenohIdProto,
    pub whatami: WhatAmI,
    pub resolution: Resolution,
    pub batch_size: BatchSize,
    pub batching: bool,
    pub wait_before_drop: Duration,
    pub queue_size: [usize; Priority::NUM],
    pub queue_backoff: Duration,
    pub defrag_buff_size: usize,
    pub link_rx_buffer_size: usize,
    pub unicast: TransportManagerConfigUnicast,
    pub multicast: TransportManagerConfigMulticast,
    pub endpoints: HashMap<String, String>, // (protocol, config)
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
    zid: ZenohIdProto,
    whatami: WhatAmI,
    resolution: Resolution,
    batch_size: BatchSize,
    batching: bool,
    wait_before_drop: Duration,
    queue_size: QueueSizeConf,
    queue_backoff: Duration,
    defrag_buff_size: usize,
    link_rx_buffer_size: usize,
    unicast: TransportManagerBuilderUnicast,
    multicast: TransportManagerBuilderMulticast,
    endpoints: HashMap<String, String>, // (protocol, config)
    tx_threads: usize,
    protocols: Option<Vec<String>>,
    #[cfg(feature = "shared-memory")]
    shm_reader: Option<ShmReader>,
}

impl TransportManagerBuilder {
    #[cfg(feature = "shared-memory")]
    pub fn shm_reader(mut self, shm_reader: Option<ShmReader>) -> Self {
        self.shm_reader = shm_reader;
        self
    }

    pub fn zid(mut self, zid: ZenohIdProto) -> Self {
        self.zid = zid;
        self
    }

    pub fn whatami(mut self, whatami: WhatAmI) -> Self {
        self.whatami = whatami;
        self
    }

    pub fn resolution(mut self, resolution: Resolution) -> Self {
        self.resolution = resolution;
        self
    }

    pub fn batch_size(mut self, batch_size: BatchSize) -> Self {
        self.batch_size = batch_size;
        self
    }

    pub fn batching(mut self, batching: bool) -> Self {
        self.batching = batching;
        self
    }

    pub fn wait_before_drop(mut self, wait_before_drop: Duration) -> Self {
        self.wait_before_drop = wait_before_drop;
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

    pub fn endpoints(mut self, endpoints: HashMap<String, String>) -> Self {
        self.endpoints = endpoints;
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
        self = self.zid((*config.id()).into());
        if let Some(v) = config.mode() {
            self = self.whatami(*v);
        }

        let link = config.transport().link();
        let mut resolution = Resolution::default();
        resolution.set(Field::FrameSN, *link.tx().sequence_number_resolution());
        self = self.resolution(resolution);
        self = self.batch_size(*link.tx().batch_size());
        self = self.batching(*link.tx().batching());
        self = self.defrag_buff_size(*link.rx().max_message_size());
        self = self.link_rx_buffer_size(*link.rx().buffer_size());
        self = self.wait_before_drop(Duration::from_micros(
            *link.tx().queue().congestion_control().wait_before_drop(),
        ));
        self = self.queue_size(link.tx().queue().size().clone());
        self = self.queue_backoff(Duration::from_nanos(*link.tx().queue().backoff()));
        self = self.tx_threads(*link.tx().threads());
        self = self.protocols(link.protocols().clone());

        let (c, errors) = zenoh_link::LinkConfigurator::default().configurations(config);
        if !errors.is_empty() {
            use std::fmt::Write;
            let mut formatter = String::from("Some protocols reported configuration errors:\r\n");
            for (proto, err) in errors {
                write!(&mut formatter, "\t{proto}: {err}\r\n")?;
            }
            bail!("{}", formatter);
        }
        self = self.endpoints(c);
        self = self.unicast(
            TransportManagerBuilderUnicast::default()
                .from_config(config)
                .await?,
        );
        self = self.multicast(TransportManagerBuilderMulticast::default().from_config(config)?);

        Ok(self)
    }

    pub fn build(self, handler: Arc<dyn TransportEventHandler>) -> ZResult<TransportManager> {
        // Initialize the PRNG and the Cipher
        let mut prng = PseudoRng::from_entropy();

        #[cfg(feature = "shared-memory")]
        let shm_reader = self
            .shm_reader
            .unwrap_or_else(|| ShmReader::new(GLOBAL_CLIENT_STORAGE.clone()));

        let unicast = self.unicast.build(
            &mut prng,
            #[cfg(feature = "shared-memory")]
            &shm_reader,
        )?;
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
            resolution: self.resolution,
            batch_size: self.batch_size,
            batching: self.batching,
            wait_before_drop: self.wait_before_drop,
            queue_size,
            queue_backoff: self.queue_backoff,
            defrag_buff_size: self.defrag_buff_size,
            link_rx_buffer_size: self.link_rx_buffer_size,
            unicast: unicast.config,
            multicast: multicast.config,
            endpoints: self.endpoints,
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

        Ok(TransportManager::new(
            params,
            prng,
            #[cfg(feature = "shared-memory")]
            shm_reader,
        ))
    }
}

impl Default for TransportManagerBuilder {
    fn default() -> Self {
        let link_rx = LinkRxConf::default();
        let queue = QueueConf::default();
        let backoff = *queue.backoff();
        let wait_before_drop = *queue.congestion_control().wait_before_drop();
        Self {
            version: VERSION,
            zid: ZenohIdProto::rand(),
            whatami: zenoh_config::defaults::mode,
            resolution: Resolution::default(),
            batch_size: BatchSize::MAX,
            batching: true,
            wait_before_drop: Duration::from_micros(wait_before_drop),
            queue_size: queue.size,
            queue_backoff: Duration::from_nanos(backoff),
            defrag_buff_size: *link_rx.max_message_size(),
            link_rx_buffer_size: *link_rx.buffer_size(),
            endpoints: HashMap::new(),
            unicast: TransportManagerBuilderUnicast::default(),
            multicast: TransportManagerBuilderMulticast::default(),
            tx_threads: 1,
            protocols: None,
            #[cfg(feature = "shared-memory")]
            shm_reader: None,
        }
    }
}

#[derive(Clone)]
pub struct TransportManager {
    pub config: Arc<TransportManagerConfig>,
    pub(crate) state: Arc<TransportManagerState>,
    pub(crate) prng: Arc<AsyncMutex<PseudoRng>>,
    pub(crate) cipher: Arc<BlockCipher>,
    pub(crate) locator_inspector: zenoh_link::LocatorInspector,
    pub(crate) new_unicast_link_sender: NewLinkChannelSender,
    #[cfg(feature = "shared-memory")]
    pub(crate) shmr: ShmReader,
    #[cfg(feature = "stats")]
    pub(crate) stats: Arc<crate::stats::TransportStats>,
    pub(crate) task_controller: TaskController,
}

impl TransportManager {
    pub fn new(
        params: TransportManagerParams,
        mut prng: PseudoRng,
        #[cfg(feature = "shared-memory")] shmr: ShmReader,
    ) -> TransportManager {
        // Initialize the Cipher
        let mut key = [0_u8; BlockCipher::BLOCK_SIZE];
        prng.fill_bytes(&mut key);
        let cipher = BlockCipher::new(key);

        // @TODO: this should be moved into the unicast module
        let (new_unicast_link_sender, new_unicast_link_receiver) = flume::unbounded();

        let this = TransportManager {
            config: Arc::new(params.config),
            state: Arc::new(params.state),
            prng: Arc::new(AsyncMutex::new(prng)),
            cipher: Arc::new(cipher),
            locator_inspector: Default::default(),
            new_unicast_link_sender,
            #[cfg(feature = "stats")]
            stats: std::sync::Arc::new(crate::stats::TransportStats::default()),
            #[cfg(feature = "shared-memory")]
            shmr,
            task_controller: TaskController::default(),
        };

        // @TODO: this should be moved into the unicast module
        let cancellation_token = this.task_controller.get_cancellation_token();
        this.task_controller
            .spawn_with_rt(zenoh_runtime::ZRuntime::Net, {
                let this = this.clone();
                async move {
                    loop {
                        tokio::select! {
                                res = new_unicast_link_receiver.recv_async() => {
                                    if let Ok(link) = res {
                                        this.handle_new_link_unicast(link).await;
                                    }
                                }
                                _ = cancellation_token.cancelled() => { break; }
                        }
                    }
                }
            });

        this
    }

    pub fn builder() -> TransportManagerBuilder {
        TransportManagerBuilder::default()
    }

    pub fn zid(&self) -> ZenohIdProto {
        self.config.zid
    }

    #[cfg(feature = "stats")]
    pub fn get_stats(&self) -> std::sync::Arc<crate::stats::TransportStats> {
        self.stats.clone()
    }

    pub async fn close(&self) {
        self.close_unicast().await;
        self.task_controller
            .terminate_all_async(Duration::from_secs(10))
            .await;
    }

    /*************************************/
    /*              LISTENER             */
    /*************************************/
    pub async fn add_listener(&self, endpoint: EndPoint) -> ZResult<Locator> {
        if self
            .locator_inspector
            .is_multicast(&endpoint.to_locator())
            .await?
        {
            self.add_listener_multicast(endpoint).await
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
            self.del_listener_multicast(endpoint).await
        } else {
            self.del_listener_unicast(endpoint).await
        }
    }

    pub async fn get_listeners(&self) -> Vec<EndPoint> {
        let mut lsu = self.get_listeners_unicast().await;
        let mut lsm = self.get_listeners_multicast().await;
        lsu.append(&mut lsm);
        lsu
    }

    // TODO(yuyuan): Can we make this async as above?
    pub fn get_locators(&self) -> Vec<Locator> {
        let mut lsu = zenoh_runtime::ZRuntime::Net.block_in_place(self.get_locators_unicast());
        let mut lsm = zenoh_runtime::ZRuntime::Net.block_in_place(self.get_locators_multicast());
        lsu.append(&mut lsm);
        lsu
    }
}
