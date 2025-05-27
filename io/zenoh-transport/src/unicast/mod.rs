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
pub mod authentication;
pub mod establishment;
pub(crate) mod link;
pub(crate) mod lowlatency;
pub(crate) mod manager;
#[cfg(feature = "test")]
pub mod test_helpers;
pub(crate) mod transport_unicast_inner;
pub(crate) mod universal;

use std::{
    fmt,
    sync::{Arc, Weak},
};

#[cfg(feature = "transport_multilink")]
use establishment::ext::auth::ZPublicKey;
pub use manager::*;
use zenoh_core::zcondfeat;
use zenoh_link::Link;
use zenoh_protocol::{
    core::{Bits, WhatAmI, ZenohIdProto},
    network::NetworkMessageMut,
    transport::{close, init::ext::PatchType, TransportSn},
};
use zenoh_result::{zerror, ZResult};

use self::transport_unicast_inner::TransportUnicastTrait;
use super::{TransportPeer, TransportPeerEventHandler};
#[cfg(feature = "shared-memory")]
use crate::shm::TransportShmConfig;
#[cfg(feature = "stats")]
use crate::stats::TransportStats;
use crate::unicast::authentication::TransportAuthId;
#[cfg(feature = "auth_usrpwd")]
use crate::unicast::establishment::ext::auth::UsrPwdId;

/*************************************/
/*        TRANSPORT UNICAST          */
/*************************************/
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TransportConfigUnicast {
    pub(crate) zid: ZenohIdProto,
    pub(crate) whatami: WhatAmI,
    pub(crate) sn_resolution: Bits,
    pub(crate) tx_initial_sn: TransportSn,
    pub(crate) is_qos: bool,
    #[cfg(feature = "transport_multilink")]
    pub(crate) multilink: Option<ZPublicKey>,
    #[cfg(feature = "shared-memory")]
    pub(crate) shm: Option<TransportShmConfig>,
    pub(crate) is_lowlatency: bool,
    #[cfg(feature = "auth_usrpwd")]
    pub(crate) auth_id: UsrPwdId,
    pub(crate) patch: PatchType,
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
    pub fn get_zid(&self) -> ZResult<ZenohIdProto> {
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
            links: transport.get_links(),
            is_qos: transport.is_qos(),
            #[cfg(feature = "shared-memory")]
            is_shm: transport.is_shm(),
        };
        Ok(tp)
    }

    #[inline(always)]
    pub fn get_links(&self) -> ZResult<Vec<Link>> {
        let transport = self.get_inner()?;
        Ok(transport.get_links())
    }

    pub fn get_auth_ids(&self) -> ZResult<TransportAuthId> {
        let transport = self.get_inner()?;
        Ok(transport.get_auth_ids())
    }

    #[inline(always)]
    pub fn schedule(&self, message: NetworkMessageMut) -> ZResult<()> {
        let transport = self.get_inner()?;
        transport.schedule(message)
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

    #[cfg(feature = "stats")]
    pub fn get_link_stats(&self) -> ZResult<Vec<(Link, Arc<TransportStats>)>> {
        let transport = self.get_inner()?;
        Ok(transport.get_link_stats())
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
