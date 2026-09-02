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

//! The Linux SocketCAN binding.
//!
//! This is also the binding that talks to a virtual `vcan0`, which is what
//! makes the link testable with no hardware:
//!
//! ```sh
//! sudo modprobe vcan
//! sudo ip link add dev vcan0 type vcan
//! sudo ip link set up vcan0
//! ```

use std::{
    ffi::CString,
    io, mem,
    os::fd::{AsRawFd, RawFd},
};

use tokio::io::unix::AsyncFd;
use zenoh_protocol::transport::BatchSize;
use zenoh_result::{bail, zerror, ZResult};

use crate::{
    frame::{self, Frame, RxDrop, RxFilter},
    CanEndpoint,
};

/// An owned CAN socket descriptor.
struct RawCan {
    fd: RawFd,
}

impl AsRawFd for RawCan {
    fn as_raw_fd(&self) -> RawFd {
        self.fd
    }
}

impl Drop for RawCan {
    fn drop(&mut self) {
        // SAFETY: `fd` is owned by this value and closed exactly once.
        unsafe { libc::close(self.fd) };
    }
}

/// Ask for a receive buffer of `requested` bytes and report what was granted.
///
/// The kernel clamps the request to `net.core.rmem_max` without telling anyone,
/// and then reports back double what it stored, so the only honest thing to do
/// is read the value back and say when it fell short.
fn set_rcvbuf(raw: &RawCan, requested: u32, device: &str) -> ZResult<()> {
    let value = requested as libc::c_int;
    // SAFETY: `value` outlives the call and its length is its own size.
    let rc = unsafe {
        libc::setsockopt(
            raw.fd,
            libc::SOL_SOCKET,
            libc::SO_RCVBUF,
            &value as *const libc::c_int as *const libc::c_void,
            mem::size_of::<libc::c_int>() as libc::socklen_t,
        )
    };
    if rc < 0 {
        let e = io::Error::last_os_error();
        bail!("CAN: setting the receive buffer on {device:?} to {requested} bytes failed: {e}");
    }

    let mut granted: libc::c_int = 0;
    let mut len = mem::size_of::<libc::c_int>() as libc::socklen_t;
    // SAFETY: `granted` and `len` are valid for the sizes given.
    let rc = unsafe {
        libc::getsockopt(
            raw.fd,
            libc::SOL_SOCKET,
            libc::SO_RCVBUF,
            &mut granted as *mut libc::c_int as *mut libc::c_void,
            &mut len,
        )
    };
    if rc < 0 {
        // Not fatal: the buffer was set, we simply cannot report on it.
        tracing::debug!("CAN: could not read back the receive buffer on {device:?}");
        return Ok(());
    }

    // The kernel reports twice what it stored, the second half being its own
    // bookkeeping allowance.
    let effective = (granted as u32) / 2;
    if effective < requested {
        tracing::warn!(
            "CAN: asked {device:?} for a {requested}-byte receive buffer but got {effective};              net.core.rmem_max is the ceiling. Raise it with              `sysctl -w net.core.rmem_max={requested}` if bursts are being dropped."
        );
    } else {
        tracing::debug!("CAN: receive buffer on {device:?} is {effective} bytes");
    }
    Ok(())
}

pub(crate) struct CanSocket {
    io: AsyncFd<RawCan>,
    /// This peer's identifier -- its address on the bus. With priority bits in
    /// play this is the peer field only, and the identifier actually
    /// transmitted also carries the traffic class.
    peer: u32,
    /// Width of the traffic-class field, 0 when priority is not mapped onto
    /// the identifier.
    prio_bits: u8,
    filter: RxFilter,
    mtu: BatchSize,
}

impl CanSocket {
    pub(crate) fn mtu(&self) -> BatchSize {
        self.mtu
    }

    pub(crate) fn open(ep: &CanEndpoint) -> ZResult<CanSocket> {
        // Bit rates are set out of band on Linux (`ip link set can0 type can
        // bitrate ...`) and a virtual interface has none at all, so `bitrate`
        // is advisory here. `dbitrate` is not: zero selects classic framing.
        let fd = unsafe {
            libc::socket(
                libc::PF_CAN,
                libc::SOCK_RAW | libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC,
                libc::CAN_RAW,
            )
        };
        if fd < 0 {
            let e = io::Error::last_os_error();
            bail!("CAN: socket(PF_CAN, SOCK_RAW, CAN_RAW) failed: {e}");
        }
        // From here on `raw` owns the descriptor, so every early return closes it.
        let raw = RawCan { fd };

        let name = CString::new(ep.device.as_str())
            .map_err(|e| zerror!("CAN: interface name {:?} is not usable: {e}", ep.device))?;
        // SAFETY: `name` is a valid NUL-terminated string for the duration of
        // the call.
        let ifindex = unsafe { libc::if_nametoindex(name.as_ptr()) };
        if ifindex == 0 {
            let e = io::Error::last_os_error();
            bail!(
                "CAN: no such interface {:?}: {e}. On Linux a virtual bus is created with \
                 `sudo ip link add dev {} type vcan && sudo ip link set up {}`",
                ep.device,
                ep.device,
                ep.device
            );
        }

        // Admit the whole band this bus segment reserves for zenoh; the read
        // then drops our own frames. A mask of 0 matches everything, which is
        // the default for a bus carrying nothing else.
        let filter = libc::can_filter {
            can_id: ep.filter_match,
            can_mask: ep.filter_mask,
        };
        // SAFETY: `filter` outlives the call and its length is its own size.
        let rc = unsafe {
            libc::setsockopt(
                raw.fd,
                libc::SOL_CAN_RAW,
                libc::CAN_RAW_FILTER,
                &filter as *const libc::can_filter as *const libc::c_void,
                mem::size_of::<libc::can_filter>() as libc::socklen_t,
            )
        };
        if rc < 0 {
            let e = io::Error::last_os_error();
            bail!(
                "CAN: setting the receive filter on {:?} failed: {e}",
                ep.device
            );
        }

        // CAN FD is required, not preferred. Classic CAN leaves a 7-byte MTU
        // against zenoh's ~16 bytes of per-fragment overhead, so a classic link
        // cannot make progress at all. Failing here beats coming up in a mode
        // whose only symptom is a session that appears to hang.
        let enable: libc::c_int = 1;
        // SAFETY: `enable` outlives the call and its length is its own size.
        let rc = unsafe {
            libc::setsockopt(
                raw.fd,
                libc::SOL_CAN_RAW,
                libc::CAN_RAW_FD_FRAMES,
                &enable as *const libc::c_int as *const libc::c_void,
                mem::size_of::<libc::c_int>() as libc::socklen_t,
            )
        };
        if rc < 0 {
            let e = io::Error::last_os_error();
            bail!(
                "CAN: interface {:?} does not support CAN FD ({e}). This link is CAN FD only: \
                 classic CAN leaves a 7-byte MTU, which is smaller than zenoh's per-fragment \
                 overhead, so no session could make progress. Enable FD on the interface, e.g. \
                 `ip link set {} type can bitrate 500000 dbitrate 2000000 fd on`",
                ep.device,
                ep.device
            );
        }

        // Frames that arrive faster than the link drains them are dropped by the
        // kernel, silently, before the link ever sees them. A real bus cannot
        // outrun the reader -- 2 Mbit/s of CAN FD is under 2 800 frames per
        // second -- but a virtual interface has no bit rate at all, and a burst
        // over `vcan` will overrun the default buffer. Measured: a 4 KiB
        // payload, which is 71 frames, lost 31% of messages on the default
        // buffer and none of them on 8 MiB.
        if let Some(requested) = ep.so_rcvbuf {
            set_rcvbuf(&raw, requested, &ep.device)?;
        }

        let mut addr: libc::sockaddr_can = unsafe { mem::zeroed() };
        addr.can_family = libc::AF_CAN as libc::sa_family_t;
        addr.can_ifindex = ifindex as libc::c_int;
        // SAFETY: `addr` is a fully initialised `sockaddr_can` and the length
        // passed is its own size.
        let rc = unsafe {
            libc::bind(
                raw.fd,
                &addr as *const libc::sockaddr_can as *const libc::sockaddr,
                mem::size_of::<libc::sockaddr_can>() as libc::socklen_t,
            )
        };
        if rc < 0 {
            let e = io::Error::last_os_error();
            bail!("CAN: binding to {:?} failed: {e}", ep.device);
        }

        let mtu = frame::FD_MTU;

        let io = AsyncFd::new(raw).map_err(|e| {
            zerror!(
                "CAN: registering {:?} with the runtime failed: {e}",
                ep.device
            )
        })?;

        Ok(CanSocket {
            io,
            peer: ep.id,
            prio_bits: ep.prio_bits,
            filter: RxFilter {
                own: ep.id,
                match_: ep.filter_match,
                mask: ep.filter_mask,
                prio_bits: ep.prio_bits,
            },
            mtu,
        })
    }

    /// Write one datagram, which must fit one frame.
    ///
    /// `priority` is zenoh's own numbering, `Control` at 0 through `Background`
    /// at 7. It selects the traffic class in the identifier when the endpoint
    /// asked for priority bits, and is ignored otherwise.
    ///
    /// Returns the number of datagram bytes accepted. There is no partial write
    /// to loop over: a datagram link writes one frame or it does not.
    pub(crate) async fn send(&self, datagram: &[u8], priority: u8) -> ZResult<usize> {
        let id = frame::tx_id(self.peer, self.prio_bits, priority);
        let (f, wire) = frame::encode(id, datagram).map_err(|e| zerror!("CAN: {e}"))?;
        let bytes = f.as_wire_bytes(wire);

        loop {
            let mut guard = self
                .io
                .writable()
                .await
                .map_err(|e| zerror!("CAN: waiting to write failed: {e}"))?;

            let attempt = guard.try_io(|inner| {
                // SAFETY: `bytes` is a valid readable region of `wire` bytes.
                let n = unsafe {
                    libc::write(
                        inner.as_raw_fd(),
                        bytes.as_ptr() as *const libc::c_void,
                        wire,
                    )
                };
                if n < 0 {
                    Err(io::Error::last_os_error())
                } else {
                    Ok(n as usize)
                }
            });

            match attempt {
                // The readiness was stale; wait for it again.
                Err(_would_block) => continue,
                Ok(Err(e)) if e.kind() == io::ErrorKind::Interrupted => continue,
                Ok(Err(e)) if e.raw_os_error() == Some(libc::ENOBUFS) => {
                    // The controller's transmit queue is full, which on a
                    // shared bus means arbitration is being lost to higher
                    // priority traffic. Dropping the datagram is correct for a
                    // best-effort link; returning an error here would fail the
                    // transport's TX task and tear down the whole session over
                    // a transient condition.
                    tracing::debug!(
                        "CAN: transmit queue full, dropping a {}-byte datagram",
                        datagram.len()
                    );
                    return Ok(datagram.len());
                }
                Ok(Err(e)) => bail!("CAN: write failed: {e}"),
                Ok(Ok(n)) if n == wire => return Ok(datagram.len()),
                Ok(Ok(n)) => bail!("CAN: short write of {n} bytes, expected {wire}"),
            }
        }
    }

    /// Read one datagram and report the identifier of the peer that sent it.
    ///
    /// Frames this peer sent itself, and frames outside the configured band,
    /// are skipped rather than returned.
    pub(crate) async fn recv(&self, out: &mut [u8]) -> ZResult<(usize, u32)> {
        loop {
            let mut guard = self
                .io
                .readable()
                .await
                .map_err(|e| zerror!("CAN: waiting to read failed: {e}"))?;

            let mut f = Frame::zeroed();
            let attempt = guard.try_io(|inner| {
                // SAFETY: `f` is a valid writable region of `size_of::<Frame>()`
                // bytes, which is the largest frame the kernel can return.
                let n = unsafe {
                    libc::read(
                        inner.as_raw_fd(),
                        &mut f as *mut Frame as *mut libc::c_void,
                        mem::size_of::<Frame>(),
                    )
                };
                if n < 0 {
                    Err(io::Error::last_os_error())
                } else {
                    Ok(n as usize)
                }
            });

            let nread = match attempt {
                Err(_would_block) => continue,
                Ok(Err(e)) if e.kind() == io::ErrorKind::Interrupted => continue,
                Ok(Err(e)) => bail!("CAN: read failed: {e}"),
                Ok(Ok(n)) => n,
            };

            // A raw CAN socket has no end of stream, so a zero-length read is
            // the descriptor going away. Treating it as a malformed frame would
            // spin: the fd stays readable and the loop would never block.
            if nread == 0 {
                bail!("CAN: the socket reported end of file");
            }

            match frame::decode(&f, nread, &self.filter, out) {
                Ok(delivered) => return Ok(delivered),
                // Expected on any shared bus, and on a loopback-enabled
                // interface every transmission comes back to us.
                Err(RxDrop::OwnFrame) | Err(RxDrop::Filtered { .. }) => continue,
                // A malformed frame is worth a line, but not worth failing the
                // link over: the bus is shared and anyone may put anything on it.
                Err(drop) => {
                    tracing::debug!("CAN: dropping a frame from {:#x}: {drop}", f.can_id);
                    continue;
                }
            }
        }
    }
}
