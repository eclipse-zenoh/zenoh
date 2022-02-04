//
// Copyright (c) 2017, 2020 ADLINK Technology Inc.
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ADLINK zenoh team, <zenoh@adlink-labs.tech>
//
mod byte;
mod empty;
mod properties;
mod unknown;
mod zint;

use crate::net::protocol::io::{WBuf, ZBuf};
pub use byte::*;
pub use empty::*;
pub use properties::*;
use std::convert::Into;
use std::fmt::Debug;
use std::ops::{Deref, DerefMut};
pub use unknown::*;
pub use zint::*;

/// # Zenoh extensions
///
/// A zenoh extension is encoded as TLV (Type, Length, Value).
/// Zenoh extensions with unknown IDs (i.e., type) can be skipped by reading the length and
/// not decoding the body (i.e. value). In case the zenoh extension is unknown, it is
/// still possible to forward it to the next hops, which in turn may be able to understand it.
/// This results in the capability of introducing new extensions in an already running system
/// without requiring the redeployment of the totatly of infrastructure nodes.
///
/// The zenoh extension wire format is the following:
///
/// ```text
/// Header flags:
/// - X: Reserved
/// - F: Forward        If F==1 then the extension needs to be forwarded. (*)
/// - Z: More           If Z==1 then another extension will follow.
///
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |Z|F|X|   ID    |
/// +-+-+-+---------+
/// %    length     %
/// +---------------+
/// ~     [u8]      ~
/// +---------------+
/// ```
///
/// (*) If the zenoh extension is not understood, then it SHOULD NOT be dropped and it
///     SHOULD be forwarded to the next hops.
///

// Extension mask
pub const EXT_ID_BITS: u8 = 5;
pub const EXT_ID_MASK: u8 = !(u8::MAX << EXT_ID_BITS);
pub const EXT_FLAGS_MASK: u8 = !EXT_ID_MASK;

// pub const FLAG_X: u8 = 1 << 5;
pub const FLAG_F: u8 = 1 << 6;
pub const FLAG_Z: u8 = 1 << 7;

pub const fn eid(header: u8) -> u8 {
    header & EXT_ID_MASK
}

pub const fn is_forward(header: u8) -> bool {
    super::has_flag(header, FLAG_F)
}

pub const fn has_more(header: u8) -> bool {
    super::has_flag(header, FLAG_Z)
}

#[repr(u8)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ZExtPolicy {
    Ignore,
    Forward,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ZExt<T>
where
    T: ZExtension,
{
    inner: T,
    policy: ZExtPolicy,
}

impl<T: ZExtension> ZExt<T> {
    pub fn new<G: Into<T>>(inner: G, policy: ZExtPolicy) -> Self {
        Self {
            inner: inner.into(),
            policy,
        }
    }

    pub fn into_inner(self) -> T {
        self.inner
    }

    pub fn as_inner(&self) -> &T {
        &self.inner
    }

    pub fn as_inner_mut(&mut self) -> &mut T {
        &mut self.inner
    }

    pub fn write(&self, wbuf: &mut WBuf, more: bool) -> bool {
        let mut header = self.inner.id();
        if let ZExtPolicy::Forward = self.policy {
            header |= FLAG_F;
        }
        if more {
            header |= FLAG_Z;
        }
        wbuf.write(header)
            && wbuf.write_usize_as_zint(self.inner.length())
            && self.inner.write(wbuf)
    }

    pub fn read(zbuf: &mut ZBuf, header: u8) -> Option<Self> {
        let length = zbuf.read_zint_as_usize()?;
        let pos = zbuf.get_pos();
        match T::read(zbuf, header, length) {
            Some(inner) => {
                let policy = if is_forward(header) {
                    ZExtPolicy::Forward
                } else {
                    ZExtPolicy::Ignore
                };
                Some(ZExt { inner, policy })
            }
            None => {
                zbuf.set_pos(pos);
                zbuf.skip_bytes(length);
                None
            }
        }
    }
}

impl<T: ZExtension> Deref for ZExt<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T: ZExtension> DerefMut for ZExt<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

pub trait ZExtension: Clone + Debug + PartialEq {
    fn id(&self) -> u8;
    fn length(&self) -> usize;
    fn write(&self, wbuf: &mut WBuf) -> bool;
    fn read(zbuf: &mut ZBuf, header: u8, length: usize) -> Option<Self>
    where
        Self: Sized;
}
