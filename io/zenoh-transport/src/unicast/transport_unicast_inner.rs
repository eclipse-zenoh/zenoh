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

use crate::{
    unicast::{link::TransportLinkUnicast, TransportConfigUnicast},
    TransportPeerEventHandler,
};
use async_trait::async_trait;
use std::{fmt::DebugStruct, sync::Arc, time::Duration};
use tokio::sync::MutexGuard as AsyncMutexGuard;
use zenoh_link::Link;
use zenoh_protocol::{
    core::{WhatAmI, ZenohId},
    network::NetworkMessage,
    transport::TransportSn,
};
use zenoh_result::ZResult;

use crate::{TransportConfigUnicast, TransportPeerEventHandler};

/*************************************/
/*      UNICAST TRANSPORT TRAIT      */
/*************************************/
#[async_trait]
pub(crate) trait TransportUnicastTrait: Send + Sync {
    /*************************************/
    /*            ACCESSORS              */
    /*************************************/
    fn set_callback(&self, callback: Arc<dyn TransportPeerEventHandler>);

    async fn get_alive(&self) -> AsyncMutexGuard<'_, bool>;
    fn get_zid(&self) -> ZenohId;
    fn get_whatami(&self) -> WhatAmI;
    fn get_callback(&self) -> Option<Arc<dyn TransportPeerEventHandler>>;
    fn get_links(&self) -> Vec<Link>;
    #[cfg(feature = "shared-memory")]
    fn is_shm(&self) -> bool;
    fn is_qos(&self) -> bool;
    fn get_config(&self) -> &TransportConfigUnicast;
    #[cfg(feature = "stats")]
    fn stats(&self) -> Arc<crate::stats::TransportStats>;

    /*************************************/
    /*               LINK                */
    /*************************************/
    async fn add_link(
        &self,
        link: LinkUnicastWithOpenAck,
        other_initial_sn: TransportSn,
        other_lease: Duration,
    ) -> AddLinkResult;

    /*************************************/
    /*                TX                 */
    /*************************************/
    fn schedule(&self, msg: NetworkMessage) -> ZResult<()>;
    fn start_tx(&self, link: &LinkUnicast, keep_alive: Duration, batch_size: u16) -> ZResult<()>;

    /*************************************/
    /*                RX                 */
    /*************************************/
    fn start_rx(&self, link: &LinkUnicast, lease: Duration, batch_size: u16) -> ZResult<()>;

    /*************************************/
    /*           INITIATION              */
    /*************************************/
    async fn sync(&self, _initial_sn_rx: TransportSn) -> ZResult<()>;

    /*************************************/
    /*            TERMINATION            */
    /*************************************/
    async fn close_link(&self, link: Link, reason: u8) -> ZResult<()>;
    async fn close(&self, reason: u8) -> ZResult<()>;

    fn add_debug_fields<'a, 'b: 'a, 'c>(
        &self,
        s: &'c mut DebugStruct<'a, 'b>,
    ) -> &'c mut DebugStruct<'a, 'b> {
        s
    }
}
