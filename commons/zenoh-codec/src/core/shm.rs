//
// Copyright (c) 2022 ZettaScale Technology
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
use crate::{RCodec, WCodec, Zenoh080};
use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
};
use zenoh_shm::{
    header::descriptor::HeaderDescriptor, watchdog::descriptor::Descriptor, SharedMemoryBufInfo,
};

impl<W> WCodec<&Descriptor, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &Descriptor) -> Self::Output {
        self.write(&mut *writer, x.id)?;
        self.write(&mut *writer, x.index_and_bitpos)?;
        Ok(())
    }
}

impl<W> WCodec<&HeaderDescriptor, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &HeaderDescriptor) -> Self::Output {
        self.write(&mut *writer, x.id)?;
        self.write(&mut *writer, x.index)?;
        Ok(())
    }
}

impl<W> WCodec<&SharedMemoryBufInfo, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &SharedMemoryBufInfo) -> Self::Output {
        let SharedMemoryBufInfo {
            offset,
            length,
            shm_manager,
            kind,
            watchdog_descriptor,
            header_descriptor,
            generation,
        } = x;

        self.write(&mut *writer, offset)?;
        self.write(&mut *writer, length)?;
        self.write(&mut *writer, shm_manager.as_str())?;
        self.write(&mut *writer, kind)?;
        self.write(&mut *writer, watchdog_descriptor)?;
        self.write(&mut *writer, header_descriptor)?;
        self.write(&mut *writer, generation)?;
        Ok(())
    }
}

impl<R> RCodec<Descriptor, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Descriptor, Self::Error> {
        let id = self.read(&mut *reader)?;
        let index_and_bitpos = self.read(&mut *reader)?;

        Ok(Descriptor {
            id,
            index_and_bitpos,
        })
    }
}

impl<R> RCodec<HeaderDescriptor, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<HeaderDescriptor, Self::Error> {
        let id = self.read(&mut *reader)?;
        let index = self.read(&mut *reader)?;

        Ok(HeaderDescriptor { id, index })
    }
}

impl<R> RCodec<SharedMemoryBufInfo, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<SharedMemoryBufInfo, Self::Error> {
        let offset: usize = self.read(&mut *reader)?;
        let length: usize = self.read(&mut *reader)?;
        let shm_manager: String = self.read(&mut *reader)?;
        let kind: u8 = self.read(&mut *reader)?;
        let watchdog_descriptor: Descriptor = self.read(&mut *reader)?;
        let header_descriptor: HeaderDescriptor = self.read(&mut *reader)?;
        let generatoion: u32 = self.read(&mut *reader)?;

        let shm_info = SharedMemoryBufInfo::new(
            offset,
            length,
            shm_manager,
            kind,
            watchdog_descriptor,
            header_descriptor,
            generatoion,
        );
        Ok(shm_info)
    }
}
