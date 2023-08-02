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
#[cfg(feature = "shared-memory")]
use crate::multicast::shm::SharedMemoryMulticast;
use crate::multicast::{transport::TransportMulticastInner, TransportMulticast};
use crate::TransportManager;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
#[cfg(feature = "shared-memory")]
use zenoh_config::SharedMemoryConf;
use zenoh_config::{Config, LinkTxConf};
use zenoh_core::zlock;
use zenoh_link::*;
use zenoh_protocol::{
    core::endpoint::{self, Protocol},
    transport::close,
};
use zenoh_result::{bail, zerror, ZResult};

pub struct TransportManagerConfigMulticast {
    pub lease: Duration,
    pub keep_alive: usize,
    pub join_interval: Duration,
    pub max_sessions: usize,
    pub is_qos: bool,
    #[cfg(feature = "shared-memory")]
    pub is_shm: bool,
}

pub struct TransportManagerBuilderMulticast {
    lease: Duration,
    keep_alive: usize,
    join_interval: Duration,
    max_sessions: usize,
    is_qos: bool,
    #[cfg(feature = "shared-memory")]
    is_shm: bool,
}

pub struct TransportManagerStateMulticast {
    // Established listeners
    pub(crate) protocols: Arc<Mutex<HashMap<String, LinkManagerMulticast>>>,
    // Established transports
    pub(crate) transports: Arc<Mutex<HashMap<Locator, Arc<TransportMulticastInner>>>>,
    // Shared memory
    #[cfg(feature = "shared-memory")]
    pub(super) shm: Arc<SharedMemoryMulticast>,
}

pub struct TransportManagerParamsMulticast {
    pub config: TransportManagerConfigMulticast,
    pub state: TransportManagerStateMulticast,
}

impl TransportManagerBuilderMulticast {
    pub fn lease(mut self, lease: Duration) -> Self {
        self.lease = lease;
        self
    }

    pub fn keep_alive(mut self, keep_alive: usize) -> Self {
        self.keep_alive = keep_alive;
        self
    }

    pub fn join_interval(mut self, join_interval: Duration) -> Self {
        self.join_interval = join_interval;
        self
    }

    pub fn max_sessions(mut self, max_sessions: usize) -> Self {
        self.max_sessions = max_sessions;
        self
    }

    pub fn qos(mut self, is_qos: bool) -> Self {
        self.is_qos = is_qos;
        self
    }

    #[cfg(feature = "shared-memory")]
    pub fn shm(mut self, is_shm: bool) -> Self {
        self.is_shm = is_shm;
        self
    }

    pub async fn from_config(
        mut self,
        config: &Config,
    ) -> ZResult<TransportManagerBuilderMulticast> {
        self = self.lease(Duration::from_millis(
            *config.transport().link().tx().lease(),
        ));
        self = self.keep_alive(*config.transport().link().tx().keep_alive());
        self = self.join_interval(Duration::from_millis(
            config.transport().multicast().join_interval().unwrap(),
        ));
        self = self.max_sessions(config.transport().multicast().max_sessions().unwrap());
        // @TODO: Force QoS deactivation in multicast since it is not supported
        // self = self.qos(*config.transport().qos().enabled());
        self = self.qos(false);
        #[cfg(feature = "shared-memory")]
        {
            self = self.shm(*config.transport().shared_memory().enabled());
        }

        Ok(self)
    }

    pub fn build(self) -> ZResult<TransportManagerParamsMulticast> {
        let config = TransportManagerConfigMulticast {
            lease: self.lease,
            keep_alive: self.keep_alive,
            join_interval: self.join_interval,
            max_sessions: self.max_sessions,
            is_qos: self.is_qos,
            #[cfg(feature = "shared-memory")]
            is_shm: self.is_shm,
        };

        let state = TransportManagerStateMulticast {
            protocols: Arc::new(Mutex::new(HashMap::new())),
            transports: Arc::new(Mutex::new(HashMap::new())),
            #[cfg(feature = "shared-memory")]
            shm: Arc::new(SharedMemoryMulticast::make()?),
        };

        let params = TransportManagerParamsMulticast { config, state };

        Ok(params)
    }
}

impl Default for TransportManagerBuilderMulticast {
    fn default() -> TransportManagerBuilderMulticast {
        let link_tx = LinkTxConf::default();
        #[cfg(feature = "shared-memory")]
        let shm = SharedMemoryConf::default();

        let tmb = TransportManagerBuilderMulticast {
            lease: Duration::from_millis(*link_tx.lease()),
            keep_alive: *link_tx.keep_alive(),
            join_interval: Duration::from_millis(0),
            max_sessions: 0,
            is_qos: false,
            #[cfg(feature = "shared-memory")]
            is_shm: *shm.enabled(),
        };
        async_std::task::block_on(tmb.from_config(&Config::default())).unwrap()
    }
}

impl TransportManager {
    pub fn config_multicast() -> TransportManagerBuilderMulticast {
        TransportManagerBuilderMulticast::default()
    }

    pub async fn close_multicast(&self) {
        log::trace!("TransportManagerMulticast::clear())");

        zlock!(self.state.multicast.protocols).clear();

        let mut tm_guard = zlock!(self.state.multicast.transports)
            .drain()
            .map(|(_, v)| v)
            .collect::<Vec<Arc<TransportMulticastInner>>>();
        for tm in tm_guard.drain(..) {
            let _ = tm.close(close::reason::GENERIC).await;
        }
    }

    /*************************************/
    /*            LINK MANAGER           */
    /*************************************/
    fn new_link_manager_multicast(&self, protocol: &Protocol) -> ZResult<LinkManagerMulticast> {
        let mut w_guard = zlock!(self.state.multicast.protocols);
        match w_guard.get(protocol.as_str()) {
            Some(lm) => Ok(lm.clone()),
            None => {
                let lm = LinkManagerBuilderMulticast::make(protocol.as_str())?;
                w_guard.insert(protocol.to_string(), lm.clone());
                Ok(lm)
            }
        }
    }

    fn _del_link_manager_multicast(&self, protocol: &Protocol) -> ZResult<()> {
        match zlock!(self.state.multicast.protocols).remove(protocol.as_str()) {
            Some(_) => Ok(()),
            None => bail!(
                "Can not delete the link manager for protocol ({}) because it has not been found.",
                protocol
            ),
        }
    }

    /*************************************/
    /*             TRANSPORT             */
    /*************************************/
    pub async fn open_transport_multicast(
        &self,
        mut endpoint: EndPoint,
    ) -> ZResult<TransportMulticast> {
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
        if !self
            .locator_inspector
            .is_multicast(&endpoint.to_locator())
            .await?
        {
            bail!(
                "Can not open a multicast transport with a unicast unicast: {}.",
                endpoint
            )
        }

        // Automatically create a new link manager for the protocol if it does not exist
        let manager = self.new_link_manager_multicast(&endpoint.protocol())?;
        // Fill and merge the endpoint configuration
        if let Some(config) = self.config.endpoints.get(endpoint.protocol().as_str()) {
            endpoint
                .config_mut()
                .extend(endpoint::Parameters::iter(config))?;
        }

        // Open the link
        let link = manager.new_link(&endpoint).await?;
        super::establishment::open_link(self, link).await
    }

    pub fn get_transport_multicast(&self, locator: &Locator) -> Option<TransportMulticast> {
        zlock!(self.state.multicast.transports)
            .get(locator)
            .map(|t| t.into())
    }

    pub fn get_transports_multicast(&self) -> Vec<TransportMulticast> {
        zlock!(self.state.multicast.transports)
            .values()
            .map(|t| t.into())
            .collect()
    }

    pub(super) fn del_transport_multicast(&self, locator: &Locator) -> ZResult<()> {
        let mut guard = zlock!(self.state.multicast.transports);
        let res = guard.remove(locator);

        let proto = locator.protocol();
        if !guard.iter().any(|(l, _)| l.protocol() == proto) {
            let _ = self._del_link_manager_multicast(&proto);
        }

        res.map(|_| ()).ok_or_else(|| {
            let e = zerror!("Can not delete the transport for locator: {}", locator);
            log::trace!("{}", e);
            e.into()
        })
    }
}
