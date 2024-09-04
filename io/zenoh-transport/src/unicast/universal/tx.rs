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
use zenoh_protocol::{
    core::{PriorityRange, Reliability},
    network::NetworkMessage,
};

use super::transport::TransportUnicastUniversal;
#[cfg(feature = "shared-memory")]
use crate::shm::map_zmsg_to_partner;
use crate::unicast::transport_unicast_inner::TransportUnicastTrait;

impl TransportUnicastUniversal {
    fn schedule_on_link(&self, msg: NetworkMessage) -> bool {
        let transport_links = self
            .links
            .read()
            .expect("reading `TransportUnicastUniversal::links` should not fail");

        let msg_reliability = Reliability::from(msg.is_reliable());

        let (full_match, partial_match, any_match) = transport_links.iter().enumerate().fold(
            (None::<(_, PriorityRange)>, None, None),
            |(mut full_match, mut partial_match, mut any_match), (i, transport_link)| {
                let reliability = transport_link.link.config.reliability == msg_reliability;
                let priorities = transport_link
                    .link
                    .config
                    .priorities
                    .and_then(|range| range.contains(msg.priority()).then_some(range));

                match (reliability, priorities) {
                    (true, Some(priorities)) => {
                        match full_match {
                            Some((_, r)) if priorities.len() < r.len() => {
                                full_match = Some((i, priorities))
                            }
                            None => full_match = Some((i, priorities)),
                            _ => {}
                        };
                    }
                    (true, None) if partial_match.is_none() => partial_match = Some(i),
                    _ if any_match.is_none() => any_match = Some(i),
                    _ => {}
                };

                (full_match, partial_match, any_match)
            },
        );

        let Some(transport_link_index) = full_match.map(|(i, _)| i).or(partial_match).or(any_match)
        else {
            tracing::trace!(
                "Message dropped because the transport has no links: {}",
                msg
            );

            // No Link found
            return false;
        };

        let transport_link = transport_links
            .get(transport_link_index)
            .expect("transport link index should be valid");

        let pipeline = transport_link.pipeline.clone();
        tracing::trace!(
            "Scheduled {:?} for transmission to {} ({})",
            msg,
            transport_link.link.link.get_dst(),
            self.get_zid()
        );
        // Drop the guard before the push_zenoh_message since
        // the link could be congested and this operation could
        // block for fairly long time
        drop(transport_links);
        pipeline.push_network_message(msg)
    }

    #[allow(unused_mut)] // When feature "shared-memory" is not enabled
    #[allow(clippy::let_and_return)] // When feature "stats" is not enabled
    #[inline(always)]
    pub(crate) fn internal_schedule(&self, mut msg: NetworkMessage) -> bool {
        #[cfg(feature = "shared-memory")]
        {
            if let Err(e) = map_zmsg_to_partner(&mut msg, &self.config.shm) {
                tracing::trace!("Failed SHM conversion: {}", e);
                return false;
            }
        }

        let res = self.schedule_on_link(msg);

        #[cfg(feature = "stats")]
        if res {
            self.stats.inc_tx_n_msgs(1);
        } else {
            self.stats.inc_tx_n_dropped(1);
        }

        res
    }
}
