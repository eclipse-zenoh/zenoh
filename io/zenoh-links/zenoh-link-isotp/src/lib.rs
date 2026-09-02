//
// Copyright (c) 2026 ZettaScale Technology
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

//! WARNING
//!
//! This crate is intended for Zenoh's internal use.
//!
//! [Click here for Zenoh's documentation](https://docs.rs/zenoh/latest/zenoh)
//!
//! A **unicast** CAN link over ISO-TP (ISO 15765-2), the CAN transport protocol.
//!
//! # Why ISO-TP rather than raw CAN
//!
//! A CAN frame carries an identifier and at most 8 bytes, and the identifier
//! names the *message*, not a destination: there is no source or destination
//! field, and every node hears every frame. Unicast therefore has to be built by
//! convention, which is what ISO-TP, J1939 and CANopen all do.
//!
//! ISO 15765-2 builds it from **an identifier pair plus flow control**. The
//! receiver must answer a first frame and then paces the sender, so exactly one
//! peer may own the other end of a pair -- on a broadcast medium, two responders
//! would collide. That pairing is the address.
//!
//! Two things follow. Segmentation moves below zenoh, so the MTU is
//! [`ISOTP_MAX_MTU`] rather than seven bytes, which is what makes classic CAN
//! usable at all. And it stops being a bus: one link is one peer pair.
//!
//! This link is therefore **unicast**, which is the point -- zenoh routes queries
//! and liveliness only to unicast faces, so a multicast CAN link cannot carry
//! ROS services, actions, parameters or graph introspection, and this one can.

#[cfg(target_os = "linux")]
mod sys;
mod unicast;

use std::str::FromStr;

use async_trait::async_trait;
pub use unicast::LinkManagerUnicastIsotp;
use zenoh_link_commons::LocatorInspector;
use zenoh_protocol::{
    core::{EndPoint, Locator, Metadata, Reliability},
    transport::BatchSize,
};
use zenoh_result::{bail, zerror, ZResult};

pub const ISOTP_LOCATOR_PREFIX: &str = "isotp";

/// ISO-TP has flow control, but a lost consecutive frame aborts the whole PDU
/// and nothing below zenoh recovers it. Honest is `false`, which is also what
/// the serial link -- the other point-to-point unicast link -- declares.
const IS_RELIABLE: bool = false;

/// The classic ISO-TP first frame carries a **12-bit** length, so a PDU is at
/// most 4095 bytes. The 2016 revision adds a 32-bit escape, but 4095 is what
/// every implementation supports, and a larger MTU is a larger unit of loss:
/// one dropped consecutive frame destroys the entire PDU.
pub const ISOTP_MAX_MTU: BatchSize = 4095;

/// `IFNAMSIZ`, including the terminator.
const IFNAMSIZ: usize = 16;

/// The identifier space of a classic 11-bit CAN frame.
const CAN_SFF_MASK: u32 = 0x0000_07FF;
/// The identifier space of a 29-bit extended CAN frame.
const CAN_EFF_MASK: u32 = 0x1FFF_FFFF;

/// Endpoint configuration keys.
///
/// ```text
/// isotp/<device>#tx_id=0x7E0;rx_id=0x7E8;eff=false;prio_classes=1
/// ```
///
/// * `device` -- the CAN interface, e.g. `can0` or `vcan0`
/// * `tx_id` -- the identifier this peer transmits its PDUs on
/// * `rx_id` -- the identifier it receives on, and on which it sends flow control
/// * `eff` -- use 29-bit extended identifiers instead of 11-bit. Default `false`
/// * `prio_classes` -- 1 or 8. See below
///
/// The two identifiers are a **directed pair**: this peer's `tx_id` must be the
/// other peer's `rx_id` and vice versa. Only ISO-TP **normal** addressing is
/// supported -- extended and mixed addressing are a deliberate non-goal, because
/// no portable implementation provides them and the interoperable common
/// denominator is normal addressing.
///
/// # Priority classes
///
/// With `prio_classes = 8` the link opens eight ISO-TP sockets and selects one
/// by the priority of the batch being written, so zenoh's QoS becomes real CAN
/// arbitration. Class *k* uses `tx_id + k` and `rx_id + k`, so a contiguous
/// block of eight identifiers is reserved from each base, and the lowest class --
/// the most urgent -- holds the lowest identifier and therefore wins the bus.
///
/// Only 1 and 8 are accepted. zenoh runs one receive task per priority when a
/// link reports priority support, so a class must own exactly one socket;
/// mapping several priorities onto one would have those tasks racing for the
/// same PDUs.
///
/// That ordering holds **within** a link. Across links it depends on how the
/// blocks were allocated, which is an operator decision rather than something
/// this link can enforce.
pub mod config {
    pub const TX_ID: &str = "tx_id";
    pub const RX_ID: &str = "rx_id";
    pub const EFF: &str = "eff";
    pub const PRIO_CLASSES: &str = "prio_classes";
    pub const STMIN: &str = "stmin";
    pub const BS: &str = "bs";
}

pub const DEFAULT_PRIO_CLASSES: u8 = 1;

#[derive(Debug, Default, Clone, Copy)]
pub struct IsotpLocatorInspector;

#[async_trait]
impl LocatorInspector for IsotpLocatorInspector {
    fn protocol(&self) -> &str {
        ISOTP_LOCATOR_PREFIX
    }

    /// Never. An ISO-TP channel is a directed identifier pair with flow
    /// control, which is the definition of point-to-point. Reporting `true`
    /// here would route the link through the multicast transport and lose the
    /// queries and liveliness this link exists to carry.
    async fn is_multicast(&self, _locator: &Locator) -> ZResult<bool> {
        Ok(false)
    }

    fn is_reliable(&self, locator: &Locator) -> ZResult<bool> {
        if let Some(reliability) = locator
            .metadata()
            .get(Metadata::RELIABILITY)
            .map(Reliability::from_str)
            .transpose()?
        {
            Ok(reliability == Reliability::Reliable)
        } else {
            Ok(IS_RELIABLE)
        }
    }
}

/// A parsed and validated ISO-TP endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) struct IsotpEndpoint {
    pub(crate) device: String,
    pub(crate) tx_id: u32,
    pub(crate) rx_id: u32,
    pub(crate) eff: bool,
    pub(crate) prio_classes: u8,
    /// Separation time this side asks the peer to leave between the
    /// ConsecutiveFrames it sends us, in milliseconds. Goes out in our
    /// FlowControl. `None` keeps the kernel default.
    pub(crate) stmin: Option<u8>,
    /// How many ConsecutiveFrames the peer may send before waiting for another
    /// FlowControl from us. `None` keeps the kernel default, which is 0,
    /// meaning "send the whole thing".
    pub(crate) bs: Option<u8>,
}

/// Parse an unsigned integer, accepting decimal and `0x`-prefixed hex.
///
/// Identifiers are conventionally written in hex. A malformed value is an
/// error rather than a silent fall back to a default: a mistyped identifier
/// produces a link that opens and never communicates, which is far harder to
/// diagnose than a refusal at startup.
fn parse_u32(key: &str, s: &str) -> ZResult<u32> {
    let t = s.trim();
    match t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        Some(hex) => u32::from_str_radix(hex, 16),
        None => t.parse::<u32>(),
    }
    .map_err(|e| zerror!("invalid `{key}` value {s:?} on an ISO-TP locator: {e}").into())
}

fn get_u32(endpoint: &EndPoint, key: &str) -> ZResult<Option<u32>> {
    endpoint
        .config()
        .get(key)
        .map(|v| parse_u32(key, v))
        .transpose()
}

/// A flow-control byte: `stmin` and `bs` are each one octet of the FlowControl
/// frame, so anything wider is a mistake worth naming rather than truncating.
fn get_u8(endpoint: &EndPoint, key: &str) -> ZResult<Option<u8>> {
    match get_u32(endpoint, key)? {
        None => Ok(None),
        Some(v) if v <= u8::MAX as u32 => Ok(Some(v as u8)),
        Some(v) => {
            bail!("ISO-TP `{key}` is {v}, but it is one byte of the FlowControl frame, so 0..=255")
        }
    }
}

// `ids_for_class`, `class_of` and `locator` are exercised by the tests below
// and consumed by the socket layer in W2; the lib target alone does not yet use
// them. The allow comes off when W2 lands.
#[allow(dead_code)]
impl IsotpEndpoint {
    pub(crate) fn parse(endpoint: &EndPoint) -> ZResult<IsotpEndpoint> {
        if endpoint.protocol().as_str() != ISOTP_LOCATOR_PREFIX {
            bail!("not an ISO-TP locator: {endpoint} (expected protocol `{ISOTP_LOCATOR_PREFIX}`)");
        }

        let device = endpoint.address().as_str().to_owned();
        if device.is_empty() {
            bail!("an ISO-TP locator needs an interface name: {endpoint}");
        }
        if device.len() >= IFNAMSIZ {
            bail!(
                "CAN interface name {device:?} is {} bytes; the kernel allows at most {}",
                device.len(),
                IFNAMSIZ - 1
            );
        }

        let eff = match endpoint.config().get(config::EFF) {
            Some(v) => v
                .trim()
                .parse::<bool>()
                .map_err(|e| zerror!("invalid `{}` value {v:?}: {e}", config::EFF))?,
            None => false,
        };

        let tx_id = get_u32(endpoint, config::TX_ID)?
            .ok_or_else(|| zerror!("an ISO-TP locator needs `{}`: {endpoint}", config::TX_ID))?;
        let rx_id = get_u32(endpoint, config::RX_ID)?
            .ok_or_else(|| zerror!("an ISO-TP locator needs `{}`: {endpoint}", config::RX_ID))?;

        let prio_classes =
            get_u32(endpoint, config::PRIO_CLASSES)?.unwrap_or(DEFAULT_PRIO_CLASSES as u32);

        // Both are single bytes on the wire, inside the FlowControl frame, so a
        // value that does not fit one is a typo rather than an intent.
        let stmin = get_u8(endpoint, config::STMIN)?;
        let bs = get_u8(endpoint, config::BS)?;

        let ep = IsotpEndpoint {
            device,
            tx_id,
            rx_id,
            eff,
            prio_classes: prio_classes.min(u8::MAX as u32) as u8,
            stmin,
            bs,
        };
        ep.validate(prio_classes)?;
        Ok(ep)
    }

    fn validate(&self, requested_classes: u32) -> ZResult<()> {
        // ISO-TP addresses a peer by a DIRECTED pair. One identifier for both
        // directions would mean a peer receiving its own PDUs and answering its
        // own flow control.
        if self.tx_id == self.rx_id {
            bail!(
                "ISO-TP `{}` and `{}` are both {:#x}, but they must be a directed pair: \
                 this peer's `{}` is the other peer's `{}`",
                config::TX_ID,
                config::RX_ID,
                self.tx_id,
                config::TX_ID,
                config::RX_ID
            );
        }

        // 1 or 8, not any divisor of 8. When a link reports that it supports
        // priorities, zenoh spawns exactly one receive task PER PRIORITY, each
        // reading the socket for its own class. Fewer classes than priorities
        // would put several of those tasks on one socket, racing for the same
        // PDUs. One class per priority, or one socket for everything.
        if !matches!(requested_classes, 1 | 8) {
            bail!(
                "ISO-TP `{}` is {requested_classes}, but only 1 or 8 are valid: zenoh runs one \
                 receive task per priority, so a class must map to exactly one socket",
                config::PRIO_CLASSES
            );
        }

        // Priority classes consume a contiguous block from each base, so the
        // whole block has to fit the identifier space, not merely the base.
        let max = if self.eff { CAN_EFF_MASK } else { CAN_SFF_MASK };
        let span = self.prio_classes as u32 - 1;
        for (key, base) in [(config::TX_ID, self.tx_id), (config::RX_ID, self.rx_id)] {
            // `map_or`, not `is_none_or`: the latter is stable since 1.82 and
            // zenoh's MSRV is 1.75, which CI enforces.
            if base > max || base.checked_add(span).map_or(true, |top| top > max) {
                bail!(
                    "ISO-TP `{key}` block {:#x}..={:#x} does not fit the {}-bit identifier \
                     space (max {max:#x}). {}",
                    base,
                    base.saturating_add(span),
                    if self.eff { 29 } else { 11 },
                    if self.eff {
                        "Lower the base or reduce `prio_classes`."
                    } else {
                        "Set `eff=true` for 29-bit identifiers, or lower the base."
                    }
                );
            }
        }

        // Two blocks that overlap would make a peer receive its own traffic on
        // some classes and not others -- a failure that only appears under load.
        let (lo, hi) = if self.tx_id < self.rx_id {
            (self.tx_id, self.rx_id)
        } else {
            (self.rx_id, self.tx_id)
        };
        if lo + span >= hi {
            bail!(
                "ISO-TP `{}` and `{}` blocks overlap with `{}`={}: {:#x}..={:#x} and {:#x}..={:#x}",
                config::TX_ID,
                config::RX_ID,
                config::PRIO_CLASSES,
                self.prio_classes,
                self.tx_id,
                self.tx_id + span,
                self.rx_id,
                self.rx_id + span
            );
        }

        Ok(())
    }

    /// The identifier pair this link uses for a given traffic class.
    ///
    /// Class 0 is the most urgent and holds the lowest identifier, so it wins
    /// arbitration -- zenoh numbers `Control` at 0 and `Background` at 7, and CAN
    /// gives the bus to the lowest identifier, so the two orderings already
    /// agree and nothing is inverted.
    pub(crate) fn ids_for_class(&self, class: u8) -> (u32, u32) {
        let k = (class as u32).min(self.prio_classes as u32 - 1);
        (self.tx_id + k, self.rx_id + k)
    }

    /// Map one of zenoh's eight priorities onto this link's classes.
    ///
    /// With one class everything shares a socket; with eight the mapping is the
    /// identity, so each priority has its own identifier pair and its own place
    /// in bus arbitration.
    pub(crate) fn class_of(&self, priority: u8) -> u8 {
        if self.prio_classes == 1 {
            0
        } else {
            priority.min(self.prio_classes - 1)
        }
    }

    /// Whether this link maps zenoh priorities onto distinct identifiers.
    pub(crate) fn has_priority_classes(&self) -> bool {
        self.prio_classes > 1
    }

    /// The far end of the pair, as this peer sees it: the identifiers swapped.
    pub(crate) fn peer_locator(&self) -> Locator {
        Locator::from_str(&format!(
            "{ISOTP_LOCATOR_PREFIX}/{}?{}={:#x};{}={:#x}",
            self.device,
            config::TX_ID,
            self.rx_id,
            config::RX_ID,
            self.tx_id
        ))
        .expect("an ISO-TP locator is always well formed")
    }

    pub(crate) fn locator(&self) -> Locator {
        Locator::from_str(&format!(
            "{ISOTP_LOCATOR_PREFIX}/{}?{}={:#x};{}={:#x}",
            self.device,
            config::TX_ID,
            self.tx_id,
            config::RX_ID,
            self.rx_id
        ))
        .expect("an ISO-TP locator is always well formed")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ep(s: &str) -> EndPoint {
        EndPoint::from_str(s).unwrap()
    }

    #[test]
    fn a_directed_pair_parses() {
        let c = IsotpEndpoint::parse(&ep("isotp/vcan0#tx_id=0x7E0;rx_id=0x7E8")).unwrap();
        assert_eq!(c.device, "vcan0");
        assert_eq!((c.tx_id, c.rx_id), (0x7E0, 0x7E8));
        assert!(!c.eff);
        assert_eq!(c.prio_classes, DEFAULT_PRIO_CLASSES);
    }

    #[test]
    fn decimal_and_hex_are_both_accepted() {
        let c = IsotpEndpoint::parse(&ep("isotp/vcan0#tx_id=2016;rx_id=2024")).unwrap();
        assert_eq!((c.tx_id, c.rx_id), (0x7E0, 0x7E8));
    }

    /// Both identifiers are required. Defaulting either one would produce a
    /// link that opens and silently never communicates.
    #[test]
    fn both_identifiers_are_required() {
        assert!(IsotpEndpoint::parse(&ep("isotp/vcan0#tx_id=0x7E0")).is_err());
        assert!(IsotpEndpoint::parse(&ep("isotp/vcan0#rx_id=0x7E8")).is_err());
        assert!(IsotpEndpoint::parse(&ep("isotp/vcan0")).is_err());
    }

    /// The core of ISO-TP addressing: a pair, not an identifier.
    #[test]
    fn one_identifier_for_both_directions_is_refused() {
        let e = IsotpEndpoint::parse(&ep("isotp/vcan0#tx_id=0x7E0;rx_id=0x7E0")).unwrap_err();
        assert!(e.to_string().contains("directed pair"), "{e}");
    }

    #[test]
    fn eleven_bit_identifiers_are_the_default_and_are_enforced() {
        let e = IsotpEndpoint::parse(&ep("isotp/vcan0#tx_id=0x800;rx_id=0x801")).unwrap_err();
        assert!(e.to_string().contains("11-bit"), "{e}");
        // The same pair is fine once extended identifiers are asked for.
        assert!(IsotpEndpoint::parse(&ep("isotp/vcan0#tx_id=0x800;rx_id=0x801;eff=true")).is_ok());
    }

    #[test]
    fn extended_identifiers_have_their_own_ceiling() {
        let e = IsotpEndpoint::parse(&ep("isotp/vcan0#tx_id=0x20000000;rx_id=0x1;eff=true"))
            .unwrap_err();
        assert!(e.to_string().contains("29-bit"), "{e}");
    }

    /// One socket for everything, or one per priority. Anything between would
    /// put several of zenoh's per-priority receive tasks on one socket.
    #[test]
    fn only_one_or_eight_classes_are_valid() {
        for n in [1u32, 8] {
            let s = format!("isotp/vcan0#tx_id=0x100;rx_id=0x200;prio_classes={n}");
            assert!(IsotpEndpoint::parse(&ep(&s)).is_ok(), "{n} should be valid");
        }
        for n in [0u32, 2, 3, 4, 5, 16] {
            let s = format!("isotp/vcan0#tx_id=0x100;rx_id=0x200;prio_classes={n}");
            assert!(
                IsotpEndpoint::parse(&ep(&s)).is_err(),
                "{n} should be refused"
            );
        }
    }

    /// Classes consume a contiguous block, so the block must fit, not just the
    /// base. A base of 0x7FF with 8 classes runs off the end of 11 bits.
    #[test]
    fn a_class_block_that_overruns_the_identifier_space_is_refused() {
        let e = IsotpEndpoint::parse(&ep("isotp/vcan0#tx_id=0x7FF;rx_id=0x100;prio_classes=8"))
            .unwrap_err();
        assert!(e.to_string().contains("does not fit"), "{e}");
    }

    /// Overlapping blocks would make a peer hear itself on some classes only --
    /// a failure that appears under load and not at startup.
    #[test]
    fn overlapping_class_blocks_are_refused() {
        let e = IsotpEndpoint::parse(&ep("isotp/vcan0#tx_id=0x100;rx_id=0x104;prio_classes=8"))
            .unwrap_err();
        assert!(e.to_string().contains("overlap"), "{e}");
        // Far enough apart is fine.
        assert!(
            IsotpEndpoint::parse(&ep("isotp/vcan0#tx_id=0x100;rx_id=0x200;prio_classes=8")).is_ok()
        );
    }

    #[test]
    fn class_zero_is_the_most_urgent_and_holds_the_lowest_identifier() {
        let c = IsotpEndpoint::parse(&ep("isotp/vcan0#tx_id=0x100;rx_id=0x200;prio_classes=8"))
            .unwrap();
        assert_eq!(c.ids_for_class(0), (0x100, 0x200));
        assert_eq!(c.ids_for_class(7), (0x107, 0x207));
        let ids: Vec<u32> = (0..8).map(|k| c.ids_for_class(k).0).collect();
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        assert_eq!(ids, sorted, "more urgent must mean numerically lower");
    }

    /// zenoh's eight priorities fold onto the configured classes in order.
    #[test]
    fn priorities_fold_onto_classes_without_reordering() {
        let one = IsotpEndpoint::parse(&ep("isotp/vcan0#tx_id=0x100;rx_id=0x200")).unwrap();
        for p in 0..8 {
            assert_eq!(one.class_of(p), 0, "one class means everything shares it");
        }
        let eight = IsotpEndpoint::parse(&ep("isotp/vcan0#tx_id=0x100;rx_id=0x200;prio_classes=8"))
            .unwrap();
        for p in 0..8u8 {
            assert_eq!(eight.class_of(p), p);
        }
    }

    #[test]
    fn an_over_long_interface_name_is_refused() {
        let e = IsotpEndpoint::parse(&ep("isotp/vcan0123456789abcdef#tx_id=0x1;rx_id=0x2"))
            .unwrap_err();
        assert!(e.to_string().contains("at most 15"), "{e}");
    }

    #[test]
    fn a_malformed_value_is_an_error_not_a_default() {
        let e = IsotpEndpoint::parse(&ep("isotp/vcan0#tx_id=0x7Eg;rx_id=0x7E8")).unwrap_err();
        assert!(e.to_string().contains("invalid `tx_id` value"), "{e}");
    }

    #[tokio::test]
    async fn the_inspector_reports_a_unicast_best_effort_link() {
        let i = IsotpLocatorInspector;
        let l = Locator::from_str("isotp/vcan0").unwrap();
        assert_eq!(i.protocol(), "isotp");
        assert!(
            !i.is_multicast(&l).await.unwrap(),
            "reporting multicast would lose queries and liveliness, which is the whole point"
        );
        assert!(!i.is_reliable(&l).unwrap());
    }
}
