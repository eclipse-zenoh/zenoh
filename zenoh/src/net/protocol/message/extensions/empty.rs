//
// Copyright (c) 2017, 2022 ADLINK Technology Inc.
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
use super::ZExtension;
use crate::net::protocol::io::{WBuf, ZBuf};

/// # Empty extension
///
/// An extension with no body.
///
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ZExtEmpty<const ID: u8>;

impl<const ID: u8> ZExtension for ZExtEmpty<{ ID }> {
    fn id(&self) -> u8 {
        ID
    }

    fn length(&self) -> usize {
        0
    }

    fn write(&self, _wbuf: &mut WBuf) -> bool {
        true
    }

    fn read(_zbuf: &mut ZBuf, _header: u8, _length: usize) -> Option<Self> {
        Some(Self)
    }
}

impl<const ID: u8> From<()> for ZExtEmpty<{ ID }> {
    fn from(_empty: ()) -> Self {
        Self
    }
}
