//
// Copyright (c) 2025 ZettaScale Technology
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
use std::{net::SocketAddr, num::ParseIntError};

use tracing::warn;
use zenoh_protocol::core::Config;
use zenoh_result::{zerror, ZResult};

use crate::DSCP;

fn parse_int(s: &str) -> Result<u32, ParseIntError> {
    if let Some(s) = ["0x", "0X"].iter().find_map(|pfx| s.strip_prefix(pfx)) {
        u32::from_str_radix(s, 16)
    } else if let Some(s) = ["0xb", "0B"].iter().find_map(|pfx| s.strip_prefix(pfx)) {
        u32::from_str_radix(s, 2)
    } else {
        s.parse::<u32>()
    }
}

/// Parse DSCP config.
///
/// It supports hexa/binary prefixes, as well as `|` operator, e.g. `dscp=0x04|0x10`.
pub fn parse_dscp(config: &Config) -> ZResult<Option<u32>> {
    let Some(dscp) = config.get(DSCP) else {
        return Ok(None);
    };
    Ok(Some(
        dscp.split('|')
            .map(parse_int)
            .map(Result::ok)
            .reduce(|a, b| Some(a? | b?))
            .flatten()
            .ok_or_else(|| zerror!("Unknown DSCP argument: {dscp}"))?,
    ))
}

/// Set DSCP option to the socket if supported by the target.
///
/// If the target doesn't support it, a warning is emitted.
/// IPv4 uses IP_TOS, while IPv6 uses IPV6_TCLASS
pub fn set_dscp<'a>(
    socket: impl Into<socket2::SockRef<'a>>,
    addr: SocketAddr,
    dscp: u32,
) -> std::io::Result<()> {
    match addr {
        #[cfg(not(any(
            target_os = "fuchsia",
            target_os = "redox",
            target_os = "solaris",
            target_os = "illumos",
        )))]
        SocketAddr::V4(_) => socket.into().set_tos(dscp)?,
        #[cfg(any(
            target_os = "android",
            target_os = "dragonfly",
            target_os = "freebsd",
            target_os = "fuchsia",
            target_os = "linux",
            target_os = "macos",
            target_os = "netbsd",
            target_os = "openbsd"
        ))]
        SocketAddr::V6(_) => socket.into().set_tclass_v6(dscp)?,
        #[allow(unreachable_patterns)]
        SocketAddr::V4(_) => warn!(
            "IPv4 DSCP is unsupported on platform {}",
            std::env::consts::OS
        ),
        #[allow(unreachable_patterns)]
        SocketAddr::V6(_) => warn!(
            "IPv6 DSCP is unsupported on platform {}",
            std::env::consts::OS
        ),
    }
    Ok(())
}
