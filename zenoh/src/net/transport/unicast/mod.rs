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
pub mod establishment;
pub(crate) mod link;
pub(crate) mod manager;
pub(crate) mod rx;
pub(crate) mod transport;
pub(crate) mod tx;

use super::common;
#[cfg(feature = "stats")]
use super::common::stats::stats_struct;
use super::protocol;
use super::protocol::core::{SeqNumBytes, WhatAmI, ZInt, ZenohId};
use super::protocol::message::{CloseReason, ZenohMessage};
use super::{TransportPeer, TransportPeerEventHandler};
use crate::net::link::Link;
pub use manager::*;
use std::fmt;
use std::sync::{Arc, Weak};
use transport::TransportUnicastInner;
use zenoh_util::core::Result as ZResult;
use zenoh_util::zerror;

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
    pub struct TransportUnicastStats {
        pub tx_t_msgs,
        pub tx_z_msgs,
        pub tx_z_dropped,
        pub tx_z_data_msgs,
        pub tx_z_data_payload_bytes,
        pub tx_z_data_reply_msgs,
        pub tx_z_data_reply_payload_bytes,
        pub tx_z_pull_msgs,
        pub tx_z_query_msgs,
        pub tx_z_declare_msgs,
        pub tx_z_linkstate_msgs,
        pub tx_z_unit_msgs,
        pub tx_z_unit_reply_msgs,
        pub tx_bytes,
        pub rx_t_msgs,
        pub rx_z_msgs,
        pub rx_z_data_msgs,
        pub rx_z_data_payload_bytes,
        pub rx_z_data_reply_msgs,
        pub rx_z_data_reply_payload_bytes,
        pub rx_z_pull_msgs,
        pub rx_z_query_msgs,
        pub rx_z_declare_msgs,
        pub rx_z_linkstate_msgs,
        pub rx_z_unit_msgs,
        pub rx_z_unit_reply_msgs,
        pub rx_bytes,
    }
}

/*************************************/
/*        TRANSPORT UNICAST          */
/*************************************/
#[derive(Clone, Copy)]
pub(crate) struct TransportConfigUnicast {
    pub(crate) peer: ZenohId,
    pub(crate) whatami: WhatAmI,
    pub(crate) sn_bytes: SeqNumBytes,
    pub(crate) initial_sn_tx: ZInt,
    pub(crate) is_shm: bool,
    pub(crate) is_qos: bool,
}

/// [`TransportUnicast`] is the transport handler returned
/// when opening a new unicast transport
#[derive(Clone)]
pub struct TransportUnicast(Weak<TransportUnicastInner>);

impl TransportUnicast {
    #[inline(always)]
    pub(super) fn get_inner(&self) -> ZResult<Arc<TransportUnicastInner>> {
        self.0
            .upgrade()
            .ok_or_else(|| zerror!("Transport unicast closed").into())
    }

    #[inline(always)]
    pub fn get_zid(&self) -> ZResult<ZenohId> {
        let transport = self.get_inner()?;
        Ok(transport.get_zid())
    }

    #[inline(always)]
    pub fn get_whatami(&self) -> ZResult<WhatAmI> {
        let transport = self.get_inner()?;
        Ok(transport.get_whatami())
    }

    #[inline(always)]
    pub fn get_sn_bytes(&self) -> ZResult<SeqNumBytes> {
        let transport = self.get_inner()?;
        Ok(transport.get_sn_bytes())
    }

    #[inline(always)]
    pub fn is_shm(&self) -> ZResult<bool> {
        let transport = self.get_inner()?;
        Ok(transport.is_shm())
    }

    #[inline(always)]
    pub fn is_qos(&self) -> ZResult<bool> {
        let transport = self.get_inner()?;
        Ok(transport.is_qos())
    }

    #[inline(always)]
    pub fn get_callback(&self) -> ZResult<Option<Arc<dyn TransportPeerEventHandler>>> {
        let transport = self.get_inner()?;
        Ok(transport.get_callback())
    }

    pub fn get_peer(&self) -> ZResult<TransportPeer> {
        let transport = self.get_inner()?;
        let tp = TransportPeer {
            zid: transport.get_zid(),
            whatami: transport.get_whatami(),
            is_qos: transport.is_qos(),
            is_shm: transport.is_shm(),
            links: transport
                .get_links()
                .into_iter()
                .map(|l| l.into())
                .collect(),
        };
        Ok(tp)
    }

    #[inline(always)]
    pub fn get_links(&self) -> ZResult<Vec<Link>> {
        let transport = self.get_inner()?;
        Ok(transport
            .get_links()
            .into_iter()
            .map(|l| l.into())
            .collect())
    }

    #[inline(always)]
    pub fn schedule(&self, message: ZenohMessage) -> ZResult<()> {
        let transport = self.get_inner()?;
        transport.schedule(message);
        Ok(())
    }

    #[inline(always)]
    pub async fn close_link(&self, link: &Link) -> ZResult<()> {
        let transport = self.get_inner()?;
        let link = transport
            .get_links()
            .into_iter()
            .find(|l| l.get_src() == link.src && l.get_dst() == link.dst)
            .ok_or_else(|| zerror!("Invalid link"))?;
        transport.close_link(&link, CloseReason::Generic).await?;
        Ok(())
    }

    #[inline(always)]
    pub async fn close(&self) -> ZResult<()> {
        // Return Ok if the transport has already been closed
        match self.get_inner() {
            Ok(transport) => transport.close(CloseReason::Generic).await,
            Err(_) => Ok(()),
        }
    }

    #[inline(always)]
    pub fn handle_message(&self, message: ZenohMessage) -> ZResult<()> {
        self.schedule(message)
    }

    #[cfg(feature = "stats")]
    pub fn get_stats(&self) -> ZResult<TransportUnicastStats> {
        Ok(self.get_inner()?.stats.snapshot())
    }
}

impl From<&Arc<TransportUnicastInner>> for TransportUnicast {
    fn from(s: &Arc<TransportUnicastInner>) -> TransportUnicast {
        TransportUnicast(Arc::downgrade(s))
    }
}

impl Eq for TransportUnicast {}

impl PartialEq for TransportUnicast {
    fn eq(&self, other: &Self) -> bool {
        Weak::ptr_eq(&self.0, &other.0)
    }
}

impl fmt::Debug for TransportUnicast {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.get_inner() {
            Ok(transport) => f
                .debug_struct("Transport Unicast")
                .field("pid", &transport.get_zid())
                .field("whatami", &transport.get_whatami())
                .field("sn_bytes", &transport.get_sn_bytes())
                .field("sn_resolution", &transport.get_sn_bytes().resolution())
                .field("is_qos", &transport.is_qos())
                .field("is_shm", &transport.is_shm())
                .field("links", &transport.get_links())
                .finish(),
            Err(e) => {
                write!(f, "{}", e)
            }
        }
    }
}
