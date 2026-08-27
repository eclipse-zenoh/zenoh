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
//! A CAN and CAN FD link, interoperating with the zenoh-pico link of the same
//! wire format.
//!
//! A CAN bus is a broadcast medium: every node hears every frame and filters by
//! identifier. That makes this a **multicast** link, not a unicast one. Each
//! peer owns one identifier, transmits on it, accepts frames from every other
//! identifier the mask admits, and drops its own. The sender's identifier is
//! that peer's address.
//!
//! CAN frames are bounded and self-delimiting, so the link is a **datagram**
//! link: zenoh's own transport fragments anything larger than the MTU, and the
//! link never needs segmentation or reassembly of its own.

mod frame;
mod multicast;
#[cfg(target_os = "linux")]
mod sys;

use std::str::FromStr;

use async_trait::async_trait;
pub use multicast::LinkManagerMulticastCan;
use zenoh_link_commons::{LocatorInspector, TCP_SO_RCV_BUF};
use zenoh_protocol::{
    core::{EndPoint, Locator, Metadata, Reliability},
    transport::BatchSize,
};
use zenoh_result::{bail, ZResult};

pub const CAN_LOCATOR_PREFIX: &str = "can";

/// Reliable at frame level -- CRC, ACK slot, automatic retransmission -- but not
/// end to end: controller buffers overrun and a bus-off condition drops
/// everything. zenoh's own reliability covers that.
const IS_RELIABLE: bool = false;

/// The largest MTU the link can ever report, used before a mode is known.
/// `open` replaces it with the MTU for the mode actually obtained.
pub const CAN_MAX_MTU: BatchSize = frame::FD_MTU;

/// `IFNAMSIZ`, including the terminator.
const IFNAMSIZ: usize = 16;

/// Endpoint configuration keys.
///
/// ```text
/// can/<device>#bitrate=500000;dbitrate=2000000;id=0x100;match=0;mask=0
/// ```
///
/// * `device` -- the CAN interface name, e.g. `can0` or `vcan0`
/// * `bitrate` -- arbitration-phase bit rate; also the sole rate for classic CAN
/// * `dbitrate` -- CAN FD data-phase bit rate. Must be non-zero: this link is
///   CAN FD only
/// * `id` -- **this** peer's identifier. It transmits on this, and every other
///   peer sees it as this peer's address
/// * `match` -- accept frames whose `(id & mask) == match`
/// * `mask` -- `0`, the default, accepts every identifier on the bus
///
/// On Linux the bit rates are advisory: rates are set out of band with
/// `ip link set can0 type can bitrate ...` and a virtual interface has none at
/// all. They are still validated, because `dbitrate=0` used to mean classic
/// CAN and now means a misconfiguration.
///
/// # Identifier value is bus priority
///
/// A **lower identifier wins arbitration**, so `id` is a real-time decision and
/// not a name. The peer that must not be delayed needs the lower identifier: on
/// a safety island that is the node publishing the stop command, not the one
/// publishing bulk telemetry.
///
/// The defaults below are a starting point, not an allocation. Two peers that
/// both accept them differ only by whoever was configured first, which is a
/// priority ordering nobody chose.
///
/// Priority is also per **peer**, not per message: one identifier carries all of
/// a peer's traffic.
pub mod config {
    pub const BITRATE: &str = "bitrate";
    pub const DBITRATE: &str = "dbitrate";
    pub const ID: &str = "id";
    pub const MATCH: &str = "match";
    pub const MASK: &str = "mask";
    /// Width of the traffic-class field in the identifier, 0..=3.
    ///
    /// `0`, the default, is the wire zenoh-pico speaks: the identifier is the
    /// peer and nothing else. Above 0 the top `prio_bits` bits carry zenoh's
    /// message priority so that an urgent message wins bus arbitration against
    /// a bulk one -- including one from the same peer. Both ends of a bus must
    /// agree on this, and it costs peer address space.
    pub const PRIO_BITS: &str = "prio_bits";
    /// Receive buffer size in bytes, spelled as the TCP and UDP links spell it.
    ///
    /// Absent, the kernel default applies. Raising it matters only when frames
    /// can arrive faster than the link drains them, which a real bus cannot do
    /// -- 2 Mbit/s of CAN FD is under 2 800 frames per second -- but a virtual
    /// interface can, because it has no bit rate at all. See the README.
    pub use zenoh_link_commons::TCP_SO_RCV_BUF as SO_RCVBUF;
}

pub const DEFAULT_BITRATE: u32 = 500_000;
pub const DEFAULT_DBITRATE: u32 = 2_000_000;
pub const DEFAULT_ID: u32 = 0x100;
pub const DEFAULT_MATCH: u32 = 0;
pub const DEFAULT_MASK: u32 = 0;
/// Off. The identifier is the peer, which is what zenoh-pico expects.
pub const DEFAULT_PRIO_BITS: u8 = 0;

#[derive(Debug, Default, Clone, Copy)]
pub struct CanLocatorInspector;

#[async_trait]
impl LocatorInspector for CanLocatorInspector {
    fn protocol(&self) -> &str {
        CAN_LOCATOR_PREFIX
    }

    /// Always. A CAN bus is a broadcast medium, and declaring otherwise routes
    /// the listen side through the unicast accept path, which needs a socket
    /// and an `accept()` that no datagram medium has.
    async fn is_multicast(&self, _locator: &Locator) -> ZResult<bool> {
        Ok(true)
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

/// A parsed and validated CAN endpoint.
///
/// Parsing and validation happen on every platform, so a malformed endpoint is
/// reported as a malformed endpoint rather than as a missing platform. Only the
/// fields the SocketCAN binding consumes are otherwise unused off Linux.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) struct CanEndpoint {
    pub(crate) device: String,
    pub(crate) bitrate: u32,
    pub(crate) dbitrate: u32,
    pub(crate) id: u32,
    pub(crate) filter_match: u32,
    pub(crate) filter_mask: u32,
    /// `None` leaves the kernel default in place.
    pub(crate) so_rcvbuf: Option<u32>,
    /// Width of the traffic-class field. 0 disables priority mapping.
    pub(crate) prio_bits: u8,
}

/// Parse an unsigned integer, accepting decimal and `0x`-prefixed hex.
///
/// Identifiers are conventionally written in hex and bit rates in decimal, and
/// forcing one notation on both would make every endpoint harder to read.
///
/// Unlike the zenoh-pico parser, a malformed value is an error rather than a
/// silent fall back to the default: a mistyped identifier is a priority
/// misconfiguration, and those must not be quiet.
fn parse_u32(key: &str, s: &str) -> ZResult<u32> {
    let t = s.trim();
    let parsed = match t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        Some(hex) => u32::from_str_radix(hex, 16),
        None => t.parse::<u32>(),
    };
    parsed.map_err(|e| {
        zenoh_result::zerror!("invalid `{key}` value {s:?} on a CAN locator: {e}").into()
    })
}

fn get_u32(endpoint: &EndPoint, key: &str, default: u32) -> ZResult<u32> {
    match endpoint.config().get(key) {
        Some(v) => parse_u32(key, v),
        None => Ok(default),
    }
}

fn get_opt_u32(endpoint: &EndPoint, key: &str) -> ZResult<Option<u32>> {
    endpoint
        .config()
        .get(key)
        .map(|v| parse_u32(key, v))
        .transpose()
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
impl CanEndpoint {
    pub(crate) fn parse(endpoint: &EndPoint) -> ZResult<CanEndpoint> {
        if endpoint.protocol().as_str() != CAN_LOCATOR_PREFIX {
            bail!(
                "not a CAN locator: {} (expected protocol `{CAN_LOCATOR_PREFIX}`)",
                endpoint
            );
        }

        let device = endpoint.address().as_str().to_owned();
        // The address is the interface name. Empty is meaningless, and an
        // over-long one would silently truncate into a different interface, so
        // both are rejected here rather than at open, where the error would be
        // less obvious.
        if device.is_empty() {
            bail!("a CAN locator needs an interface name: {endpoint}");
        }
        if device.len() >= IFNAMSIZ {
            bail!(
                "CAN interface name {device:?} is {} bytes; the kernel allows at most {}",
                device.len(),
                IFNAMSIZ - 1
            );
        }

        let ep = CanEndpoint {
            device,
            bitrate: get_u32(endpoint, config::BITRATE, DEFAULT_BITRATE)?,
            dbitrate: get_u32(endpoint, config::DBITRATE, DEFAULT_DBITRATE)?,
            id: get_u32(endpoint, config::ID, DEFAULT_ID)?,
            filter_match: get_u32(endpoint, config::MATCH, DEFAULT_MATCH)?,
            filter_mask: get_u32(endpoint, config::MASK, DEFAULT_MASK)?,
            so_rcvbuf: get_opt_u32(endpoint, TCP_SO_RCV_BUF)?,
            prio_bits: get_u32(endpoint, config::PRIO_BITS, DEFAULT_PRIO_BITS as u32)? as u8,
        };
        ep.validate()?;
        Ok(ep)
    }

    fn validate(&self) -> ZResult<()> {
        // This link is CAN FD only. `dbitrate=0` used to select classic CAN,
        // whose 7-byte MTU is smaller than zenoh's per-fragment overhead, so a
        // classic link could never carry a session. Refusing beats shipping a
        // mode whose only symptom is a hang.
        if self.dbitrate == 0 {
            bail!(
                "CAN `{}` is 0, which selected classic CAN. This link is CAN FD only: a \
                 7-byte MTU is smaller than zenoh's per-fragment overhead, so no session \
                 could make progress. Set a data-phase bit rate, e.g. `{}=2000000`",
                config::DBITRATE,
                config::DBITRATE
            );
        }

        if self.prio_bits > frame::PRIO_BITS_MAX {
            bail!(
                "CAN `{}` is {}, but zenoh has 8 priorities so at most {} bits are useful",
                config::PRIO_BITS,
                self.prio_bits,
                frame::PRIO_BITS_MAX
            );
        }

        // Only 11-bit identifiers are expressible: the sender never sets
        // CAN_EFF_FLAG, so a larger value would silently become a different
        // identifier on the wire. Traffic-class bits come
        // out of that same 11, so they shrink the peer space.
        let max_peer = frame::max_peer_id(self.prio_bits);
        for (key, value) in [
            (config::ID, self.id),
            (config::MATCH, self.filter_match),
            (config::MASK, self.filter_mask),
        ] {
            if value > max_peer {
                if self.prio_bits == 0 {
                    bail!(
                        "CAN `{key}` is {value:#x}, above the 11-bit maximum {max_peer:#x}; \
                         extended identifiers are not part of this wire format"
                    );
                }
                bail!(
                    "CAN `{key}` is {value:#x}, but `{}={}` reserves the top {} bits of the \
                     identifier for the traffic class, leaving {max_peer:#x} for the peer",
                    config::PRIO_BITS,
                    self.prio_bits,
                    self.prio_bits
                );
            }
        }

        // A peer that filtered out its own identifier would never be reachable
        // by anyone, which is a configuration error rather than a quiet
        // degradation.
        if self.filter_mask != 0 && (self.id & self.filter_mask) != self.filter_match {
            bail!(
                "CAN `id` {:#x} lies outside its own `match`/`mask` band ({:#x}/{:#x}), \
                 so no peer could address it",
                self.id,
                self.filter_match,
                self.filter_mask
            );
        }

        Ok(())
    }

    /// The locator naming a peer on this bus, which is how the multicast
    /// transport tells peers apart. Derived locally from the frame identifier
    /// and never transmitted.
    ///
    /// The identifier goes in the locator's **metadata**, not its config: a
    /// `Locator` is an `EndPoint` with the config truncated away
    /// (`From<EndPoint> for Locator`), so a config-borne identifier would be
    /// silently dropped and every peer would look alike.
    pub(crate) fn peer_locator(&self, id: u32) -> Locator {
        Locator::new(
            CAN_LOCATOR_PREFIX,
            &self.device,
            format!("{}={:#x}", config::ID, id),
        )
        .expect("a CAN peer locator is always well formed")
    }

    /// The locator naming the identifier band this link listens to, which is
    /// what the transport manager keys the multicast transport by. Metadata
    /// rather than config, for the same reason as [`Self::peer_locator`].
    pub(crate) fn group_locator(&self) -> Locator {
        Locator::new(
            CAN_LOCATOR_PREFIX,
            &self.device,
            format!(
                "{}={:#x};{}={:#x}",
                config::MATCH,
                self.filter_match,
                config::MASK,
                self.filter_mask
            ),
        )
        .expect("a CAN group locator is always well formed")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ep(s: &str) -> EndPoint {
        EndPoint::from_str(s).unwrap()
    }

    #[test]
    fn defaults_apply_when_keys_are_absent() {
        let c = CanEndpoint::parse(&ep("can/vcan0")).unwrap();
        assert_eq!(c.device, "vcan0");
        assert_eq!(c.bitrate, DEFAULT_BITRATE);
        assert_eq!(c.dbitrate, DEFAULT_DBITRATE);
        assert_eq!(c.id, DEFAULT_ID);
        assert_eq!(c.filter_match, DEFAULT_MATCH);
        assert_eq!(c.filter_mask, DEFAULT_MASK);
    }

    #[test]
    fn the_full_grammar_parses() {
        let c = CanEndpoint::parse(&ep(
            "can/can0#bitrate=500000;dbitrate=2000000;id=0x101;match=0x100;mask=0x700",
        ))
        .unwrap();
        assert_eq!(c.device, "can0");
        assert_eq!(c.id, 0x101);
        assert_eq!(c.filter_match, 0x100);
        assert_eq!(c.filter_mask, 0x700);
    }

    #[test]
    fn decimal_and_hex_are_both_accepted() {
        let c = CanEndpoint::parse(&ep("can/vcan0#id=257")).unwrap();
        assert_eq!(c.id, 0x101);
    }

    #[test]
    fn priority_bits_default_to_off() {
        assert_eq!(
            CanEndpoint::parse(&ep("can/vcan0")).unwrap().prio_bits,
            DEFAULT_PRIO_BITS
        );
        assert_eq!(
            CanEndpoint::parse(&ep("can/vcan0#id=0x0A;prio_bits=3"))
                .unwrap()
                .prio_bits,
            3
        );
    }

    #[test]
    fn too_many_priority_bits_are_refused() {
        let e = CanEndpoint::parse(&ep("can/vcan0#id=0x0A;prio_bits=4")).unwrap_err();
        assert!(e.to_string().contains("at most 3 bits"), "{e}");
    }

    /// The class field eats peer space, and an id that no longer fits has to
    /// say so rather than silently colliding with another class.
    #[test]
    fn an_id_that_no_longer_fits_is_refused_with_the_budget() {
        let e = CanEndpoint::parse(&ep("can/vcan0#id=0x101;prio_bits=3")).unwrap_err();
        assert!(e.to_string().contains("reserves the top 3 bits"), "{e}");
        assert!(e.to_string().contains("0xff"), "{e}");
        // The same id is fine with two class bits, which leave 9 for the peer.
        assert!(CanEndpoint::parse(&ep("can/vcan0#id=0x101;prio_bits=2")).is_ok());
    }

    #[test]
    fn the_receive_buffer_defaults_to_the_kernels() {
        assert_eq!(
            CanEndpoint::parse(&ep("can/vcan0")).unwrap().so_rcvbuf,
            None
        );
        assert_eq!(
            CanEndpoint::parse(&ep("can/vcan0#so_rcvbuf=8388608"))
                .unwrap()
                .so_rcvbuf,
            Some(8 * 1024 * 1024)
        );
        assert!(CanEndpoint::parse(&ep("can/vcan0#so_rcvbuf=lots")).is_err());
    }

    /// Classic CAN is refused, and the message says why rather than leaving
    /// someone to discover a 7-byte MTU at runtime.
    #[test]
    fn a_zero_dbitrate_is_refused() {
        let e = CanEndpoint::parse(&ep("can/vcan0#dbitrate=0")).unwrap_err();
        assert!(e.to_string().contains("CAN FD only"), "{e}");
    }

    #[test]
    fn a_malformed_value_is_an_error_not_a_default() {
        let e = CanEndpoint::parse(&ep("can/vcan0#id=0x10g")).unwrap_err();
        assert!(e.to_string().contains("invalid `id` value"), "{e}");
    }

    #[test]
    fn an_empty_interface_name_is_refused() {
        // zenoh's own endpoint parser rejects an empty address before the link
        // ever sees it. The link keeps its own guard for callers that build an
        // EndPoint by some other route.
        assert!(EndPoint::from_str("can/").is_err());
    }

    #[test]
    fn an_over_long_interface_name_is_refused() {
        let e = CanEndpoint::parse(&ep("can/vcan0123456789abcdef")).unwrap_err();
        assert!(e.to_string().contains("at most 15"), "{e}");
    }

    #[test]
    fn extended_identifiers_are_refused_with_a_reason() {
        let e = CanEndpoint::parse(&ep("can/vcan0#id=0x800")).unwrap_err();
        assert!(e.to_string().contains("11-bit maximum"), "{e}");
    }

    #[test]
    fn an_id_outside_its_own_band_is_refused() {
        let e = CanEndpoint::parse(&ep("can/vcan0#id=0x201;match=0x100;mask=0x700")).unwrap_err();
        assert!(e.to_string().contains("outside its own"), "{e}");
    }

    #[test]
    fn an_id_inside_its_own_band_is_accepted() {
        assert!(CanEndpoint::parse(&ep("can/vcan0#id=0x101;match=0x100;mask=0x700")).is_ok());
    }

    #[test]
    fn a_peer_locator_carries_the_sender_identifier() {
        let c = CanEndpoint::parse(&ep("can/vcan0")).unwrap();
        let l = c.peer_locator(0x101);
        assert_eq!(l.protocol().as_str(), CAN_LOCATOR_PREFIX);
        assert_eq!(l.address().as_str(), "vcan0");
        assert_eq!(l.metadata().get(config::ID), Some("0x101"));
    }

    /// The multicast transport tells peers apart by comparing these locators,
    /// so two identifiers must never produce the same one.
    #[test]
    fn peer_locators_distinguish_peers() {
        let c = CanEndpoint::parse(&ep("can/vcan0")).unwrap();
        assert_ne!(c.peer_locator(0x100), c.peer_locator(0x101));
    }

    /// A `Locator` is an `EndPoint` with the config truncated, which is why the
    /// identifier lives in the metadata. This pins that reasoning: had it gone
    /// in the config, it would vanish here.
    #[test]
    fn a_config_borne_identifier_would_be_lost() {
        let e = ep("can/vcan0#id=0x101");
        assert_eq!(e.config().get(config::ID), Some("0x101"));
        assert_eq!(e.to_locator().metadata().get(config::ID), None);
    }

    /// Bands differ, so the transports keyed by them differ.
    #[test]
    fn group_locators_distinguish_bands() {
        let a = CanEndpoint::parse(&ep("can/vcan0#id=0x101;match=0x100;mask=0x700")).unwrap();
        let b = CanEndpoint::parse(&ep("can/vcan0#id=0x201;match=0x200;mask=0x700")).unwrap();
        assert_ne!(a.group_locator(), b.group_locator());
        assert_eq!(a.group_locator().address().as_str(), "vcan0");
    }

    #[tokio::test]
    async fn the_inspector_reports_a_multicast_best_effort_link() {
        let i = CanLocatorInspector;
        let l = Locator::from_str("can/vcan0").unwrap();
        assert_eq!(i.protocol(), "can");
        assert!(i.is_multicast(&l).await.unwrap());
        assert!(!i.is_reliable(&l).unwrap());
    }
}
