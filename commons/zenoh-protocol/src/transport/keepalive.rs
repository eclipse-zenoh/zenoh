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

pub mod flag {
    // pub const X: u8 = 1 << 5; // 0x20       Reserved
    // pub const X: u8 = 1 << 6; // 0x40       Reserved
    pub const Z: u8 = 1 << 7; // 0x80 Extensions    if Z==1 then an extension will follow
}

/// # KeepAlive message
///
/// The [`KeepAlive`] message SHOULD be sent periodically to avoid the expiration of the
/// link lease period. A [`KeepAlive`] message MAY NOT be sent on the link if some other
/// data has been transmitted on the same link during the last keep alive interval.
///
/// The [`KeepAlive`] message flow is the following:
///
/// ```text
/// A                   B
/// |    KEEP ALIVE     |
/// |------------------>|
/// |                   |
/// ~        ...        ~
/// |                   |
/// |    KEEP ALIVE     |
/// |<------------------|
/// |                   |
/// |    KEEP ALIVE     |
/// |------------------>|
/// |                   |
/// ~        ...        ~
/// |                   |
/// |    KEEP ALIVE     |
/// |<------------------|
/// |                   |
/// ~        ...        ~
/// |                   |
/// |    KEEP ALIVE     |
/// |------------------>|
/// |                   |
/// ~        ...        ~
/// |                   |
/// ```
///
/// NOTE: In order to consider eventual packet loss, transmission latency and jitter, the time
///       interval between two subsequent [`KeepAlive`] messages SHOULD be set to one fourth of
///       the lease time. This is in-line with the ITU-T G.8013/Y.1731 specification on continuous
///       connectivity check which considers a link as failed when no messages are received in
///       3.5 times the target keep alive interval.
///
/// The [`KeepAlive`] message structure is defined as follows:
///
/// ```text
/// Flags:
/// - X: Reserved
/// - X: Reserved
/// - Z: Extensions     If Z==1 then zenoh extensions will follow.
///
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |Z|X|X| KALIVE  |
/// +-+-+-+---------+
/// ~  [KAliveExts] ~ if Flag(Z)==1
/// +---------------+
/// ```
///
/// NOTE: 16 bits (2 bytes) may be prepended to the serialized message indicating the total length
///       in bytes of the message, resulting in the maximum length of a message being 65535 bytes.
///       This is necessary in those stream-oriented transports (e.g., TCP) that do not preserve
///       the boundary of the serialized messages. The length is encoded as little-endian.
///       In any case, the length of a message must not exceed 65535 bytes.
///
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeepAlive;

impl KeepAlive {
    #[cfg(feature = "test")]
    #[doc(hidden)]
    pub fn rand() -> Self {
        Self
    }
}
