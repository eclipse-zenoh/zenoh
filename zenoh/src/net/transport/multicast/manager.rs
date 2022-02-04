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
use super::super::TransportManager;
use super::transport::TransportMulticastInner;
use super::*;
use crate::config::Config;
use crate::net::link::*;
use crate::net::protocol::message::CloseReason;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use zenoh_util::core::Result as ZResult;
use zenoh_util::properties::config::*;
use zenoh_util::{zerror, zlock, zparse};

pub struct TransportManagerConfigMulticast {
    pub lease: Duration,
    pub keep_alive: Duration,
    pub join_interval: Duration,
    pub max_sessions: usize,
    pub is_qos: bool,
}

pub struct TransportManagerBuilderMulticast {
    lease: Duration,
    keep_alive: Duration,
    join_interval: Duration,
    max_sessions: usize,
    is_qos: bool,
}

pub struct TransportManagerStateMulticast {
    // Established listeners
    pub(super) protocols: Arc<Mutex<HashMap<LocatorProtocol, LinkManagerMulticast>>>,
    // Established transports
    pub(super) transports: Arc<Mutex<HashMap<Locator, Arc<TransportMulticastInner>>>>,
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

    pub fn keep_alive(mut self, keep_alive: Duration) -> Self {
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

    pub async fn from_config(
        mut self,
        properties: &Config,
    ) -> ZResult<TransportManagerBuilderMulticast> {
        if let Some(v) = properties.transport().link().lease() {
            self = self.lease(Duration::from_millis(*v));
        }
        if let Some(v) = properties.transport().link().keep_alive() {
            self = self.keep_alive(Duration::from_millis(*v));
        }
        if let Some(v) = properties.transport().multicast().join_interval() {
            self = self.join_interval(Duration::from_millis(*v));
        }
        if let Some(v) = properties.transport().multicast().max_sessions() {
            self = self.max_sessions(*v);
        }
        if let Some(v) = properties.transport().qos() {
            self = self.qos(*v);
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
        };

        let state = TransportManagerStateMulticast {
            protocols: Arc::new(Mutex::new(HashMap::new())),
            transports: Arc::new(Mutex::new(HashMap::new())),
        };

        let params = TransportManagerParamsMulticast { config, state };

        Ok(params)
    }
}

impl Default for TransportManagerBuilderMulticast {
    fn default() -> TransportManagerBuilderMulticast {
        TransportManagerBuilderMulticast {
            lease: Duration::from_millis(zparse!(ZN_LINK_LEASE_DEFAULT).unwrap()),
            keep_alive: Duration::from_millis(zparse!(ZN_LINK_KEEP_ALIVE_DEFAULT).unwrap()),
            join_interval: Duration::from_millis(zparse!(ZN_JOIN_INTERVAL_DEFAULT).unwrap()),
            max_sessions: zparse!(ZN_MAX_SESSIONS_MULTICAST_DEFAULT).unwrap(),
            is_qos: zparse!(ZN_QOS_DEFAULT).unwrap(),
        }
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
            let _ = tm.close(CloseReason::Generic).await;
        }
    }

    /*************************************/
    /*            LINK MANAGER           */
    /*************************************/
    fn new_link_manager_multicast(
        &self,
        protocol: &LocatorProtocol,
    ) -> ZResult<LinkManagerMulticast> {
        let mut w_guard = zlock!(self.state.multicast.protocols);
        match w_guard.get(protocol) {
            Some(lm) => Ok(lm.clone()),
            None => {
                let lm = LinkManagerBuilderMulticast::make(protocol)?;
                w_guard.insert(protocol.clone(), lm.clone());
                Ok(lm)
            }
        }
    }

    fn del_link_manager_multicast(&self, protocol: &LocatorProtocol) -> ZResult<()> {
        match zlock!(self.state.multicast.protocols).remove(protocol) {
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
        if !endpoint.locator.address.is_multicast() {
            bail!(
                "Can not open a multicast transport with a unicast unicast: {}.",
                endpoint
            )
        }

        // Automatically create a new link manager for the protocol if it does not exist
        let manager = self.new_link_manager_multicast(&endpoint.locator.address.get_proto())?;
        // Fill and merge the endpoint configuration
        if let Some(config) = self
            .config
            .endpoint
            .get(&endpoint.locator.address.get_proto())
        {
            let config = match endpoint.config.as_ref() {
                Some(ec) => {
                    let mut config = config.clone();
                    for (k, v) in ec.iter() {
                        config.insert(k.clone(), v.clone());
                    }
                    config
                }
                None => config.clone(),
            };
            endpoint.config = Some(Arc::new(config));
        };

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

        let proto = locator.address.get_proto();
        if !guard.iter().any(|(l, _)| l.address.get_proto() == proto) {
            let _ = self.del_link_manager_multicast(&proto);
        }

        res.map(|_| ()).ok_or_else(|| {
            let e = zerror!("Can not delete the transport for locator: {}", locator);
            log::trace!("{}", e);
            e.into()
        })
    }
}
