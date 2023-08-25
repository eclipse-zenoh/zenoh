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
pub(crate) mod establishment;
pub(crate) mod link;
pub(crate) mod manager;
pub(crate) mod rx;
#[cfg(feature = "shared-memory")]
pub(crate) mod shm;
pub(crate) mod transport;
pub(crate) mod tx;

use super::common;
#[cfg(feature = "stats")]
use super::common::stats::stats_struct;
use crate::{TransportMulticastEventHandler, TransportPeer};
pub use manager::{
    TransportManagerBuilderMulticast, TransportManagerConfigMulticast,
    TransportManagerParamsMulticast,
};
use std::{
    fmt,
    sync::{Arc, Weak},
};
use transport::TransportMulticastInner;
use zenoh_core::{zcondfeat, zread};
use zenoh_link::{Link, LinkMulticast};
use zenoh_protocol::{
    core::Bits,
    network::NetworkMessage,
    transport::{close, PrioritySn},
};
use zenoh_result::{zerror, ZResult};

/*************************************/
/*              STATS                */
/*************************************/
#[cfg(feature = "stats")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "stats")]
use std::sync::atomic::{AtomicUsize, Ordering};
#[cfg(feature = "stats")]
stats_struct! {
    #[derive(Clone, Debug, Deserialize, Serialize)]
    pub struct TransportMulticastStats {
        pub tx_t_msgs,
        pub tx_n_msgs,
        pub tx_n_dropped,
        pub tx_z_put_user_msgs,
        pub tx_z_put_user_pl_bytes,
        // pub tx_z_put_admin_msgs,
        // pub tx_z_put_admin_pl_bytes,
        pub tx_z_del_user_msgs,
        // pub tx_z_del_admin_msgs,
        pub tx_z_query_user_msgs,
        pub tx_z_query_user_pl_bytes,
        // pub tx_z_query_admin_msgs,
        // pub tx_z_query_admin_pl_bytes,
        pub tx_z_reply_user_msgs,
        pub tx_z_reply_user_pl_bytes,
        // pub tx_z_reply_admin_msgs,
        // pub tx_z_reply_admin_pl_bytes,
        pub tx_bytes,

        pub rx_t_msgs,
        pub rx_n_msgs,
        pub rx_z_put_user_msgs,
        pub rx_z_put_user_pl_bytes,
        // pub rx_z_put_admin_msgs,
        // pub rx_z_put_admin_pl_bytes,
        pub rx_z_del_user_msgs,
        // pub rx_z_del_admin_msgs,
        pub rx_z_query_user_msgs,
        pub rx_z_query_user_pl_bytes,
        // pub rx_z_query_admin_msgs,
        // pub rx_z_query_admin_pl_bytes,
        pub rx_z_reply_user_msgs,
        pub rx_z_reply_user_pl_bytes,
        // pub rx_z_reply_admin_msgs,
        // pub rx_z_reply_admin_pl_bytes,
        pub rx_bytes,
    }
}

/*************************************/
/*       TRANSPORT MULTICAST         */
/*************************************/
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TransportConfigMulticast {
    pub(crate) sn_resolution: Bits,
    pub(crate) initial_sns: Box<[PrioritySn]>,
    pub(crate) link: LinkMulticast,
    #[cfg(feature = "shared-memory")]
    pub(crate) is_shm: bool,
}

#[derive(Clone)]
pub struct TransportMulticast(Weak<TransportMulticastInner>);

impl TransportMulticast {
    #[inline(always)]
    fn get_transport(&self) -> ZResult<Arc<TransportMulticastInner>> {
        self.0
            .upgrade()
            .ok_or_else(|| zerror!("Transport multicast closed").into())
    }

    #[inline(always)]
    pub fn get_sn_resolution(&self) -> ZResult<Bits> {
        let transport = self.get_transport()?;
        Ok(transport.get_sn_resolution())
    }

    #[cfg(feature = "shared-memory")]
    #[inline(always)]
    pub fn is_shm(&self) -> ZResult<bool> {
        let transport = self.get_transport()?;
        Ok(transport.is_shm())
    }

    #[inline(always)]
    pub fn is_qos(&self) -> ZResult<bool> {
        let transport = self.get_transport()?;
        Ok(transport.is_qos())
    }

    #[inline(always)]
    pub fn get_callback(&self) -> ZResult<Option<Arc<dyn TransportMulticastEventHandler>>> {
        let transport = self.get_transport()?;
        Ok(transport.get_callback())
    }

    #[inline(always)]
    pub fn get_link(&self) -> ZResult<Link> {
        let transport = self.get_transport()?;
        Ok(transport.get_link().into())
    }

    #[inline(always)]
    pub fn get_peers(&self) -> ZResult<Vec<TransportPeer>> {
        let transport = self.get_transport()?;
        Ok(transport.get_peers())
    }

    #[inline(always)]
    pub async fn close(&self) -> ZResult<()> {
        // Return Ok if the transport has already been closed
        match self.get_transport() {
            Ok(transport) => transport.close(close::reason::GENERIC).await,
            Err(_) => Ok(()),
        }
    }

    #[inline(always)]
    pub fn schedule(&self, message: NetworkMessage) -> ZResult<()> {
        let transport = self.get_transport()?;
        transport.schedule(message);
        Ok(())
    }

    #[inline(always)]
    pub fn handle_message(&self, message: NetworkMessage) -> ZResult<()> {
        self.schedule(message)
    }

    #[cfg(feature = "stats")]
    pub fn get_stats(&self) -> ZResult<TransportMulticastStats> {
        Ok(self.get_transport()?.stats.snapshot())
    }
}

impl From<&Arc<TransportMulticastInner>> for TransportMulticast {
    fn from(s: &Arc<TransportMulticastInner>) -> TransportMulticast {
        TransportMulticast(Arc::downgrade(s))
    }
}

impl Eq for TransportMulticast {}

impl PartialEq for TransportMulticast {
    fn eq(&self, other: &Self) -> bool {
        Weak::ptr_eq(&self.0, &other.0)
    }
}

impl fmt::Debug for TransportMulticast {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.get_transport() {
            Ok(transport) => {
                let is_shm = zcondfeat!("shared-memory", transport.is_shm(), false);
                let peers: String = zread!(transport.peers)
                    .iter()
                    .map(|(l, p)| {
                        format!("(locator: {}, zid: {}, whatami: {})", l, p.zid, p.whatami)
                    })
                    .collect();

                f.debug_struct("Transport Multicast")
                    .field("sn_resolution", &transport.get_sn_resolution())
                    .field("is_qos", &transport.is_qos())
                    .field("is_shm", &is_shm)
                    .field("peers", &peers)
                    .finish()
            }
            Err(e) => {
                write!(f, "{e}")
            }
        }
    }
}
