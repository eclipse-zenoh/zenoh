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
use std::os::fd::{AsFd, AsRawFd};

/// Sets the Linux socket priority (`SO_PRIORITY`) of a socket.
///
/// Values above 6 need the `CAP_NET_ADMIN` capability.
pub fn set_socket_priority(socket: &impl AsFd, priority: u32) -> std::io::Result<()> {
    let value = libc::c_int::try_from(priority)
        .map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidInput))?;
    // SAFETY: the fd is borrowed for the whole call, and `value` outlives it.
    let ret = unsafe {
        libc::setsockopt(
            socket.as_fd().as_raw_fd(),
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

/// Checks that this process is allowed to set the socket priority `priority`.
pub fn check_socket_priority(priority: u32) -> std::io::Result<()> {
    let socket = socket2::Socket::new(socket2::Domain::IPV4, socket2::Type::DGRAM, None)?;
    set_socket_priority(&socket, priority)
}
