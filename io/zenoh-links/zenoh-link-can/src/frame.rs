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

//! The CAN link wire format.
//!
//! This module deliberately contains no I/O and no platform types, so every
//! rule below is a unit test that runs anywhere, with no `vcan0` and no root.
//!
//! One CAN frame carries one zenoh datagram. CAN FD payload lengths are
//! quantised -- the DLC encodes 0..=8, 12, 16, 20, 24, 32, 48, 64 and nothing
//! between -- so a 40-byte datagram travels in a 48-byte frame and the receiver
//! cannot recover the true length from the frame alone. Byte 0 of every payload
//! is therefore the datagram length and bytes 1..=N are the datagram.
//!
//! The format is fixed by the zenoh-pico implementation it interoperates with,
//! and is not ours to change unilaterally.

use core::fmt;

use zenoh_protocol::transport::BatchSize;

/// Byte 0 of the frame payload carries the true datagram length.
pub(crate) const LEN_PREFIX: usize = 1;

/// `CANFD_MAX_DLEN`.
pub(crate) const FD_MAX_DLEN: usize = 64;
/// Frame lengths 0..=8 are all representable; above that only the DLC steps are.
const CONTIGUOUS_DLEN: usize = 8;

/// Usable datagram bytes once the length prefix is subtracted.
///
/// This link is **CAN FD only**. On classic CAN a frame carries 8 bytes, and
/// the length prefix is not needed there because the DLC encodes 0..=8 exactly,
/// so the payload is 8. That is still not enough: a `Join` is 33 bytes at its
/// smallest, and transport messages are never fragmented -- they must fit one
/// batch whole or the session cannot open. A classic link is not a slower link,
/// it is one that never establishes. Refusing it at open beats shipping a path
/// that can only ever hang.
pub(crate) const FD_MTU: BatchSize = (FD_MAX_DLEN - LEN_PREFIX) as BatchSize; // 63

/// `sizeof(struct canfd_frame)`, which is the read and write size the kernel
/// uses for CAN FD. A 16-byte read is a classic frame and is not ours.
pub(crate) const CANFD_MTU_WIRE: usize = 72;

/// `CANFD_BRS` -- use the fast data phase.
pub(crate) const CANFD_BRS: u8 = 0x01;

pub(crate) const CAN_SFF_MASK: u32 = 0x0000_07FF;
pub(crate) const CAN_EFF_MASK: u32 = 0x1FFF_FFFF;

/// The representable CAN FD frame lengths above 8.
const FD_DLC_STEPS: [u8; 7] = [12, 16, 20, 24, 32, 48, 64];

/// Identifiers are 11 bits, so this is the whole address space to divide up.
pub(crate) const ID_BITS: u8 = 11;

/// zenoh has 8 priorities, so a fully priority-major layout costs 3 bits.
pub(crate) const PRIO_BITS_MAX: u8 = 3;

/// The mask that recovers a peer's identity from a frame identifier.
///
/// With no priority bits the identifier *is* the peer, and the mask matches
/// what zenoh-pico applies, so the two implementations agree. With `n` priority
/// bits the top `n` bits are the traffic class and only the rest identify the
/// peer -- otherwise one peer would look like `2^n` different peers, one per
/// priority it happens to transmit at.
pub(crate) const fn peer_mask(prio_bits: u8) -> u32 {
    if prio_bits == 0 {
        CAN_EFF_MASK
    } else {
        (1u32 << (ID_BITS - prio_bits)) - 1
    }
}

/// The largest peer identifier expressible alongside `prio_bits` class bits.
pub(crate) const fn max_peer_id(prio_bits: u8) -> u32 {
    (1u32 << (ID_BITS - prio_bits)) - 1
}

/// The identifier `peer` transmits on for a batch of the given zenoh priority.
///
/// The class occupies the **most significant** bits, so it dominates
/// arbitration: a lower identifier wins the bus, and zenoh numbers its
/// priorities with `Control` at 0 and `Background` at 7, so the two orderings
/// already agree and nothing has to be inverted.
///
/// With fewer than 3 class bits the 8 priorities are folded onto `2^n` classes
/// by dropping the low bits, which keeps the ordering and merges neighbours.
pub(crate) fn tx_id(peer: u32, prio_bits: u8, priority: u8) -> u32 {
    if prio_bits == 0 {
        return peer;
    }
    let class = (priority >> (PRIO_BITS_MAX - prio_bits)) as u32;
    (class << (ID_BITS - prio_bits)) | peer
}

/// How the receiver decides which frames are its own, which are in band, and
/// which peer sent one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RxFilter {
    /// This peer's identifier, in the peer field.
    pub(crate) own: u32,
    pub(crate) match_: u32,
    pub(crate) mask: u32,
    pub(crate) prio_bits: u8,
}

/// `struct canfd_frame` from `<linux/can.h>`.
///
/// Declared here rather than taken from `libc` because `libc`'s version keeps
/// its reserved fields private, so it cannot be constructed. The layout is
/// asserted against `libc`'s in [`assert_layout_matches_libc`] on Linux.
///
/// In classic mode only the first [`CAN_MTU_WIRE`] bytes are written, and they
/// overlay `struct can_frame` exactly: `len` lands on `can_dlc`, `flags` -- zero
/// in classic mode -- lands on `__pad`, and `res1` lands on `len8_dlc`.
#[repr(C, align(8))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Frame {
    pub(crate) can_id: u32,
    pub(crate) len: u8,
    pub(crate) flags: u8,
    pub(crate) res0: u8,
    pub(crate) res1: u8,
    pub(crate) data: [u8; FD_MAX_DLEN],
}

impl Frame {
    pub(crate) const fn zeroed() -> Self {
        Frame {
            can_id: 0,
            len: 0,
            flags: 0,
            res0: 0,
            res1: 0,
            data: [0u8; FD_MAX_DLEN],
        }
    }

    /// The first `wire` bytes of the frame, as they go to `write(2)`.
    ///
    /// `Frame` is `repr(C)` with no interior padding -- 4 + 1 + 1 + 1 + 1 + 64 is
    /// exactly 72, and 72 is a multiple of the 8-byte alignment -- so every byte
    /// of the struct is an initialised field byte.
    pub(crate) fn as_wire_bytes(&self, wire: usize) -> &[u8] {
        debug_assert!(wire <= core::mem::size_of::<Frame>());
        // SAFETY: `Frame` is `repr(C)` and padding-free, so it is valid to read
        // as a byte slice, and `wire` is bounded by its size.
        unsafe { core::slice::from_raw_parts(self as *const Frame as *const u8, wire) }
    }
}

/// Refusal reasons for [`encode`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TxError {
    /// The datagram does not fit one frame. zenoh's transport fragments to the
    /// link MTU before the link is called, so this is a bug at the call site
    /// rather than an expected runtime condition.
    TooLarge { len: usize },
    /// Only 11-bit identifiers are expressible: the sender never sets
    /// `CAN_EFF_FLAG`, so a larger value would silently become a different
    /// identifier on the wire.
    IdentifierTooWide { id: u32 },
}

impl fmt::Display for TxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TxError::TooLarge { len } => {
                write!(
                    f,
                    "datagram of {len} bytes exceeds the CAN link MTU of {FD_MTU}"
                )
            }
            TxError::IdentifierTooWide { id } => write!(
                f,
                "CAN identifier {id:#x} exceeds the 11-bit range (max {CAN_SFF_MASK:#x}); \
                 extended identifiers are not part of this wire format"
            ),
        }
    }
}

/// Why a received frame was not delivered to the transport.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RxDrop {
    /// Not a `canfd_frame`: a classic frame from some other device on the bus,
    /// a runt, or an error frame.
    NotAFrame { nread: usize },
    /// Our own transmission, heard back on a loopback-enabled interface.
    OwnFrame,
    /// Outside the identifier band this bus reserves for zenoh.
    Filtered { sender: u32 },
    /// No room for even the length prefix.
    NoLengthByte,
    /// The length byte disagrees with the frame length.
    BadLength { declared: usize, available: usize },
    /// The transport's buffer cannot hold the datagram. Dropping beats handing
    /// back a truncated datagram that would deserialise as garbage.
    BufferTooSmall { needed: usize, have: usize },
}

impl fmt::Display for RxDrop {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RxDrop::NotAFrame { nread } => {
                write!(f, "read of {nread} bytes is not a CAN FD frame")
            }
            RxDrop::OwnFrame => write!(f, "own transmission"),
            RxDrop::Filtered { sender } => write!(f, "identifier {sender:#x} outside the band"),
            RxDrop::NoLengthByte => write!(f, "frame carries no length byte"),
            RxDrop::BadLength {
                declared,
                available,
            } => write!(
                f,
                "length byte declares {declared} bytes but only {available} are present"
            ),
            RxDrop::BufferTooSmall { needed, have } => {
                write!(
                    f,
                    "datagram of {needed} bytes does not fit a {have}-byte buffer"
                )
            }
        }
    }
}

/// Round a payload length up to the next representable CAN FD frame length.
///
/// Below 9 bytes every length is representable and the payload is used as is;
/// above it only the DLC steps exist.
pub(crate) fn fd_frame_len(payload: usize) -> u8 {
    if payload <= CONTIGUOUS_DLEN {
        return payload as u8;
    }
    for step in FD_DLC_STEPS {
        if payload <= step as usize {
            return step;
        }
    }
    // Unreachable for any payload <= 64, which `encode` has already enforced.
    FD_MAX_DLEN as u8
}

/// Build the frame carrying `datagram`, and the number of bytes to write.
pub(crate) fn encode(id: u32, datagram: &[u8]) -> Result<(Frame, usize), TxError> {
    if id > CAN_SFF_MASK {
        return Err(TxError::IdentifierTooWide { id });
    }
    if datagram.len() > FD_MTU as usize {
        return Err(TxError::TooLarge {
            len: datagram.len(),
        });
    }

    let mut frame = Frame::zeroed();
    frame.can_id = id;
    frame.data[0] = datagram.len() as u8;
    frame.data[LEN_PREFIX..LEN_PREFIX + datagram.len()].copy_from_slice(datagram);

    // The bit-rate switch is requested for every frame, not only the long ones:
    // the data phase is where the rate gain is.
    frame.flags = CANFD_BRS;
    frame.len = fd_frame_len(datagram.len() + LEN_PREFIX);

    Ok((frame, CANFD_MTU_WIRE))
}

/// Decide what a received frame is, and copy out the datagram if it is one.
///
/// The rules are applied in the same order as the zenoh-pico receiver, so the
/// two implementations drop the same frames for the same reasons.
pub(crate) fn decode(
    frame: &Frame,
    nread: usize,
    filter: &RxFilter,
    out: &mut [u8],
) -> Result<(usize, u32), RxDrop> {
    // Only CAN FD. A 16-byte read is a classic frame, which on a shared bus is
    // some other device's traffic and never ours.
    if nread != CANFD_MTU_WIRE {
        return Err(RxDrop::NotAFrame { nread });
    }

    // The peer field, not the whole identifier: with priority bits in play the
    // same peer transmits under several identifiers and they must all resolve
    // to one address.
    let sender = frame.can_id & peer_mask(filter.prio_bits);
    if sender == filter.own {
        return Err(RxDrop::OwnFrame);
    }
    if filter.mask != 0 && (sender & filter.mask) != filter.match_ {
        return Err(RxDrop::Filtered { sender });
    }

    let frame_len = frame.len as usize;
    if frame_len < LEN_PREFIX {
        return Err(RxDrop::NoLengthByte);
    }

    let declared = frame.data[0] as usize;
    let available = frame_len - LEN_PREFIX;
    if declared > available {
        return Err(RxDrop::BadLength {
            declared,
            available,
        });
    }
    if declared > out.len() {
        return Err(RxDrop::BufferTooSmall {
            needed: declared,
            have: out.len(),
        });
    }

    out[..declared].copy_from_slice(&frame.data[LEN_PREFIX..LEN_PREFIX + declared]);
    Ok((declared, sender))
}

/// A receive filter with no priority bits, which is the wire zenoh-pico speaks.
#[cfg(test)]
fn f(own: u32, match_: u32, mask: u32) -> RxFilter {
    RxFilter {
        own,
        match_,
        mask,
        prio_bits: 0,
    }
}

#[cfg(all(test, target_os = "linux"))]
#[test]
fn assert_layout_matches_libc() {
    assert_eq!(
        core::mem::size_of::<Frame>(),
        core::mem::size_of::<libc::canfd_frame>()
    );
    assert_eq!(
        core::mem::align_of::<Frame>(),
        core::mem::align_of::<libc::canfd_frame>()
    );
    assert_eq!(core::mem::size_of::<Frame>(), CANFD_MTU_WIRE);
    assert_eq!(libc::CANFD_BRS as u8, CANFD_BRS);
    assert_eq!(libc::CAN_SFF_MASK, CAN_SFF_MASK);
    assert_eq!(libc::CAN_EFF_MASK, CAN_EFF_MASK);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn datagram(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i + 1) as u8).collect()
    }

    #[test]
    fn the_mtu_is_the_frame_size_less_the_prefix() {
        assert_eq!(FD_MTU, 63);
    }

    /// Every DLC boundary. The left column is the
    /// datagram length; the right is the resulting `frame.len`, which includes
    /// the length prefix.
    #[test]
    fn fd_dlc_steps() {
        let expected: &[(usize, u8)] = &[
            (0, 1),
            (7, 8),
            // payload 9 is past the last contiguous length, so it rounds
            (8, 12),
            (11, 12),
            (12, 16),
            (15, 16),
            (16, 20),
            (19, 20),
            (20, 24),
            (23, 24),
            (24, 32),
            (31, 32),
            (32, 48),
            (47, 48),
            (48, 64),
            (62, 64),
            (63, 64),
        ];
        for (len, frame_len) in expected {
            let (frame, wire) = encode(0x100, &datagram(*len)).unwrap();
            assert_eq!(frame.len, *frame_len, "datagram of {len} bytes");
            assert_eq!(wire, CANFD_MTU_WIRE);
        }
    }

    #[test]
    fn brs_is_set_on_every_frame_including_short_ones() {
        for len in [0usize, 1, 7, 8, 63] {
            let (frame, _) = encode(0x100, &datagram(len)).unwrap();
            assert_eq!(frame.flags, CANFD_BRS, "datagram of {len} bytes");
        }
    }

    #[test]
    fn over_mtu_is_refused() {
        assert_eq!(
            encode(0x100, &datagram(64)),
            Err(TxError::TooLarge { len: 64 })
        );
        assert!(encode(0x100, &datagram(63)).is_ok());
    }

    /// A classic frame is a 16-byte read. This link does not speak classic, so
    /// such a frame is another device's traffic and must be dropped rather than
    /// decoded -- the bytes would parse, and mean nothing.
    #[test]
    fn classic_frames_are_not_ours() {
        let (frame, _) = encode(0x101, &datagram(4)).unwrap();
        let mut out = [0u8; FD_MAX_DLEN];
        assert_eq!(
            decode(&frame, 16, &f(0x100, 0, 0), &mut out),
            Err(RxDrop::NotAFrame { nread: 16 })
        );
    }

    #[test]
    fn extended_identifiers_are_refused() {
        assert_eq!(
            encode(0x800, &datagram(1)),
            Err(TxError::IdentifierTooWide { id: 0x800 })
        );
        assert!(encode(CAN_SFF_MASK, &datagram(1)).is_ok());
    }

    #[test]
    fn round_trip_every_length() {
        for len in 0..=FD_MTU as usize {
            let sent = datagram(len);
            let (frame, wire) = encode(0x101, &sent).unwrap();
            let mut out = [0u8; FD_MAX_DLEN];
            let (n, sender) = decode(&frame, wire, &f(0x100, 0, 0), &mut out).unwrap();
            assert_eq!(n, len);
            assert_eq!(sender, 0x101);
            assert_eq!(&out[..n], &sent[..]);
        }
    }

    #[test]
    fn padding_between_the_datagram_and_the_frame_end_is_zero() {
        // A 12-byte datagram occupies 13 bytes and travels in a 16-byte frame.
        let (frame, _) = encode(0x101, &datagram(12)).unwrap();
        assert_eq!(frame.len, 16);
        assert!(frame.data[13..].iter().all(|b| *b == 0));
    }

    #[test]
    fn own_frames_are_dropped() {
        let (frame, wire) = encode(0x100, &datagram(4)).unwrap();
        let mut out = [0u8; FD_MAX_DLEN];
        assert_eq!(
            decode(&frame, wire, &f(0x100, 0, 0), &mut out),
            Err(RxDrop::OwnFrame)
        );
    }

    #[test]
    fn a_zero_mask_accepts_every_identifier() {
        let (frame, wire) = encode(0x7FF, &datagram(4)).unwrap();
        let mut out = [0u8; FD_MAX_DLEN];
        assert!(decode(&frame, wire, &f(0x100, 0, 0), &mut out).is_ok());
    }

    #[test]
    fn a_nonzero_mask_rejects_outside_the_band() {
        let mut out = [0u8; FD_MAX_DLEN];
        // Band 0x100..=0x1FF.
        let (inside, wire) = encode(0x1AB, &datagram(4)).unwrap();
        assert!(decode(&inside, wire, &f(0x100, 0x100, 0x700), &mut out).is_ok());

        let (outside, wire) = encode(0x2AB, &datagram(4)).unwrap();
        assert_eq!(
            decode(&outside, wire, &f(0x100, 0x100, 0x700), &mut out),
            Err(RxDrop::Filtered { sender: 0x2AB })
        );
    }

    #[test]
    fn a_read_that_is_not_a_frame_is_dropped() {
        let (frame, _) = encode(0x101, &datagram(4)).unwrap();
        let mut out = [0u8; FD_MAX_DLEN];
        for nread in [0usize, 1, 15, 16, 17, 71, 73] {
            assert_eq!(
                decode(&frame, nread, &f(0x100, 0, 0), &mut out),
                Err(RxDrop::NotAFrame { nread })
            );
        }
    }

    #[test]
    fn a_frame_with_no_length_byte_is_dropped() {
        let mut frame = Frame::zeroed();
        frame.can_id = 0x101;
        frame.len = 0;
        let mut out = [0u8; FD_MAX_DLEN];
        assert_eq!(
            decode(&frame, CANFD_MTU_WIRE, &f(0x100, 0, 0), &mut out),
            Err(RxDrop::NoLengthByte)
        );
    }

    #[test]
    fn a_length_byte_that_overruns_the_frame_is_dropped() {
        let (mut frame, wire) = encode(0x101, &datagram(4)).unwrap();
        frame.data[0] = 63; // frame.len is 5, so only 4 bytes are present
        let mut out = [0u8; FD_MAX_DLEN];
        assert_eq!(
            decode(&frame, wire, &f(0x100, 0, 0), &mut out),
            Err(RxDrop::BadLength {
                declared: 63,
                available: 4
            })
        );
    }

    #[test]
    fn a_datagram_larger_than_the_buffer_is_dropped_not_truncated() {
        let (frame, wire) = encode(0x101, &datagram(20)).unwrap();
        let mut out = [0u8; 8];
        assert_eq!(
            decode(&frame, wire, &f(0x100, 0, 0), &mut out),
            Err(RxDrop::BufferTooSmall {
                needed: 20,
                have: 8
            })
        );
    }

    #[test]
    fn wire_bytes_start_with_the_identifier_little_endian() {
        let (frame, wire) = encode(0x123, &datagram(3)).unwrap();
        let bytes = frame.as_wire_bytes(wire);
        assert_eq!(bytes.len(), CANFD_MTU_WIRE);
        assert_eq!(&bytes[..4], &[0x23, 0x01, 0x00, 0x00]);
        assert_eq!(bytes[4], 4, "frame.len is the datagram plus its prefix");
        assert_eq!(bytes[5], CANFD_BRS);
        assert_eq!(&bytes[8..12], &[3, 1, 2, 3]);
    }
}

/// Golden frames: the exact bytes the zenoh-pico sender puts on the wire.
///
/// These are hand-derived from `_z_send_can` in
/// `src/system/unix/network.c` of the vendored zenoh-pico tree, not produced by
/// [`encode`]. That is the point -- they are what makes the two implementations
/// one wire format rather than two, and an interop regression fails here rather
/// than on a bus.
///
/// Layout, for reading the tables below:
/// `[0..4]` `can_id`, little-endian and native -- the C code assigns
/// `frame.can_id = sock->_id` with no byte swap; `[4]` `frame.len`;
/// `[5]` `frame.flags`; `[6]` `__res0`; `[7]` `__res1`; `[8]` the datagram
/// length prefix; `[9..]` the datagram, then zeros.
///
/// Every byte past the datagram is zero because `_z_send_can` opens with
/// `memset(&frame, 0, sizeof(frame))` and never writes the reserved fields or
/// the DLC padding afterwards.
#[cfg(test)]
mod golden {
    use super::*;

    /// Pad a frame prefix out to its full wire length with zeros.
    fn padded(prefix: &[u8], wire: usize) -> Vec<u8> {
        let mut v = prefix.to_vec();
        v.resize(wire, 0);
        v
    }

    fn assert_golden(id: u32, datagram: &[u8], expected: &[u8]) {
        let (frame, wire) = encode(id, datagram).unwrap();
        assert_eq!(wire, expected.len(), "wire length");
        assert_eq!(
            frame.as_wire_bytes(wire),
            expected,
            "frame bytes for a {}-byte datagram on {:#x}",
            datagram.len(),
            id
        );
    }

    /// `_z_send_can(sock{id=0x100, fd_mode=true}, ptr, 0)`.
    ///
    /// `payload = 1`, which is `<= 8`, so `frame_len` stays 1 and only the
    /// `else if (sock->_fd_mode)` arm runs -- BRS is still set.
    #[test]
    fn empty_datagram_fd() {
        assert_golden(
            0x100,
            &[],
            &padded(
                &[0x00, 0x01, 0x00, 0x00, 0x01, CANFD_BRS, 0x00, 0x00, 0x00],
                CANFD_MTU_WIRE,
            ),
        );
    }

    /// `_z_send_can(sock{id=0x100, fd_mode=true}, "\x01..\x05", 5)`.
    ///
    /// `payload = 6`, still `<= 8`, so no DLC round-up.
    #[test]
    fn short_datagram_fd() {
        assert_golden(
            0x100,
            &[1, 2, 3, 4, 5],
            &padded(
                &[
                    0x00, 0x01, 0x00, 0x00, 0x06, CANFD_BRS, 0x00, 0x00, //
                    0x05, 1, 2, 3, 4, 5,
                ],
                CANFD_MTU_WIRE,
            ),
        );
    }

    /// `_z_send_can(sock{id=0x101, fd_mode=true}, "\x01..\x0B", 11)`.
    ///
    /// `payload = 12`, and the round-up loop compares `payload <= steps[i]`, so
    /// 12 selects the 12-byte step exactly rather than rounding on to 16.
    #[test]
    fn datagram_landing_exactly_on_a_dlc_step() {
        assert_golden(
            0x101,
            &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
            &padded(
                &[
                    0x01, 0x01, 0x00, 0x00, 0x0C, CANFD_BRS, 0x00, 0x00, //
                    0x0B, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11,
                ],
                CANFD_MTU_WIRE,
            ),
        );
    }

    /// `_z_send_can(sock{id=0x101, fd_mode=true}, "\x01..\x0C", 12)`.
    ///
    /// `payload = 13` rounds up to the 16-byte step, so `frame.len` is 16 while
    /// only 13 bytes are meaningful. The three pad bytes are zero, and the
    /// receiver recovers the true length from the prefix rather than the DLC --
    /// which is the whole reason the prefix exists.
    #[test]
    fn datagram_that_must_round_up() {
        assert_golden(
            0x101,
            &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12],
            &padded(
                &[
                    0x01, 0x01, 0x00, 0x00, 0x10, CANFD_BRS, 0x00, 0x00, //
                    0x0C, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12,
                ],
                CANFD_MTU_WIRE,
            ),
        );
    }

    /// `_z_send_can(sock{id=0x200, fd_mode=true}, "\x01..\x3F", 63)`.
    ///
    /// A full-MTU datagram: `payload = 64` fills the frame exactly, so there is
    /// no padding at all.
    #[test]
    fn full_mtu_datagram_fd() {
        let datagram: Vec<u8> = (1..=63).collect();
        let mut expected = vec![0x00, 0x02, 0x00, 0x00, 0x40, CANFD_BRS, 0x00, 0x00, 0x3F];
        expected.extend_from_slice(&datagram);
        assert_eq!(expected.len(), CANFD_MTU_WIRE, "the frame is exactly full");
        assert_golden(0x200, &datagram, &expected);
    }

    /// The receiver's rules, in the order `_z_read_can` applies them.
    ///
    /// A datagram shorter than the DLC is legal -- that is exactly what the
    /// round-up case produces -- so the check is `declared > available`, not
    /// `declared != available`.
    #[test]
    fn a_datagram_shorter_than_its_dlc_is_legal() {
        let (frame, wire) = encode(0x101, &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]).unwrap();
        assert_eq!(frame.len, 16);
        let mut out = [0u8; FD_MAX_DLEN];
        let (n, sender) = decode(&frame, wire, &f(0x100, 0, 0), &mut out).unwrap();
        assert_eq!((n, sender), (12, 0x101));
        assert_eq!(&out[..n], &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]);
    }

    /// `mask == 0` short-circuits, so `match` is never consulted and every
    /// identifier on the bus is admitted.
    #[test]
    fn a_zero_mask_ignores_match_entirely() {
        let (frame, wire) = encode(0x7FF, &[1]).unwrap();
        let mut out = [0u8; FD_MAX_DLEN];
        assert!(decode(&frame, wire, &f(0x100, 0x555, 0), &mut out).is_ok());
    }
}

/// The priority-major identifier layout.
///
/// A CAN identifier *is* the bus priority, so mapping zenoh's QoS onto the top
/// bits of the identifier is what lets an urgent message win arbitration
/// against a bulk burst -- including one from the same peer, which is the case
/// zenoh-pico structurally cannot reach.
#[cfg(test)]
mod priority {
    use super::*;

    /// Zenoh's own numbering, lowest value highest priority.
    const CONTROL: u8 = 0;
    const REAL_TIME: u8 = 1;
    const DATA: u8 = 5;
    const BACKGROUND: u8 = 7;

    /// The default. The wire must be exactly what zenoh-pico speaks, so the
    /// identifier is the peer and the priority changes nothing at all.
    #[test]
    fn no_priority_bits_leaves_the_identifier_alone() {
        for prio in 0..=7u8 {
            assert_eq!(tx_id(0x101, 0, prio), 0x101);
        }
        assert_eq!(peer_mask(0), CAN_EFF_MASK);
    }

    /// Class in the top three bits, peer in the low eight.
    #[test]
    fn three_bits_put_the_class_above_the_peer() {
        assert_eq!(tx_id(0x0A, 3, CONTROL), 0x00A);
        assert_eq!(tx_id(0x0A, 3, REAL_TIME), 0x10A);
        assert_eq!(tx_id(0x0A, 3, DATA), 0x50A);
        assert_eq!(tx_id(0x0A, 3, BACKGROUND), 0x70A);
    }

    /// The property that makes this worth doing: a more urgent message from
    /// **any** peer outranks a less urgent one from **any** peer, because the
    /// class dominates from the most significant bit and a lower identifier
    /// wins arbitration.
    #[test]
    fn class_dominates_peer_in_arbitration() {
        // The lowest-numbered peer at Background still loses to the
        // highest-numbered peer at Control.
        assert!(tx_id(0xFF, 3, CONTROL) < tx_id(0x00, 3, BACKGROUND));
        // And within a class, the lower peer wins, as before.
        assert!(tx_id(0x01, 3, DATA) < tx_id(0x02, 3, DATA));
    }

    /// Zenoh numbers priorities with Control at 0 and Background at 7, and CAN
    /// gives the bus to the lowest identifier, so the two orderings already
    /// agree. Nothing is inverted anywhere, and this pins that.
    #[test]
    fn zenoh_priority_order_matches_arbitration_order() {
        let ids: Vec<u32> = (0..=7u8).map(|p| tx_id(0x10, 3, p)).collect();
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        assert_eq!(ids, sorted, "more urgent must mean numerically lower");
    }

    /// One peer transmits under eight identifiers; all eight must resolve to
    /// the same address, or the transport would see eight peers.
    #[test]
    fn every_class_resolves_to_one_peer() {
        for prio in 0..=7u8 {
            assert_eq!(tx_id(0x0A, 3, prio) & peer_mask(3), 0x0A);
        }
    }

    /// The receiver must drop its own frames whatever class they carry. Getting
    /// this wrong makes a peer talk to itself.
    #[test]
    fn own_frames_are_dropped_at_every_priority() {
        let filter = RxFilter {
            own: 0x0A,
            match_: 0,
            mask: 0,
            prio_bits: 3,
        };
        let mut out = [0u8; FD_MAX_DLEN];
        for prio in 0..=7u8 {
            let (frame, wire) = encode(tx_id(0x0A, 3, prio), &[1, 2, 3]).unwrap();
            assert_eq!(
                decode(&frame, wire, &filter, &mut out),
                Err(RxDrop::OwnFrame),
                "priority {prio}"
            );
        }
    }

    /// And a different peer is delivered, at every class, under its peer id.
    #[test]
    fn other_peers_are_delivered_under_their_peer_id() {
        let filter = RxFilter {
            own: 0x0A,
            match_: 0,
            mask: 0,
            prio_bits: 3,
        };
        let mut out = [0u8; FD_MAX_DLEN];
        for prio in 0..=7u8 {
            let (frame, wire) = encode(tx_id(0x0B, 3, prio), &[1, 2, 3]).unwrap();
            let (n, sender) = decode(&frame, wire, &filter, &mut out).unwrap();
            assert_eq!((n, sender), (3, 0x0B), "priority {prio}");
        }
    }

    /// Fewer class bits fold neighbouring priorities together but must not
    /// reorder them.
    #[test]
    fn fewer_bits_fold_without_reordering() {
        // Two bits: four classes, pairs of priorities merged.
        assert_eq!(tx_id(0x10, 2, CONTROL), tx_id(0x10, 2, REAL_TIME));
        assert_eq!(tx_id(0x10, 2, DATA), tx_id(0x10, 2, 4));
        let ids: Vec<u32> = (0..=7u8).map(|p| tx_id(0x10, 2, p)).collect();
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        assert_eq!(ids, sorted);
        // One bit: urgent half versus bulk half.
        assert_eq!(tx_id(0x10, 1, CONTROL), 0x010);
        assert_eq!(tx_id(0x10, 1, DATA), 0x410);
    }

    /// Class bits come out of the peer space, and the budget must be stated.
    #[test]
    fn class_bits_cost_peer_space() {
        assert_eq!(max_peer_id(0), 0x7FF); // 2048 peers, no classes
        assert_eq!(max_peer_id(1), 0x3FF);
        assert_eq!(max_peer_id(2), 0x1FF);
        assert_eq!(max_peer_id(3), 0x0FF); // 256 peers, 8 classes
    }

    /// A band still selects peers, not classes, so a filtered bus keeps working
    /// when priority bits are switched on.
    #[test]
    fn bands_filter_on_the_peer_field() {
        let filter = RxFilter {
            own: 0x0A,
            match_: 0x00,
            mask: 0xF0,
            prio_bits: 3,
        };
        let mut out = [0u8; FD_MAX_DLEN];
        // Peer 0x0B is inside the band 0x00/0xF0, at the noisiest class.
        let (inside, wire) = encode(tx_id(0x0B, 3, BACKGROUND), &[1]).unwrap();
        assert!(decode(&inside, wire, &filter, &mut out).is_ok());
        // Peer 0x1B is outside it, even at the most urgent class.
        let (outside, wire) = encode(tx_id(0x1B, 3, CONTROL), &[1]).unwrap();
        assert_eq!(
            decode(&outside, wire, &filter, &mut out),
            Err(RxDrop::Filtered { sender: 0x1B })
        );
    }
}
