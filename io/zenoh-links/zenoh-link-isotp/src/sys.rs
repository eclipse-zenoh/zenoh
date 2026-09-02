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

//! The Linux `CAN_ISOTP` binding.
//!
//! The kernel is the ISO 15765-2:2016 implementation; this is only the socket
//! plumbing. `libc` provides `CAN_ISOTP` and `sockaddr_can` but none of the
//! option structs, so they are declared here against
//! `include/uapi/linux/can/isotp.h`.
//!
//! Three properties of that ABI are load-bearing and are asserted or handled
//! rather than assumed:
//!
//! * **Every socket option must be set before `bind`.** `isotp_setsockopt` opens
//!   with `if (so->bound) return -EISCONN;`, so there is no reconfiguring a live
//!   socket.
//! * **`optlen` is checked for exact equality**, not a minimum, so a struct of
//!   the wrong size is rejected at runtime. The `SIZE_*` assertions below turn
//!   that into a compile error instead.
//! * **`sockaddr_can.can_addr.tp` declares `rx_id` before `tx_id`.** We use
//!   `libc`'s struct and set the fields by name; declaring our own in the
//!   intuitive order would silently swap them, and a swapped pair produces a
//!   link that opens cleanly and never communicates.

use std::{
    ffi::CString,
    io, mem,
    os::fd::{AsRawFd, RawFd},
};

use tokio::io::unix::AsyncFd;
use zenoh_result::{bail, zerror, ZResult};

use crate::IsotpEndpoint;

/// `SOL_CAN_BASE + CAN_ISOTP`.
const SOL_CAN_ISOTP: libc::c_int = 100 + 6;

const CAN_ISOTP_OPTS: libc::c_int = 1;
/// The FlowControl this side sends: how fast, and how much at a time, we are
/// willing to be fed.
const CAN_ISOTP_RECV_FC: libc::c_int = 2;
const CAN_ISOTP_LL_OPTS: libc::c_int = 5;

/// Wait for transmit completion before `write` returns. Without it a write is
/// only queued, so the link would report success for a PDU the controller has
/// not yet placed on the bus, and backpressure would never reach zenoh.
const CAN_ISOTP_WAIT_TX_DONE: u32 = 0x0400;

/// `CAN_EFF_FLAG` -- marks an identifier as 29-bit extended.
const CAN_EFF_FLAG: u32 = 0x8000_0000;

/// `<linux/can.h>`: the frame kinds the link layer may generate.
const CAN_MTU: u8 = 16;

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct CanIsotpOptions {
    flags: u32,
    frame_txtime: u32,
    ext_address: u8,
    txpad_content: u8,
    rxpad_content: u8,
    rx_ext_address: u8,
}

/// `struct can_isotp_fc_options`. Every field is one octet of the FlowControl
/// frame this side transmits, so the peer paces its ConsecutiveFrames by what
/// is set here.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct CanIsotpFcOptions {
    /// ConsecutiveFrames the peer may send before it must wait for another
    /// FlowControl. 0 means "send everything, do not wait".
    bs: u8,
    /// Separation time the peer must leave between ConsecutiveFrames.
    /// 0x00..=0x7F is milliseconds; 0xF1..=0xF9 is 100us..900us.
    stmin: u8,
    /// How many WAIT frames we may send before giving up. 0 omits them.
    wftmax: u8,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct CanIsotpLlOptions {
    mtu: u8,
    tx_dl: u8,
    tx_flags: u8,
}

// The kernel compares `optlen` for equality, so a layout drift would surface as
// a runtime EINVAL on an otherwise correct configuration. Catch it here.
const _: () = assert!(mem::size_of::<CanIsotpOptions>() == 12);
const _: () = assert!(mem::size_of::<CanIsotpLlOptions>() == 3);
const _: () = assert!(mem::size_of::<CanIsotpFcOptions>() == 3);

struct RawIsotp {
    fd: RawFd,
}

impl AsRawFd for RawIsotp {
    fn as_raw_fd(&self) -> RawFd {
        self.fd
    }
}

impl Drop for RawIsotp {
    fn drop(&mut self) {
        // SAFETY: `fd` is owned by this value and closed exactly once.
        unsafe { libc::close(self.fd) };
    }
}

/// One ISO-TP channel: a directed identifier pair on one interface.
pub(crate) struct IsotpSocket {
    io: AsyncFd<RawIsotp>,
}

impl IsotpSocket {
    /// Open and bind a channel for one traffic class.
    ///
    /// `class` selects the identifier pair within the endpoint's block; with the
    /// default single class it is always 0.
    pub(crate) fn open(ep: &IsotpEndpoint, class: u8) -> ZResult<IsotpSocket> {
        let (tx_id, rx_id) = ep.ids_for_class(class);
        let (tx_id, rx_id) = if ep.eff {
            (tx_id | CAN_EFF_FLAG, rx_id | CAN_EFF_FLAG)
        } else {
            (tx_id, rx_id)
        };

        let fd = unsafe {
            libc::socket(
                libc::PF_CAN,
                libc::SOCK_DGRAM | libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC,
                libc::CAN_ISOTP,
            )
        };
        if fd < 0 {
            let e = io::Error::last_os_error();
            if e.raw_os_error() == Some(libc::EPROTONOSUPPORT) {
                bail!(
                    "ISO-TP: the kernel has no CAN_ISOTP support ({e}). The `can-isotp` module \
                     is mainline since Linux 5.10 and normally autoloads; load it with \
                     `sudo modprobe can-isotp`"
                );
            }
            bail!("ISO-TP: socket(PF_CAN, SOCK_DGRAM, CAN_ISOTP) failed: {e}");
        }
        let raw = RawIsotp { fd };

        let name = CString::new(ep.device.as_str())
            .map_err(|e| zerror!("ISO-TP: interface name {:?} is not usable: {e}", ep.device))?;
        // SAFETY: `name` is a valid NUL-terminated string for the call.
        let ifindex = unsafe { libc::if_nametoindex(name.as_ptr()) };
        if ifindex == 0 {
            let e = io::Error::last_os_error();
            bail!("ISO-TP: no such interface {:?}: {e}", ep.device);
        }

        // Options first: the kernel refuses them once the socket is bound.
        let opts = CanIsotpOptions {
            flags: CAN_ISOTP_WAIT_TX_DONE,
            // Left at 0, which the kernel reads as "keep the default" rather
            // than "zero"; a true zero would need CAN_ISOTP_FRAME_TXTIME_ZERO.
            frame_txtime: 0,
            ..Default::default()
        };
        setsockopt(&raw, CAN_ISOTP_OPTS, &opts, "options")?;

        // Flow control, but only when the endpoint asked for it. Left alone the
        // kernel sends BS=0 and STmin=0, which says "send the whole message as
        // fast as you like" -- right for a Linux peer and wrong for a small one
        // that cannot take a burst. These are the two knobs ISO 15765-2 gives a
        // receiver, and they are the receiver's to set, which is why they go on
        // this socket and describe what we ask of the peer.
        if ep.stmin.is_some() || ep.bs.is_some() {
            let fc = CanIsotpFcOptions {
                bs: ep.bs.unwrap_or(0),
                stmin: ep.stmin.unwrap_or(0),
                wftmax: 0,
            };
            setsockopt(&raw, CAN_ISOTP_RECV_FC, &fc, "flow control")?;
        }

        // Classic CAN framing: 8-byte link-layer frames. This is what makes the
        // link work on ordinary CAN hardware, which is most of it. The kernel
        // requires tx_dl <= 8 and tx_flags == 0 when mtu is CAN_MTU.
        let ll = CanIsotpLlOptions {
            mtu: CAN_MTU,
            tx_dl: 8,
            tx_flags: 0,
        };
        setsockopt(&raw, CAN_ISOTP_LL_OPTS, &ll, "link-layer options")?;

        let mut addr: libc::sockaddr_can = unsafe { mem::zeroed() };
        addr.can_family = libc::AF_CAN as libc::sa_family_t;
        addr.can_ifindex = ifindex as libc::c_int;
        // By name, never by position: the C declaration is `{ rx_id, tx_id }`.
        addr.can_addr.tp.tx_id = tx_id;
        addr.can_addr.tp.rx_id = rx_id;

        // SAFETY: `addr` is a fully initialised `sockaddr_can`.
        let rc = unsafe {
            libc::bind(
                raw.fd,
                &addr as *const libc::sockaddr_can as *const libc::sockaddr,
                mem::size_of::<libc::sockaddr_can>() as libc::socklen_t,
            )
        };
        if rc < 0 {
            let e = io::Error::last_os_error();
            bail!(
                "ISO-TP: binding {:?} to tx={tx_id:#x} rx={rx_id:#x} failed: {e}",
                ep.device
            );
        }

        let io = AsyncFd::new(raw).map_err(|e| {
            zerror!(
                "ISO-TP: registering {:?} with the runtime failed: {e}",
                ep.device
            )
        })?;
        Ok(IsotpSocket { io })
    }

    /// Write one PDU. The kernel segments it into first and consecutive frames
    /// and observes the peer's flow control.
    pub(crate) async fn send(&self, buf: &[u8]) -> ZResult<usize> {
        // The kernel rejects a zero-length write with EINVAL, which would read
        // as a configuration error rather than what it is.
        if buf.is_empty() {
            return Ok(0);
        }
        loop {
            let mut guard = self
                .io
                .writable()
                .await
                .map_err(|e| zerror!("ISO-TP: waiting to write failed: {e}"))?;
            let attempt = guard.try_io(|inner| {
                // SAFETY: `buf` is a valid readable region of its own length.
                let n = unsafe {
                    libc::write(
                        inner.as_raw_fd(),
                        buf.as_ptr() as *const libc::c_void,
                        buf.len(),
                    )
                };
                if n < 0 {
                    Err(io::Error::last_os_error())
                } else {
                    Ok(n as usize)
                }
            });
            match attempt {
                Err(_would_block) => continue,
                Ok(Err(e)) if e.kind() == io::ErrorKind::Interrupted => continue,
                Ok(Err(e)) => bail!("ISO-TP: write failed: {e}"),
                Ok(Ok(n)) => return Ok(n),
            }
        }
    }

    /// Read one reassembled PDU.
    pub(crate) async fn recv(&self, buf: &mut [u8]) -> ZResult<usize> {
        loop {
            let mut guard = self
                .io
                .readable()
                .await
                .map_err(|e| zerror!("ISO-TP: waiting to read failed: {e}"))?;
            let attempt = guard.try_io(|inner| {
                // SAFETY: `buf` is a valid writable region of its own length.
                let n = unsafe {
                    libc::read(
                        inner.as_raw_fd(),
                        buf.as_mut_ptr() as *mut libc::c_void,
                        buf.len(),
                    )
                };
                if n < 0 {
                    Err(io::Error::last_os_error())
                } else {
                    Ok(n as usize)
                }
            });
            match attempt {
                Err(_would_block) => continue,
                Ok(Err(e)) if e.kind() == io::ErrorKind::Interrupted => continue,
                Ok(Err(e)) => bail!("ISO-TP: read failed: {e}"),
                Ok(Ok(n)) => return Ok(n),
            }
        }
    }
}

fn setsockopt<T>(raw: &RawIsotp, opt: libc::c_int, value: &T, what: &str) -> ZResult<()> {
    // SAFETY: `value` outlives the call and its length is its own size, which
    // is what the kernel compares `optlen` against.
    let rc = unsafe {
        libc::setsockopt(
            raw.fd,
            SOL_CAN_ISOTP,
            opt,
            value as *const T as *const libc::c_void,
            mem::size_of::<T>() as libc::socklen_t,
        )
    };
    if rc < 0 {
        let e = io::Error::last_os_error();
        bail!("ISO-TP: setting {what} failed: {e}");
    }
    Ok(())
}
