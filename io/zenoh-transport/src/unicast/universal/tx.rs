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

#[cfg(feature = "unstable")]
use zenoh_protocol::core::CongestionControl;
use zenoh_protocol::{
    core::{Priority, PriorityRange, Reliability},
    network::{NetworkMessageExt, NetworkMessageMut, NetworkMessageRef},
    transport::close,
};
use zenoh_result::ZResult;

use super::transport::TransportUnicastUniversal;
#[cfg(feature = "shared-memory")]
use crate::shm::map_zmsg_to_partner;
use crate::unicast::transport_unicast_inner::TransportUnicastTrait;

impl TransportUnicastUniversal {
    /// Returns the index of the best matching [`Reliability`]-[`PriorityRange`] pair.
    ///
    /// The result is either:
    /// 1. A "full match" where the pair matches both `reliability` and `priority`. In case of
    ///    multiple candidates, the pair with the smaller range is selected.
    /// 2. A "partial match" where the pair match `reliability` and **not** `priority`.
    /// 3. An "any match" where any available pair is selected.
    ///
    /// If `elements` is empty then [`None`] is returned.
    fn select(
        elements: impl Iterator<Item = (Reliability, Option<PriorityRange>)>,
        reliability: Reliability,
        priority: Priority,
    ) -> Option<usize> {
        #[derive(Default)]
        struct Match {
            full: Option<usize>,
            partial: Option<usize>,
            any: Option<usize>,
        }

        let (match_, _) = elements.enumerate().fold(
            (Match::default(), Option::<PriorityRange>::None),
            |(mut match_, mut prev_priorities), (i, (r, ps))| {
                match (r.eq(&reliability), ps.filter(|ps| ps.contains(&priority))) {
                    (true, Some(priorities))
                        if prev_priorities
                            .as_ref()
                            .map_or(true, |ps| ps.len() > priorities.len()) =>
                    {
                        match_.full = Some(i);
                        prev_priorities = Some(priorities);
                    }
                    (true, None) if match_.partial.is_none() => match_.partial = Some(i),
                    _ if match_.any.is_none() => match_.any = Some(i),
                    _ => {}
                };

                (match_, prev_priorities)
            },
        );

        match_.full.or(match_.partial).or(match_.any)
    }

    fn schedule_on_link(&self, msg: NetworkMessageRef) -> ZResult<()> {
        let transport_links = self
            .links
            .read()
            .expect("reading `TransportUnicastUniversal::links` should not fail");

        let Some(transport_link_index) = Self::select(
            transport_links.iter().map(|tl| {
                (
                    tl.link
                        .config
                        .reliability
                        .unwrap_or(Reliability::from(tl.link.link.is_reliable())),
                    tl.link.config.priorities.clone(),
                )
            }),
            Reliability::from(msg.is_reliable()),
            msg.priority(),
        ) else {
            tracing::trace!(
                "Message dropped because the transport has no links: {}",
                msg
            );
            // No Link found
            #[cfg(feature = "stats")]
            self.stats.inc_tx_n_dropped(1);
            return Ok(());
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

        #[cfg(feature = "unstable")]
        if msg.congestion_control() == CongestionControl::BlockFirst {
            let priority = msg.priority();
            if transport_link.block_first_waiters[priority as usize]
                .wait_timeout(self.manager.config.wait_before_drop)
                .is_err()
            {
                #[cfg(feature = "stats")]
                self.stats.inc_tx_n_dropped(1);
                return Ok(());
            };
            let transport = self.clone();
            let block_first_notifier =
                transport_link.block_first_notifiers[priority as usize].clone();
            let msg = NetworkMessageExt::to_owned(&msg);
            zenoh_runtime::ZRuntime::Net.spawn_blocking(move || {
                let msg = msg.as_ref();
                if let Ok(pushed) = pipeline.push_network_message(msg) {
                    transport.handle_push_result(msg, pushed);
                }
                let _ = block_first_notifier.notify();
            });
            return Ok(());
        }

        // Drop the guard before the push_zenoh_message since
        // the link could be congested and this operation could
        // block for fairly long time
        drop(transport_links);

        self.handle_push_result(msg, pipeline.push_network_message(msg)?);
        Ok(())
    }

    fn handle_push_result(&self, msg: NetworkMessageRef, pushed: bool) {
        if !pushed && !msg.is_droppable() {
            tracing::error!(
                "Unable to push non droppable network message to {}. Closing transport!",
                self.config.zid
            );
            zenoh_runtime::ZRuntime::RX.spawn({
                let transport = self.clone();
                async move {
                    if let Err(e) = transport.close(close::reason::UNRESPONSIVE).await {
                        tracing::error!(
                            "Error closing transport with {}: {}",
                            transport.config.zid,
                            e
                        );
                    }
                }
            });
        }
        #[cfg(feature = "stats")]
        if pushed {
            #[cfg(feature = "shared-memory")]
            if msg.is_shm() {
                self.stats.tx_n_msgs.inc_shm(1);
            } else {
                self.stats.tx_n_msgs.inc_net(1);
            }
            #[cfg(not(feature = "shared-memory"))]
            self.stats.tx_n_msgs.inc_net(1);
        } else {
            self.stats.inc_tx_n_dropped(1);
        }
    }

    #[allow(unused_mut)] // When feature "shared-memory" is not enabled
    #[allow(clippy::let_and_return)] // When feature "stats" is not enabled
    #[inline(always)]
    pub(crate) fn internal_schedule(&self, mut msg: NetworkMessageMut) -> ZResult<()> {
        #[cfg(feature = "shared-memory")]
        if let Some(shm_context) = &self.shm_context {
            map_zmsg_to_partner(&mut msg, &shm_context.shm_config, &shm_context.shm_provider);
        }
        self.schedule_on_link(msg.as_ref())
    }
}

#[cfg(test)]
mod tests {
    use zenoh_protocol::core::{Priority, PriorityRange, Reliability};

    use crate::unicast::universal::transport::TransportUnicastUniversal;

    macro_rules! priority_range {
        ($start:literal, $end:literal) => {
            PriorityRange::new($start.try_into().unwrap()..=$end.try_into().unwrap())
        };
    }

    #[test]
    /// Tests the "full match" scenario with exactly one candidate.
    fn test_link_selection_scenario_1() {
        let selection = TransportUnicastUniversal::select(
            [
                (Reliability::Reliable, Some(priority_range!(0, 1))),
                (Reliability::Reliable, Some(priority_range!(1, 2))),
                (Reliability::BestEffort, Some(priority_range!(0, 1))),
            ]
            .into_iter(),
            Reliability::Reliable,
            Priority::try_from(0).unwrap(),
        );
        assert_eq!(selection, Some(0));
    }

    #[test]
    /// Tests the "full match" scenario with multiple candidates.
    fn test_link_selection_scenario_2() {
        let selection = TransportUnicastUniversal::select(
            [
                (Reliability::Reliable, Some(priority_range!(0, 2))),
                (Reliability::Reliable, Some(priority_range!(0, 1))),
            ]
            .into_iter(),
            Reliability::Reliable,
            Priority::try_from(0).unwrap(),
        );
        assert_eq!(selection, Some(1));
    }

    #[test]
    /// Tests the "partial match" scenario.
    fn test_link_selection_scenario_3() {
        let selection = TransportUnicastUniversal::select(
            [
                (Reliability::BestEffort, Some(priority_range!(0, 1))),
                (Reliability::Reliable, None),
            ]
            .into_iter(),
            Reliability::Reliable,
            Priority::try_from(0).unwrap(),
        );
        assert_eq!(selection, Some(1));
    }

    #[test]
    /// Tests the "any match" scenario.
    fn test_link_selection_scenario_4() {
        let selection = TransportUnicastUniversal::select(
            [(Reliability::BestEffort, None)].into_iter(),
            Reliability::Reliable,
            Priority::try_from(0).unwrap(),
        );
        assert_eq!(selection, Some(0));
    }
}
