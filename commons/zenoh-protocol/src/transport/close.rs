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
    pub const S: u8 = 1 << 5; // 0x20 Session close if S==1 close the whole session, close only the link otherwise
                              // pub const X: u8 = 1 << 6; // 0x40       Reserved
    pub const Z: u8 = 1 << 7; // 0x80 Extensions    if Z==1 then an extension will follow
}

// Reason for the Close message
pub mod reason {
    pub const GENERIC: u8 = 0x00;
    pub const UNSUPPORTED: u8 = 0x01;
    pub const INVALID: u8 = 0x02;
    pub const MAX_SESSIONS: u8 = 0x03;
    pub const MAX_LINKS: u8 = 0x04;
    pub const EXPIRED: u8 = 0x05;
    pub const UNRESPONSIVE: u8 = 0x06;
    pub const CONNECTION_TO_SELF: u8 = 0x07;
}

pub fn reason_to_str(reason: u8) -> &'static str {
    match reason {
        reason::GENERIC => "GENERIC",
        reason::UNSUPPORTED => "UNSUPPORTED",
        reason::INVALID => "INVALID",
        reason::MAX_SESSIONS => "MAX_SESSIONS",
        reason::MAX_LINKS => "MAX_LINKS",
        reason::EXPIRED => "EXPIRED",
        reason::UNRESPONSIVE => "UNRESPONSIVE",
        reason::CONNECTION_TO_SELF => "CONNECTION_TO_SELF",
        _ => "UNKNOWN",
    }
}

/// # Close message
///
/// The [`Close`] message is sent in any of the following two cases:
///     1) in response to an INIT or OPEN message which are not accepted;
///     2) at any time to arbitrarily close the transport with the corresponding zenoh node.
///
/// The [`Close`] message flow is the following:
///
/// ```text
///     A                   B
///     |       CLOSE       |
///     |------------------>|
///     |                   |
/// ```
///
/// The [`Close`] message structure is defined as follows:
///
/// ```text
/// Flags:
/// - S: Session close  If S==1 close the whole session, close only the link otherwise
/// - X: Reserved
/// - Z: Extensions     If Z==1 then zenoh extensions will follow.
///
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |Z|X|S|  CLOSE  |
/// +-+-+-+---------+
/// |     reason    |
/// +---------------+
/// ~  [CloseExts]  ~ if Flag(Z)==1
/// +---------------+
/// ```
/// NOTE: 16 bits (2 bytes) may be prepended to the serialized message indicating the total length
///       in bytes of the message, resulting in the maximum length of a message being 65535 bytes.
///       This is necessary in those stream-oriented transports (e.g., TCP) that do not preserve
///       the boundary of the serialized messages. The length is encoded as little-endian.
///       In any case, the length of a message must not exceed 65535 bytes.
///
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Close {
    pub reason: u8,
    pub session: bool,
}

impl Close {
    #[cfg(feature = "test")]
    #[doc(hidden)]
    pub fn rand() -> Self {
        use rand::Rng;

        let mut rng = rand::thread_rng();

        let reason: u8 = rng.gen();
        let session = rng.gen_bool(0.5);

        Self { reason, session }
    }
}
