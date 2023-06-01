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
use core::convert::TryFrom;
use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
};
use zenoh_protocol::core::{Timestamp, ZenohId};

impl<W> WCodec<&Timestamp, &mut W> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &Timestamp) -> Self::Output {
        self.write(&mut *writer, x.get_time().as_u64())?;
        let id = x.get_id();
        self.write(&mut *writer, &id.to_le_bytes()[..id.size()])?;
        Ok(())
    }
}

impl<R> RCodec<Timestamp, &mut R> for Zenoh060
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Timestamp, Self::Error> {
        let time: u64 = self.read(&mut *reader)?;
        let size: usize = self.read(&mut *reader)?;
        if size > (uhlc::ID::MAX_SIZE) {
            return Err(DidntRead);
        }
        let mut id = [0_u8; ZenohId::MAX_SIZE];
        reader.read_exact(&mut id[..size])?;

        let time = uhlc::NTP64(time);
        let id = uhlc::ID::try_from(&id[..size]).map_err(|_| DidntRead)?;
        Ok(Timestamp::new(time, id))
    }
}
