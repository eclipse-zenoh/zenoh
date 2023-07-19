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

use std::{sync::Arc, fmt::DebugStruct, time::Duration};

use async_trait::async_trait;
use zenoh_link::{LinkUnicast, LinkUnicastDirection};
use zenoh_protocol::{core::{ZenohId, WhatAmI}, network::NetworkMessage, transport::TransportSn};
use zenoh_result::ZResult;
use async_std::sync::MutexGuard as AsyncMutexGuard;

use crate::{TransportPeerEventHandler, TransportExecutor};

/*************************************/
/*      UNICAST TRANSPORT TRAIT      */
/*************************************/
#[async_trait]
pub(crate) trait TransportUnicastInnerTrait: Send + Sync {
    /*************************************/
    /*            ACCESSORS              */
    /*************************************/
    fn set_callback(&self, callback: Arc<dyn TransportPeerEventHandler>);
    async fn get_alive(&self) -> AsyncMutexGuard<'_, bool>;
    fn get_zid(&self) -> ZenohId;
    fn get_whatami(&self) -> WhatAmI;
    fn get_callback(&self) -> Option<Arc<dyn TransportPeerEventHandler>>;
    fn get_links(&self) -> Vec<LinkUnicast>;
    #[cfg(feature = "shared-memory")]
    fn is_shm(&self) -> bool;
    fn is_qos(&self) -> bool;

    /*************************************/
    /*               LINK                */
    /*************************************/
    fn add_link(
        &self,
        link: LinkUnicast,
        direction: LinkUnicastDirection,
    ) -> ZResult<()>;

    /*************************************/
    /*                TX                 */
    /*************************************/
    async fn schedule(&self, msg: NetworkMessage) -> ZResult<()>;
    fn start_tx(
        &self,
        link: &LinkUnicast,
        executor: &TransportExecutor,
        keep_alive: Duration,
        batch_size: u16,
    ) -> ZResult<()>;

    /*************************************/
    /*                RX                 */
    /*************************************/
    fn start_rx(
        &self,
        link: &LinkUnicast,
        lease: Duration,
        batch_size: u16,
    ) -> ZResult<()>;

    /*************************************/
    /*           INITIATION              */
    /*************************************/
    async fn sync(&self, initial_sn_rx: TransportSn) -> ZResult<()> { Ok(()) }

    /*************************************/
    /*            TERMINATION            */
    /*************************************/
    async fn close_link(&self, link: &LinkUnicast, reason: u8) -> ZResult<()>;
    async fn close(&self, reason: u8) -> ZResult<()>;

    fn add_debug_fields<'a, 'b: 'a, 'c>(&self, s: &'c mut DebugStruct<'a, 'b>) -> &'c mut DebugStruct<'a, 'b> { s }
}
