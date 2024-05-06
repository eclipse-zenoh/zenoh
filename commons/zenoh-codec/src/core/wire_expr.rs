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
use alloc::string::String;

use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
};
use zenoh_protocol::{
    core::{ExprId, ExprLen, WireExpr},
    network::Mapping,
};

use crate::{core::Zenoh080Bounded, RCodec, WCodec, Zenoh080, Zenoh080Condition};

impl<W> WCodec<&WireExpr<'_>, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &WireExpr<'_>) -> Self::Output {
        let WireExpr {
            scope,
            suffix,
            mapping: _,
        } = x;

        let zodec = Zenoh080Bounded::<ExprId>::new();
        zodec.write(&mut *writer, *scope)?;

        if x.has_suffix() {
            let zodec = Zenoh080Bounded::<ExprLen>::new();
            zodec.write(&mut *writer, suffix.as_ref())?;
        }
        Ok(())
    }
}

impl<R> RCodec<WireExpr<'static>, &mut R> for Zenoh080Condition
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<WireExpr<'static>, Self::Error> {
        let zodec = Zenoh080Bounded::<ExprId>::new();
        let scope: ExprId = zodec.read(&mut *reader)?;

        let suffix: String = if self.condition {
            let zodec = Zenoh080Bounded::<ExprLen>::new();
            zodec.read(&mut *reader)?
        } else {
            String::new()
        };
        Ok(WireExpr {
            scope,
            suffix: suffix.into(),
            mapping: Mapping::DEFAULT,
        })
    }
}
