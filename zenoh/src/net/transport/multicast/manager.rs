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
use super::defaults::*;
use super::transport::TransportMulticastInner;
use super::*;
use crate::net::link::*;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::properties::config::ConfigProperties;
use zenoh_util::properties::config::*;
use zenoh_util::{zerror, zlock};

pub struct TransportManagerConfigMulticast {
    pub lease: Duration,
    pub keep_alive: Duration,
    pub join_interval: Duration,
    pub max_sessions: usize,
    pub is_qos: bool,
}

impl Default for TransportManagerConfigMulticast {
    fn default() -> Self {
        Self::builder().build()
    }
}

impl TransportManagerConfigMulticast {
    pub fn builder() -> TransportManagerConfigBuilderMulticast {
        TransportManagerConfigBuilderMulticast::default()
    }
}

pub struct TransportManagerConfigBuilderMulticast {
    lease: Duration,
    keep_alive: Duration,
    join_interval: Duration,
    max_sessions: usize,
    is_qos: bool,
}

impl Default for TransportManagerConfigBuilderMulticast {
    fn default() -> TransportManagerConfigBuilderMulticast {
        TransportManagerConfigBuilderMulticast {
            lease: Duration::from_millis(*ZN_LINK_LEASE),
            keep_alive: Duration::from_millis(*ZN_LINK_KEEP_ALIVE),
            join_interval: Duration::from_millis(*ZN_JOIN_INTERVAL),
            max_sessions: usize::MAX,
            is_qos: true,
        }
    }
}

impl TransportManagerConfigBuilderMulticast {
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

    pub async fn from_properties(
        mut self,
        properties: &ConfigProperties,
    ) -> ZResult<TransportManagerConfigBuilderMulticast> {
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

        if let Some(v) = properties.get(&ZN_LINK_LEASE_KEY) {
            self = self.lease(Duration::from_millis(zparse!(v)?));
        }
        if let Some(v) = properties.get(&ZN_LINK_KEEP_ALIVE_KEY) {
            self = self.keep_alive(Duration::from_millis(zparse!(v)?));
        }
        if let Some(v) = properties.get(&ZN_JOIN_INTERVAL_KEY) {
            self = self.join_interval(Duration::from_millis(zparse!(v)?));
        }
        if let Some(v) = properties.get(&ZN_MAX_SESSIONS_KEY) {
            self = self.max_sessions(zparse!(v)?);
        }
        if let Some(v) = properties.get(&ZN_QOS_KEY) {
            self = self.qos(zparse!(v)?);
        }

        Ok(self)
    }

    pub fn build(self) -> TransportManagerConfigMulticast {
        TransportManagerConfigMulticast {
            lease: self.lease,
            keep_alive: self.keep_alive,
            join_interval: self.join_interval,
            max_sessions: self.max_sessions,
            is_qos: self.is_qos,
        }
    }
}

pub struct TransportManagerStateMulticast {
    // Established listeners
    pub(super) protocols: Arc<Mutex<HashMap<LocatorProtocol, LinkManagerMulticast>>>,
    // Established transports
    pub(super) transports: Arc<Mutex<HashMap<Locator, Arc<TransportMulticastInner>>>>,
}

impl Default for TransportManagerStateMulticast {
    fn default() -> TransportManagerStateMulticast {
        TransportManagerStateMulticast {
            protocols: Arc::new(Mutex::new(HashMap::new())),
            transports: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl TransportManager {
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

    // fn get_link_manager_multicast(
    //     &self,
    //     protocol: &LocatorProtocol,
    // ) -> ZResult<LinkManagerMulticast> {
    //     match zlock!(self.state.multicast.protocols).get(protocol) {
    //         Some(manager) => Ok(manager.clone()),
    //         None => zerror!(ZErrorKind::Other {
    //             descr: format!(
    //                 "Can not get the link manager for protocol ({}) because it has not been found",
    //                 protocol
    //             )
    //         }),
    //     }
    // }

    fn del_link_manager_multicast(&self, protocol: &LocatorProtocol) -> ZResult<()> {
        match zlock!(self.state.multicast.protocols).remove(protocol) {
            Some(_) => Ok(()),
            None => zerror!(ZErrorKind::Other {
                descr: format!("Can not delete the link manager for protocol ({}) because it has not been found.", protocol)
            })
        }
    }

    /*************************************/
    /*             TRANSPORT             */
    /*************************************/
    pub async fn open_transport_multicast(&self, locator: &Locator) -> ZResult<TransportMulticast> {
        if !locator.is_multicast() {
            return zerror!(ZErrorKind::InvalidLocator {
                descr: format!(
                    "Can not open a multicast transport with a unicast locator: {}.",
                    locator
                )
            });
        }

        // Automatically create a new link manager for the protocol if it does not exist
        let manager = self.new_link_manager_multicast(&locator.get_proto())?;
        let ps = self.config.locator_property.get(&locator.get_proto());
        // Open the multicast link throught the link manager
        let link = manager.new_link(locator, ps).await?;
        // Open the link
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

    pub(super) async fn del_transport_multicast(&self, locator: &Locator) -> ZResult<()> {
        let mut guard = zlock!(self.state.multicast.transports);
        let res = guard.remove(locator);

        let proto = locator.get_proto();
        if !guard.iter().any(|(l, _)| l.get_proto() == proto) {
            let _ = self.del_link_manager_multicast(&proto);
        }

        res.map(|_| ()).ok_or_else(|| {
            let e = format!("Can not delete the transport for locator: {}", locator);
            log::trace!("{}", e);
            zerror2!(ZErrorKind::Other { descr: e })
        })
    }
}
