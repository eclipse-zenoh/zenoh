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
use std::{
    collections::HashMap,
    fmt::DebugStruct,
    sync::{Arc, RwLock},
    time::Duration,
};

use async_trait::async_trait;
use tokio::sync::{Mutex as AsyncMutex, MutexGuard as AsyncMutexGuard};
use zenoh_core::{zasynclock, zcondfeat, zread, zwrite};
use zenoh_link::Link;
use zenoh_protocol::{
    core::{Priority, WhatAmI, ZenohIdProto},
    network::NetworkMessageMut,
    transport::{close, Close, PrioritySn, TransportMessage, TransportSn},
};
use zenoh_result::{bail, zerror, ZResult};

#[cfg(feature = "shared-memory")]
use crate::shm_context::UnicastTransportShmContext;
use crate::{
    common::priority::{TransportPriorityRx, TransportPriorityTx},
    unicast::{
        authentication::TransportAuthId,
        link::{LinkUnicastWithOpenAck, TransportLinkUnicastDirection},
        transport_unicast_inner::{AddLinkResult, TransportUnicastTrait},
        universal::link::TransportLinkUnicastUniversal,
        TransportConfigUnicast,
    },
    TransportManager, TransportPeerEventHandler,
};

/*************************************/
/*        UNIVERSAL TRANSPORT        */
/*************************************/
#[derive(Clone)]
pub(crate) struct TransportUnicastUniversal {
    // Transport Manager
    pub(crate) manager: Arc<TransportManager>,
    // Transport config
    pub(super) config: Arc<TransportConfigUnicast>,
    // Tx priorities
    pub(super) priority_tx: Arc<[TransportPriorityTx]>,
    // Rx priorities
    pub(super) priority_rx: Arc<[TransportPriorityRx]>,
    #[cfg(feature = "shared-memory")]
    pub(super) shm_context: Option<UnicastTransportShmContext>,
    // The links associated to the channel
    pub(super) links: Arc<RwLock<TransportLinks>>,
    // The callback
    pub(super) callback: Arc<RwLock<Option<Arc<dyn TransportPeerEventHandler>>>>,
    // Lock used to ensure no race in add_link method
    add_link_lock: Arc<AsyncMutex<()>>,
    // Mutex for notification
    pub(super) alive: Arc<AsyncMutex<bool>>,
    // Transport statistics
    #[cfg(feature = "stats")]
    pub(super) stats: zenoh_stats::TransportStats,
}

impl TransportUnicastUniversal {
    pub fn make(
        manager: TransportManager,
        config: TransportConfigUnicast,
        #[cfg(feature = "shared-memory")] shm_context: Option<UnicastTransportShmContext>,
        #[cfg(feature = "stats")] stats: zenoh_stats::TransportStats,
    ) -> ZResult<Arc<dyn TransportUnicastTrait>> {
        let mut priority_tx = vec![];
        let mut priority_rx = vec![];

        let num = if config.is_qos { Priority::NUM } else { 1 };
        for _ in 0..num {
            priority_tx.push(TransportPriorityTx::make(config.sn_resolution)?);
        }

        for _ in 0..Priority::NUM {
            priority_rx.push(TransportPriorityRx::make(
                config.sn_resolution,
                manager.config.defrag_buff_size,
            )?);
        }

        let initial_sn = PrioritySn {
            reliable: config.tx_initial_sn,
            best_effort: config.tx_initial_sn,
        };
        for c in priority_tx.iter() {
            c.sync(initial_sn)?;
        }

        let t = Arc::new(TransportUnicastUniversal {
            manager: Arc::new(manager),
            config: Arc::new(config),
            priority_tx: priority_tx.into_boxed_slice().into(),
            priority_rx: priority_rx.into_boxed_slice().into(),
            links: Arc::new(RwLock::new(TransportLinks::default())),
            add_link_lock: Arc::new(AsyncMutex::new(())),
            callback: Arc::new(RwLock::new(None)),
            alive: Arc::new(AsyncMutex::new(false)),
            #[cfg(feature = "stats")]
            stats,
            #[cfg(feature = "shared-memory")]
            shm_context,
        });

        Ok(t)
    }

    /*************************************/
    /*           TERMINATION             */
    /*************************************/
    pub(super) async fn delete(&self) -> ZResult<()> {
        tracing::debug!(
            "[{}] Closing transport with peer: {}",
            self.manager.config.zid,
            self.config.zid
        );

        // Mark the transport as no longer alive and keep the lock
        // to avoid concurrent new_transport and closing/closed notifications
        let mut a_guard = self.get_alive().await;
        *a_guard = false;
        let callback = zwrite!(self.callback).take();

        // Delete the transport on the manager
        let _ = self.manager.del_transport_unicast(&self.config.zid).await;

        // Close all the links
        let mut links = zwrite!(self.links).take();
        for l in links.drain(..) {
            let _ = l.close().await;
        }

        // Notify the callback that we have closed the transport
        if let Some(cb) = callback.as_ref() {
            cb.closed();
        }

        Ok(())
    }

    pub(crate) async fn del_link(&self, link: Link) -> ZResult<()> {
        // Try to remove the link
        let Some((is_last, stl, associated_link)) = zwrite!(self.links).remove_link(&link) else {
            bail!(
                "Can not delete Link {} with peer: {}",
                link,
                self.config.zid
            )
        };

        // Notify the callback
        let cb = zread!(self.callback).clone();
        if let Some(callback) = cb {
            callback.del_link(link);
            if let Some(asl) = &associated_link {
                callback.del_link(Link::new_unicast(
                    &asl.link.link,
                    asl.link.config.priorities.clone(),
                    asl.link.config.reliability,
                ));
            }
        }

        let res = {
            // Associated link must also be closed. run both close calls, return whichever failed first
            let r1 = stl.close().await;
            let r2 = if let Some(asl) = associated_link {
                asl.close().await
            } else {
                Ok(())
            };
            match r1 {
                r1 @ Err(_) => r1,
                Ok(_) => r2,
            }
        };

        if is_last {
            self.delete().await?;
        }
        res
    }

    async fn sync(&self, initial_sn_rx: TransportSn) -> ZResult<()> {
        // Mark the transport as alive and keep the lock
        // to avoid concurrent new_transport and closing/closed notifications
        let mut a_guard = zasynclock!(self.alive);
        if *a_guard {
            let e = zerror!("Transport already synched with peer: {}", self.config.zid);
            tracing::trace!("{}", e);
            return Err(e.into());
        }

        *a_guard = true;

        let csn = PrioritySn {
            reliable: initial_sn_rx,
            best_effort: initial_sn_rx,
        };
        for c in self.priority_rx.iter() {
            c.sync(csn)?;
        }

        Ok(())
    }
}

#[async_trait]
impl TransportUnicastTrait for TransportUnicastUniversal {
    /*************************************/
    /*               LINK                */
    /*************************************/
    async fn add_link(
        &self,
        link: LinkUnicastWithOpenAck,
        other_initial_sn: TransportSn,
        other_lease: Duration,
    ) -> AddLinkResult {
        let add_link_guard = zasynclock!(self.add_link_lock);

        // Check if we can add more inbound links
        if let TransportLinkUnicastDirection::Inbound = link.inner_config().direction {
            let limit = zcondfeat!(
                "transport_multilink",
                match self.config.multilink {
                    Some(_) => self.manager.config.unicast.max_links,
                    None => 1,
                },
                1
            );

            if zread!(self.links)
                .reached_multilink_limit(TransportLinkUnicastDirection::Inbound, limit)
            {
                let e = zerror!(
                    "Can not add Link {} with peer {}: max num of links reached ({})",
                    link,
                    self.config.zid,
                    limit
                );
                return Err((e.into(), link.fail(), close::reason::MAX_LINKS));
            }
        }

        // sync the RX sequence number
        let _ = self.sync(other_initial_sn).await;

        // Wrap the link
        let (link, ack, associated_link) = link.unpack();
        let (mut link, consumer) =
            TransportLinkUnicastUniversal::new(self, link, &self.priority_tx);

        // Handle associated link (if mixed-reliability link)
        let (associated_link, al_consumer) = associated_link
            .map(|l| TransportLinkUnicastUniversal::new(self, l, &self.priority_tx))
            .map_or((None, None), |(l, c)| (Some(l), Some(c)));
        // Add the link to the channel
        {
            let mut transport_links = zwrite!(self.links);
            transport_links.push_link(link.clone(), associated_link.clone());
        }
        // create a callback to start the link
        let transport = self.clone();
        let start_tx = {
            let mut link = link.clone();
            let transport = transport.clone();
            let associated_link = associated_link.clone();
            Box::new(move || {
                // Start the TX loop
                let keep_alive = self.manager.config.unicast.lease
                    / self.manager.config.unicast.keep_alive as u32;
                link.start_tx(transport.clone(), consumer, keep_alive);
                if let Some(mut associated_link) = associated_link {
                    associated_link.start_tx(
                        transport,
                        al_consumer.expect("consumer should be Some when associated_link is Some"),
                        keep_alive,
                    );
                }
            })
        };

        let start_rx = Box::new(move || {
            // Start the RX loop
            link.start_rx(transport.clone(), other_lease);
            if let Some(mut associated_link) = associated_link {
                associated_link.start_rx(transport, other_lease);
            }
        });

        Ok((start_tx, start_rx, ack, Some(add_link_guard)))
    }

    /*************************************/
    /*            ACCESSORS              */
    /*************************************/
    fn set_callback(&self, callback: Arc<dyn TransportPeerEventHandler>) {
        *zwrite!(self.callback) = Some(callback);
    }

    async fn get_alive(&self) -> AsyncMutexGuard<'_, bool> {
        zasynclock!(self.alive)
    }

    fn get_zid(&self) -> ZenohIdProto {
        self.config.zid
    }

    fn get_whatami(&self) -> WhatAmI {
        self.config.whatami
    }

    #[cfg(feature = "shared-memory")]
    fn is_shm(&self) -> bool {
        self.config.shm.is_some()
    }

    fn is_qos(&self) -> bool {
        self.config.is_qos
    }

    fn get_callback(&self) -> Option<Arc<dyn TransportPeerEventHandler>> {
        zread!(self.callback).clone()
    }

    fn get_config(&self) -> &TransportConfigUnicast {
        &self.config
    }

    #[cfg(feature = "stats")]
    fn stats(&self) -> zenoh_stats::TransportStats {
        self.stats.clone()
    }

    /*************************************/
    /*           TERMINATION             */
    /*************************************/
    async fn close(&self, reason: u8) -> ZResult<()> {
        tracing::trace!("Closing transport with peer: {}", self.config.zid);

        let mut pipelines = zread!(self.links)
            .inner
            .iter()
            .map(|sl| sl.pipeline.clone())
            .collect::<Vec<_>>();
        for p in pipelines.drain(..) {
            // Close message to be sent on all the links
            // session should always be true for user-triggered close. However, in case of
            // multiple links, it is safer to close all the links first. When no links are left,
            // the transport is then considered closed.
            let msg: TransportMessage = Close {
                reason,
                session: false,
            }
            .into();

            p.push_transport_message(msg, Priority::Background);
        }
        // Terminate and clean up the transport
        self.delete().await
    }

    fn get_links(&self) -> Vec<Link> {
        zread!(self.links)
            .inner
            .iter()
            .map(|l| l.link.link())
            .collect()
    }

    fn get_auth_ids(&self) -> TransportAuthId {
        let mut transport_auth_id = TransportAuthId::new(self.get_zid());
        // Convert LinkUnicast auth ids to AuthId
        zread!(self.links)
            .inner
            .iter()
            .for_each(|l| transport_auth_id.push_link_auth_id(l.link.link.get_auth_id().clone()));

        // Convert usrpwd auth id to AuthId
        #[cfg(feature = "auth_usrpwd")]
        transport_auth_id.set_username(&self.config.auth_id);
        transport_auth_id
    }

    /*************************************/
    /*                TX                 */
    /*************************************/
    fn schedule(&self, msg: NetworkMessageMut) -> ZResult<bool> {
        self.internal_schedule(msg)
    }

    fn add_debug_fields<'a, 'b: 'a, 'c>(
        &self,
        s: &'c mut DebugStruct<'a, 'b>,
    ) -> &'c mut DebugStruct<'a, 'b> {
        s.field("sn_resolution", &self.config.sn_resolution)
    }
}

/// Container for Transport links that manages link associations (namely for mixed-reliability).
/// Manages insertion/removal of links and their potential associated links while providing the
/// following:
/// - Evaluate multilink limit by considering each link association as a single link
/// - Expose a read-only view on links that mixes associations in the same set (as if they were
/// seperate links) for message scheduling based on QoS
#[derive(Default)]
pub(super) struct TransportLinks {
    pub(super) inner: Box<[TransportLinkUnicastUniversal]>,
    // TODO: memory usage of associations can probably be optimized
    /// associations of links that internally use a shared state (namely mixed-reliability links)
    associations: HashMap<Link, (Link, TransportLinkUnicastDirection)>,
}

impl TransportLinks {
    fn push_link(
        &mut self,
        link: TransportLinkUnicastUniversal,
        associated_link: Option<TransportLinkUnicastUniversal>,
    ) {
        let mut links =
            Vec::with_capacity(self.inner.len() + if associated_link.is_some() { 2 } else { 1 });
        links.extend_from_slice(&self.inner);
        links.push(link.clone());

        if let Some(l) = associated_link {
            links.push(l.clone());
            // add associations to HashMap
            let l1 = Link::new_unicast(
                &link.link.link,
                link.link.config.priorities.clone(),
                link.link.config.reliability,
            );
            let l2 = Link::new_unicast(
                &l.link.link,
                l.link.config.priorities.clone(),
                l.link.config.reliability,
            );
            self.associations
                .insert(l1.clone(), (l2.clone(), link.link.config.direction));
            self.associations
                .insert(l2, (l1, link.link.config.direction));
        }

        self.inner = links.into_boxed_slice();
    }

    fn remove_link(
        &mut self,
        link: &Link,
    ) -> Option<(
        bool,
        TransportLinkUnicastUniversal,
        Option<TransportLinkUnicastUniversal>,
    )> {
        let link_equality = |tl: &TransportLinkUnicastUniversal, link: &Link| {
            // Compare LinkUnicast link to not compare TransportLinkUnicast direction
            Link::new_unicast(
                &tl.link.link,
                tl.link.config.priorities.clone(),
                tl.link.config.reliability,
            )
            .eq(link)
        };
        let Some(index) = self.inner.iter().position(|tl| link_equality(tl, link)) else {
            return None;
        };

        // Remove the link
        let mut links = self.inner.to_vec();
        let stl = links.remove(index);

        // Remove associated link (if applicable)
        let asl = if let Some((associated_link, _)) = self.associations.remove(link) {
            // Remove opposite association
            self.associations.remove(&associated_link);
            // Remove associated link from links vec
            match links
                .iter()
                .position(|tl| link_equality(tl, &associated_link))
            {
                Some(index) => Some(links.remove(index)),
                None => {
                    // this is possible if link equality cannot be guaranteed between now and
                    // when the link was originally inserted. RX or TX task will fail and remove
                    // it once its internal connection is closed by the associated link.
                    tracing::debug!("Associated link not found while removing link {link}");
                    None
                }
            }
        } else {
            None
        };

        self.inner = links.into_boxed_slice();
        Some((self.inner.is_empty(), stl, asl))
    }

    fn take(&mut self) -> Vec<TransportLinkUnicastUniversal> {
        let links = self.inner.to_vec();
        self.inner = vec![].into_boxed_slice();
        links
    }

    fn reached_multilink_limit(
        &self,
        direction: TransportLinkUnicastDirection,
        limit: usize,
    ) -> bool {
        let count = self
            .inner
            .iter()
            .filter(|l| l.link.config.direction == direction)
            .count();
        count
            - self
                .associations
                .values()
                .filter(|(_, link_direction)| *link_direction == direction)
                .count()
                / 2
            >= limit
    }
}
