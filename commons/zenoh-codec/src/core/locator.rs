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
use crate::{RCodec, WCodec, Zenoh060};
use alloc::{string::String, vec::Vec};
use core::convert::TryFrom;
use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
};
use zenoh_protocol::core::Locator;

impl<W> WCodec<&Locator, &mut W> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &Locator) -> Self::Output {
        self.write(writer, x.as_str())
    }
}

impl<R> RCodec<Locator, &mut R> for Zenoh060
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Locator, Self::Error> {
        let loc: String = self.read(reader)?;
        Locator::try_from(loc).map_err(|_| DidntRead)
    }
}

impl<W> WCodec<&[Locator], &mut W> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &[Locator]) -> Self::Output {
        self.write(&mut *writer, x.len())?;
        for l in x {
            self.write(&mut *writer, l)?;
        }
        Ok(())
    }
}

impl<R> RCodec<Vec<Locator>, &mut R> for Zenoh060
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Vec<Locator>, Self::Error> {
        let len = self.read(&mut *reader)?;
        let mut vec: Vec<Locator> = Vec::with_capacity(len);
        for _ in 0..len {
            vec.push(self.read(&mut *reader)?);
        }
        Ok(vec)
    }
}
