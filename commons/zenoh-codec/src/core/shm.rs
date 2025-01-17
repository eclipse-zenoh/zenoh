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
use std::num::NonZeroUsize;

use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
};
use zenoh_shm::{
    api::provider::chunk::ChunkDescriptor, metadata::descriptor::MetadataDescriptor, ShmBufInfo,
};

use crate::{RCodec, WCodec, Zenoh080};

impl<W> WCodec<&MetadataDescriptor, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &MetadataDescriptor) -> Self::Output {
        self.write(&mut *writer, x.id)?;
        self.write(&mut *writer, x.index)?;
        Ok(())
    }
}

impl<W> WCodec<&ChunkDescriptor, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &ChunkDescriptor) -> Self::Output {
        self.write(&mut *writer, x.segment)?;
        self.write(&mut *writer, x.chunk)?;
        self.write(&mut *writer, x.len)?;
        Ok(())
    }
}

impl<W> WCodec<NonZeroUsize, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: NonZeroUsize) -> Self::Output {
        self.write(&mut *writer, x.get())?;
        Ok(())
    }
}

impl<W> WCodec<&ShmBufInfo, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &ShmBufInfo) -> Self::Output {
        let ShmBufInfo {
            data_len,
            metadata,
            generation,
        } = x;

        self.write(&mut *writer, *data_len)?;
        self.write(&mut *writer, metadata)?;
        self.write(&mut *writer, generation)?;
        Ok(())
    }
}

impl<R> RCodec<MetadataDescriptor, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<MetadataDescriptor, Self::Error> {
        let id = self.read(&mut *reader)?;
        let index = self.read(&mut *reader)?;

        Ok(MetadataDescriptor { id, index })
    }
}

impl<R> RCodec<ChunkDescriptor, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<ChunkDescriptor, Self::Error> {
        let segment = self.read(&mut *reader)?;
        let chunk = self.read(&mut *reader)?;
        let len = self.read(&mut *reader)?;

        Ok(ChunkDescriptor {
            segment,
            chunk,
            len,
        })
    }
}

impl<R> RCodec<NonZeroUsize, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<NonZeroUsize, Self::Error> {
        let size: usize = self.read(&mut *reader)?;
        let size = NonZeroUsize::new(size).ok_or(DidntRead)?;
        Ok(size)
    }
}

impl<R> RCodec<ShmBufInfo, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<ShmBufInfo, Self::Error> {
        let data_len = self.read(&mut *reader)?;
        let metadata = self.read(&mut *reader)?;
        let generation = self.read(&mut *reader)?;

        let shm_info = ShmBufInfo::new(data_len, metadata, generation);
        Ok(shm_info)
    }
}
