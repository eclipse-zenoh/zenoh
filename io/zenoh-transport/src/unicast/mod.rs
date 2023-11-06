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
pub mod establishment;
pub(crate) mod lowlatency;
pub(crate) mod manager;
pub(crate) mod transport_unicast_inner;
pub(crate) mod universal;

#[cfg(feature = "test")]
pub mod test_helpers;

#[cfg(feature = "shared-memory")]
pub(crate) mod shared_memory_unicast;

use self::transport_unicast_inner::TransportUnicastTrait;

use super::{TransportPeer, TransportPeerEventHandler};
#[cfg(feature = "transport_multilink")]
use establishment::ext::auth::ZPublicKey;
pub use manager::*;
use std::fmt;
use std::sync::{Arc, Weak};
use zenoh_core::zcondfeat;
use zenoh_link::Link;
use zenoh_protocol::network::NetworkMessage;
use zenoh_protocol::{
    core::{Bits, WhatAmI, ZenohId},
    transport::{close, TransportSn},
};
use zenoh_result::{zerror, ZResult};

/*************************************/
/*        TRANSPORT UNICAST          */
/*************************************/
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TransportConfigUnicast {
    pub(crate) zid: ZenohId,
    pub(crate) whatami: WhatAmI,
    pub(crate) sn_resolution: Bits,
    pub(crate) tx_initial_sn: TransportSn,
    pub(crate) is_qos: bool,
    #[cfg(feature = "transport_multilink")]
    pub(crate) multilink: Option<ZPublicKey>,
    #[cfg(feature = "shared-memory")]
    pub(crate) is_shm: bool,
    pub(crate) is_lowlatency: bool,
}

/// [`TransportUnicast`] is the transport handler returned
/// when opening a new unicast transport
#[derive(Clone)]
pub struct TransportUnicast(Weak<dyn TransportUnicastTrait>);

impl TransportUnicast {
    #[inline(always)]
    pub(super) fn get_inner(&self) -> ZResult<Arc<dyn TransportUnicastTrait>> {
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

    #[cfg(feature = "shared-memory")]
    #[inline(always)]
    pub fn is_shm(&self) -> ZResult<bool> {
        let transport = self.get_inner()?;
        Ok(transport.is_shm())
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
            links: transport
                .get_links()
                .into_iter()
                .map(|l| l.into())
                .collect(),
            is_qos: transport.is_qos(),
            #[cfg(feature = "shared-memory")]
            is_shm: transport.is_shm(),
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
    pub fn schedule(&self, message: NetworkMessage) -> ZResult<()> {
        let transport = self.get_inner()?;
        transport.schedule(message)
    }

    #[inline(always)]
    pub async fn close_link(&self, link: &Link) -> ZResult<()> {
        let transport = self.get_inner()?;
        let link = transport
            .get_links()
            .into_iter()
            .find(|l| l.get_src() == &link.src && l.get_dst() == &link.dst)
            .ok_or_else(|| zerror!("Invalid link"))?;
        transport.close_link(&link, close::reason::GENERIC).await?;
        Ok(())
    }

    #[inline(always)]
    pub async fn close(&self) -> ZResult<()> {
        // Return Ok if the transport has already been closed
        match self.get_inner() {
            Ok(transport) => transport.close(close::reason::GENERIC).await,
            Err(_) => Ok(()),
        }
    }

    #[cfg(feature = "stats")]
    pub fn get_stats(&self) -> ZResult<Arc<crate::stats::TransportStats>> {
        Ok(self.get_inner()?.stats())
    }
}

impl From<&Arc<dyn TransportUnicastTrait>> for TransportUnicast {
    fn from(link: &Arc<dyn TransportUnicastTrait>) -> TransportUnicast {
        TransportUnicast(Arc::downgrade(link))
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
            Ok(transport) => {
                let is_shm = zcondfeat!("shared-memory", transport.is_shm(), false);

                transport
                    .add_debug_fields(
                        f.debug_struct("Transport Unicast")
                            .field("zid", &transport.get_zid())
                            .field("whatami", &transport.get_whatami())
                            .field("is_qos", &transport.is_qos())
                            .field("is_shm", &is_shm)
                            .field("links", &transport.get_links()),
                    )
                    .finish()
            }
            Err(e) => {
                write!(f, "{e}")
            }
        }
    }
}
