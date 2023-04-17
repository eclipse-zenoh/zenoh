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
use crate::{RCodec, WCodec, Zenoh060, Zenoh060Condition};
use alloc::string::String;
use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
};
use zenoh_protocol::core::{WireExpr, ZInt};

impl<W> WCodec<&WireExpr<'_>, &mut W> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &WireExpr<'_>) -> Self::Output {
        self.write(&mut *writer, x.scope)?;
        if x.has_suffix() {
            self.write(&mut *writer, x.suffix.as_ref())?;
        }
        Ok(())
    }
}

impl<R> RCodec<WireExpr<'static>, &mut R> for Zenoh060Condition
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<WireExpr<'static>, Self::Error> {
        let scope: ZInt = self.codec.read(&mut *reader)?;
        let suffix: String = if self.condition {
            self.codec.read(&mut *reader)?
        } else {
            String::new()
        };
        Ok(WireExpr {
            scope,
            suffix: suffix.into(),
        })
    }
}
