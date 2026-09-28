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
    api::provider::chunk::ChunkDescriptor,
    metadata::descriptor::MetadataDescriptor,
    posix_shm::{segment::Segment, struct_in_shm::StructInSHM},
    shm::SegmentID,
    ShmBufInfo,
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

impl<'a, W, ID> WCodec<&'a Segment<ID>, &'a mut W> for Zenoh080
where
    W: Writer,
    ID: SegmentID,
    rand::distributions::Standard: rand::distributions::Distribution<ID>,
    Zenoh080: WCodec<ID, &'a mut W>,
{
    type Output = <Zenoh080 as WCodec<ID, &'a mut W>>::Output;

    fn write(self, writer: &'a mut W, x: &Segment<ID>) -> Self::Output {
        self.write(&mut *writer, x.id())
    }
}

impl<'a, R, ID> RCodec<Segment<ID>, &'a mut R> for Zenoh080
where
    R: Reader,
    ID: SegmentID,
    rand::distributions::Standard: rand::distributions::Distribution<ID>,
    Zenoh080: RCodec<ID, &'a mut R>,
{
    type Error = DidntRead;

    fn read(self, reader: &'a mut R) -> Result<Segment<ID>, Self::Error> {
        let id = self.read(&mut *reader).map_err(|_| DidntRead)?;
        Segment::open(id).map_err(|_| DidntRead)
    }
}

impl<'a, W, ID, Elem> WCodec<&'a StructInSHM<ID, Elem>, &'a mut W> for Zenoh080
where
    W: Writer,
    ID: SegmentID,
    rand::distributions::Standard: rand::distributions::Distribution<ID>,
    Zenoh080: WCodec<ID, &'a mut W>,
{
    type Output = <Zenoh080 as WCodec<ID, &'a mut W>>::Output;

    fn write(self, writer: &'a mut W, x: &StructInSHM<ID, Elem>) -> Self::Output {
        self.write(&mut *writer, x.id())
    }
}

impl<'a, R, ID, Elem> RCodec<StructInSHM<ID, Elem>, &'a mut R> for Zenoh080
where
    R: Reader,
    ID: SegmentID,
    rand::distributions::Standard: rand::distributions::Distribution<ID>,
    Zenoh080: RCodec<ID, &'a mut R>,
{
    type Error = DidntRead;

    fn read(self, reader: &'a mut R) -> Result<StructInSHM<ID, Elem>, Self::Error> {
        let id = self.read(&mut *reader).map_err(|_| DidntRead)?;
        StructInSHM::open(id).map_err(|_| DidntRead)
    }
}
