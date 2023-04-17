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
use alloc::vec::Vec;
use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
};
use zenoh_protocol::core::{Property, ZInt};

impl<W> WCodec<&Property, &mut W> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &Property) -> Self::Output {
        self.write(&mut *writer, x.key)?;
        self.write(&mut *writer, x.value.as_slice())?;
        Ok(())
    }
}

impl<R> RCodec<Property, &mut R> for Zenoh060
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Property, Self::Error> {
        let key: ZInt = self.read(&mut *reader)?;
        let value: Vec<u8> = self.read(&mut *reader)?;

        Ok(Property { key, value })
    }
}

impl<W> WCodec<&[Property], &mut W> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &[Property]) -> Self::Output {
        self.write(&mut *writer, x.len())?;
        for p in x.iter() {
            self.write(&mut *writer, p)?;
        }

        Ok(())
    }
}

impl<R> RCodec<Vec<Property>, &mut R> for Zenoh060
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Vec<Property>, Self::Error> {
        let num: usize = self.read(&mut *reader)?;

        let mut ps = Vec::with_capacity(num);
        for _ in 0..num {
            let p: Property = self.read(&mut *reader)?;
            ps.push(p);
        }

        Ok(ps)
    }
}
