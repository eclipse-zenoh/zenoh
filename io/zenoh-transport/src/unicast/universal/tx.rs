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
        let Some(pipeline) =
            self.links
                .get_pipeline(msg.is_reliable().into(), msg.priority(), |links| {
                    let link_props = links
                        .iter()
                        .map(|tl| (tl.link.reliability(), tl.link.config.priorities.clone()));
                    Self::select(link_props, msg.is_reliable().into(), msg.priority())
                })
        else {
            // No Link found
            tracing::trace!(
                "Message dropped because the transport has no links: {}",
                msg
            );
            // No Link found
            #[cfg(feature = "stats")]
            self.stats.inc_tx_n_dropped(1);
            return Ok(());
        };
        tracing::trace!("Scheduled {:?} for transmission to {}", msg, self.get_zid());
        let droppable = msg.is_droppable();
        let push = pipeline.push_network_message(msg)?;
        if !push && !droppable {
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
        if push {
            self.stats.inc_tx_n_msgs(1);
        } else {
            self.stats.inc_tx_n_dropped(1);
        }
        Ok(())
    }

    #[allow(unused_mut)] // When feature "shared-memory" is not enabled
    #[allow(clippy::let_and_return)] // When feature "stats" is not enabled
    #[inline(always)]
    pub(crate) fn internal_schedule(&self, mut msg: NetworkMessageMut) -> ZResult<()> {
        #[cfg(feature = "shared-memory")]
        {
            if let Err(e) = map_zmsg_to_partner(&mut msg, &self.config.shm) {
                tracing::trace!("Failed SHM conversion: {}", e);
                #[cfg(feature = "stats")]
                self.stats.inc_tx_n_dropped(1);
                return Ok(());
            }
        }

        #[cfg(feature = "unstable")]
        if msg.congestion_control() == CongestionControl::BlockFirst {
            if self
                .block_first_waiter
                .wait_timeout(self.manager.config.wait_before_drop.0)
                .is_err()
            {
                #[cfg(feature = "stats")]
                self.stats.inc_tx_n_dropped(1);
                return Ok(());
            };
            let transport = self.clone();
            let msg = msg.to_owned();
            zenoh_runtime::ZRuntime::Net.spawn_blocking(move || {
                let _ = transport.schedule_on_link(msg.as_ref());
                let _ = transport.block_first_notifier.notify();
            });
            Ok(())
        } else {
            self.schedule_on_link(msg.as_ref())
        }

        #[cfg(not(feature = "unstable"))]
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
