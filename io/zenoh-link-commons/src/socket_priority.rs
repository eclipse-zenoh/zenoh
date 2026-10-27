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
use std::str::FromStr;

use zenoh_protocol::core::{Config, PriorityRange};
use zenoh_result::{bail, zerror, Error as ZError, ZResult};

use crate::SO_PRIORITY;

/// The `so_priority` endpoint option: the Linux socket priority (`SO_PRIORITY`) of a link,
/// chosen by the priority range the link carries.
///
/// The value is a `|`-separated list. An entry `<range>:<priority>`, e.g. `0-5:6`, applies to the
/// links whose priority range is within `<range>`. A bare `<priority>` applies to all other links,
/// including links without a priority range. The first matching entry wins.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SocketPriority {
    ranges: Vec<(PriorityRange, u32)>,
    default: Option<u32>,
}

impl SocketPriority {
    /// Returns the socket priority of a link that carries `priorities`, if the option sets one.
    pub fn get(&self, priorities: Option<&PriorityRange>) -> Option<u32> {
        priorities
            .and_then(|p| self.ranges.iter().find(|(range, _)| range.includes(p)))
            .map(|(_, value)| *value)
            .or(self.default)
    }

    fn values(&self) -> impl Iterator<Item = u32> + '_ {
        self.ranges.iter().map(|(_, v)| *v).chain(self.default)
    }
}

impl FromStr for SocketPriority {
    type Err = ZError;

    fn from_str(s: &str) -> ZResult<Self> {
        let mut out = SocketPriority {
            ranges: vec![],
            default: None,
        };
        for entry in s.split('|') {
            let invalid = || zerror!("Unknown {SO_PRIORITY} argument: {s}");
            match entry.split_once(':') {
                Some((range, value)) => {
                    let range: PriorityRange = range.parse().map_err(|_| invalid())?;
                    if range.start() > range.end() {
                        return Err(invalid().into());
                    }
                    out.ranges
                        .push((range, value.parse().map_err(|_| invalid())?));
                }
                None if out.default.is_none() => {
                    out.default = Some(entry.parse().map_err(|_| invalid())?)
                }
                None => bail!("{SO_PRIORITY} has more than one value without a range: {s}"),
            }
        }
        Ok(out)
    }
}

/// Parses the `so_priority` endpoint option.
///
/// On Linux, fails if this process may not set one of its values, e.g. a value above 6 without
/// `CAP_NET_ADMIN`. On other platforms, the option is ignored with a warning.
pub fn parse_socket_priority(config: &Config) -> ZResult<Option<SocketPriority>> {
    let Some(value) = config.get(SO_PRIORITY) else {
        return Ok(None);
    };
    let socket_priority: SocketPriority = value.parse()?;
    if !cfg!(target_os = "linux") {
        tracing::warn!(
            "{SO_PRIORITY} is only supported on Linux, ignoring it on {}",
            std::env::consts::OS
        );
        return Ok(None);
    }
    let probe = socket2::Socket::new(socket2::Domain::IPV4, socket2::Type::DGRAM, None)?;
    for priority in socket_priority.values() {
        set_socket_priority(&probe, priority)
            .map_err(|e| zerror!("{SO_PRIORITY}: can not set socket priority {priority}: {e}"))?;
    }
    Ok(Some(socket_priority))
}

/// Sets the Linux socket priority (`SO_PRIORITY`) of a socket.
#[cfg(target_os = "linux")]
pub fn set_socket_priority<'a>(
    socket: impl Into<socket2::SockRef<'a>>,
    priority: u32,
) -> std::io::Result<()> {
    use std::os::fd::AsRawFd;

    let socket = socket.into();
    let value = libc::c_int::try_from(priority)
        .map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidInput))?;
    // SAFETY: `socket` borrows the fd for the whole call, and `value` outlives it.
    let ret = unsafe {
        libc::setsockopt(
            socket.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PRIORITY,
            &value as *const libc::c_int as *const libc::c_void,
            std::mem::size_of::<libc::c_int>() as libc::socklen_t,
        )
    };
    if ret == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

/// Sets the Linux socket priority (`SO_PRIORITY`) of a socket.
#[cfg(not(target_os = "linux"))]
pub fn set_socket_priority<'a>(
    _socket: impl Into<socket2::SockRef<'a>>,
    _priority: u32,
) -> std::io::Result<()> {
    Err(std::io::Error::from(std::io::ErrorKind::Unsupported))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn range(s: &str) -> PriorityRange {
        s.parse().unwrap()
    }

    #[test]
    fn so_priority_parse() {
        let sp: SocketPriority = "0-5:6|6-7:2|1".parse().unwrap();
        assert_eq!(sp.get(Some(&range("0-5"))), Some(6));
        assert_eq!(sp.get(Some(&range("1-1"))), Some(6));
        assert_eq!(sp.get(Some(&range("6-7"))), Some(2));
        assert_eq!(sp.get(Some(&range("4-7"))), Some(1));
        assert_eq!(sp.get(None), Some(1));

        let sp: SocketPriority = "6".parse().unwrap();
        assert_eq!(sp.get(Some(&range("0-5"))), Some(6));
        assert_eq!(sp.get(None), Some(6));

        let sp: SocketPriority = "0-5:6".parse().unwrap();
        assert_eq!(sp.get(Some(&range("6-7"))), None);
        assert_eq!(sp.get(None), None);

        let sp: SocketPriority = "0-7:3|0-5:6".parse().unwrap();
        assert_eq!(sp.get(Some(&range("0-5"))), Some(3));

        for invalid in [
            "", "x", "0-5", "0-5:", "0-8:6", "5-0:6", "0-5:-1", "1|2", "0-5:6;1",
        ] {
            assert!(
                invalid.parse::<SocketPriority>().is_err(),
                "{invalid} should not parse"
            );
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn so_priority_set() {
        use std::os::fd::AsRawFd;

        let socket = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        set_socket_priority(&socket, 5).unwrap();
        let mut value: libc::c_int = 0;
        let mut len = std::mem::size_of::<libc::c_int>() as libc::socklen_t;
        // SAFETY: `value` and `len` outlive the call and match SO_PRIORITY.
        let ret = unsafe {
            libc::getsockopt(
                socket.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_PRIORITY,
                &mut value as *mut libc::c_int as *mut libc::c_void,
                &mut len,
            )
        };
        assert_eq!(ret, 0);
        assert_eq!(value, 5);
    }
}
