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
    core::{Priority, PriorityRange, Reliability},
    network::NetworkMessage,
};

use super::transport::TransportUnicastUniversal;
#[cfg(feature = "shared-memory")]
use crate::shm::map_zmsg_to_partner;
use crate::unicast::transport_unicast_inner::TransportUnicastTrait;

impl TransportUnicastUniversal {
    /// Returns the index of the best matching [`Reliability`]-[`PriorityRange`] pair.
    ///
    /// The result is either:
    /// 1. A "full match" where the link matches both `reliability` and `priority`. In case of
    ///    multiple candidates, the link with the smaller range is selected.
    /// 2. A "partial match" where the link match `reliability` and **not** `priority`.
    /// 3. A "no match" where any available link is selected.
    ///
    /// If `transport_links` is empty then [`None`] is returned.
    fn select(
        elements: impl Iterator<Item = (Reliability, Option<PriorityRange>)>,
        reliability: Reliability,
        priority: Priority,
    ) -> Option<usize> {
        let (full_match, partial_match, any_match) = elements.enumerate().fold(
            (None::<(_, PriorityRange)>, None, None),
            |(mut full_match, mut partial_match, mut no_match), (i, (reliability_, priorities))| {
                let reliability = reliability_.eq(&reliability);
                let priorities = priorities.filter(|range| range.contains(priority));

                match (reliability, priorities) {
                    (true, Some(priorities))
                        if full_match
                            .as_ref()
                            .map_or(true, |(_, r)| priorities.len() < r.len()) =>
                    {
                        full_match = Some((i, priorities))
                    }
                    (true, None) if partial_match.is_none() => partial_match = Some(i),
                    _ if no_match.is_none() => no_match = Some(i),
                    _ => {}
                };

                (full_match, partial_match, no_match)
            },
        );

        full_match.map(|(i, _)| i).or(partial_match).or(any_match)
    }

    fn schedule_on_link(&self, msg: NetworkMessage) -> bool {
        let transport_links = self
            .links
            .read()
            .expect("reading `TransportUnicastUniversal::links` should not fail");

        let Some(transport_link_index) = Self::select(
            transport_links
                .iter()
                .map(|tl| (tl.link.config.reliability, tl.link.config.priorities)),
            Reliability::from(msg.is_reliable()),
            msg.priority(),
        ) else {
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

#[cfg(test)]
mod tests {
    use zenoh_protocol::core::{Priority, PriorityRange, Reliability};

    use crate::unicast::universal::transport::TransportUnicastUniversal;

    #[test]
    /// Tests the "full match" scenario with exactly one candidate.
    fn test_link_selection_scenario_1() {
        let selection = TransportUnicastUniversal::select(
            [
                (
                    Reliability::Reliable,
                    Some(PriorityRange::new(0, 1).unwrap()),
                ),
                (
                    Reliability::Reliable,
                    Some(PriorityRange::new(1, 2).unwrap()),
                ),
                (
                    Reliability::BestEffort,
                    Some(PriorityRange::new(0, 1).unwrap()),
                ),
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
                (
                    Reliability::Reliable,
                    Some(PriorityRange::new(0, 2).unwrap()),
                ),
                (
                    Reliability::Reliable,
                    Some(PriorityRange::new(0, 1).unwrap()),
                ),
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
                (
                    Reliability::BestEffort,
                    Some(PriorityRange::new(0, 1).unwrap()),
                ),
                (Reliability::Reliable, None),
            ]
            .into_iter(),
            Reliability::Reliable,
            Priority::try_from(0).unwrap(),
        );
        assert_eq!(selection, Some(1));
    }

    #[test]
    /// Tests the "no match" scenario.
    fn test_link_selection_scenario_4() {
        let selection = TransportUnicastUniversal::select(
            [(Reliability::BestEffort, None)].into_iter(),
            Reliability::Reliable,
            Priority::try_from(0).unwrap(),
        );
        assert_eq!(selection, Some(0));
    }
}
