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
use crate::{RCodec, WCodec, Zenoh080, Zenoh080Header};
use core::time::Duration;
use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
    ZSlice,
};
use zenoh_protocol::{
    common::{imsg, ZExtUnknown},
    core::ZInt,
    transport::{
        id,
        open::{ext, flag, OpenAck, OpenSyn},
    },
};

// OpenSyn
impl<W> WCodec<&OpenSyn, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &OpenSyn) -> Self::Output {
        // Header
        let mut header = id::OPEN;
        if x.lease.as_millis() % 1_000 == 0 {
            header |= flag::T;
        }
        let has_extensions = x.ext_shm.is_some() || x.ext_auth.is_some();
        if has_extensions {
            header |= flag::Z;
        }
        self.write(&mut *writer, header)?;

        // Body
        if imsg::has_flag(header, flag::T) {
            self.write(&mut *writer, x.lease.as_secs() as ZInt)?;
        } else {
            self.write(&mut *writer, x.lease.as_millis() as ZInt)?;
        }
        self.write(&mut *writer, x.initial_sn)?;
        self.write(&mut *writer, &x.cookie)?;

        // Extensions
        if let Some(shm) = x.ext_shm.as_ref() {
            let has_more = x.ext_auth.is_some();
            self.write(&mut *writer, (shm, has_more))?;
        }

        if let Some(auth) = x.ext_auth.as_ref() {
            let has_more = false;
            self.write(&mut *writer, (auth, has_more))?;
        }

        Ok(())
    }
}

impl<R> RCodec<OpenSyn, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<OpenSyn, Self::Error> {
        let header: u8 = self.read(&mut *reader)?;
        let codec = Zenoh080Header::new(header);
        codec.read(reader)
    }
}

impl<R> RCodec<OpenSyn, &mut R> for Zenoh080Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<OpenSyn, Self::Error> {
        if imsg::mid(self.header) != id::OPEN || imsg::has_flag(self.header, flag::A) {
            return Err(DidntRead);
        }

        // Body
        let lease: ZInt = self.codec.read(&mut *reader)?;
        let lease = if imsg::has_flag(self.header, flag::T) {
            Duration::from_secs(lease)
        } else {
            Duration::from_millis(lease)
        };
        let initial_sn: ZInt = self.codec.read(&mut *reader)?;
        let cookie: ZSlice = self.codec.read(&mut *reader)?;

        // Extensions
        let mut qos = None;
        let mut shm = None;
        let mut auth = None;

        let mut has_ext = imsg::has_flag(self.header, flag::Z);
        while has_ext {
            let ext: u8 = self.codec.read(&mut *reader)?;
            let eodec = Zenoh080Header::new(ext);
            match imsg::mid(ext) {
                ext::QoS::ID => {
                    let (q, ext): (ext::QoS, bool) = eodec.read(&mut *reader)?;
                    qos = Some(q);
                    has_ext = ext;
                }
                ext::SHM => {
                    let (s, ext): (ext::Shm, bool) = eodec.read(&mut *reader)?;
                    shm = Some(s);
                    has_ext = ext;
                }
                ext::AUTH => {
                    let (a, ext): (ext::Auth, bool) = eodec.read(&mut *reader)?;
                    auth = Some(a);
                    has_ext = ext;
                }
                _ => {
                    let (_, ext): (ZExtUnknown, bool) = eodec.read(&mut *reader)?;
                    has_ext = ext;
                }
            }
        }

        Ok(OpenSyn {
            lease,
            initial_sn,
            cookie,
            ext_qos: qos,
            ext_shm: shm,
            ext_auth: auth,
        })
    }
}

// OpenAck
impl<W> WCodec<&OpenAck, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &OpenAck) -> Self::Output {
        // Header
        let mut header = id::OPEN;
        header |= flag::A;
        // Verify that the timeout is expressed in seconds, i.e. subsec part is 0.
        if x.lease.subsec_nanos() == 0 {
            header |= flag::T;
        }
        let has_extensions = x.ext_auth.is_some();
        if has_extensions {
            header |= flag::Z;
        }
        self.write(&mut *writer, header)?;

        // Body
        if imsg::has_flag(header, flag::T) {
            self.write(&mut *writer, x.lease.as_secs() as ZInt)?;
        } else {
            self.write(&mut *writer, x.lease.as_millis() as ZInt)?;
        }
        self.write(&mut *writer, x.initial_sn)?;

        // Extensions
        if let Some(auth) = x.ext_auth.as_ref() {
            let has_more = false;
            self.write(&mut *writer, (auth, has_more))?;
        }

        Ok(())
    }
}

impl<R> RCodec<OpenAck, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<OpenAck, Self::Error> {
        let header: u8 = self.read(&mut *reader)?;
        let codec = Zenoh080Header::new(header);
        codec.read(reader)
    }
}

impl<R> RCodec<OpenAck, &mut R> for Zenoh080Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<OpenAck, Self::Error> {
        if imsg::mid(self.header) != id::OPEN || !imsg::has_flag(self.header, flag::A) {
            return Err(DidntRead);
        }

        // Body
        let lease: ZInt = self.codec.read(&mut *reader)?;
        let lease = if imsg::has_flag(self.header, flag::T) {
            Duration::from_secs(lease)
        } else {
            Duration::from_millis(lease)
        };
        let initial_sn: ZInt = self.codec.read(&mut *reader)?;

        // Extensions
        let mut ext_qos = None;
        let mut ext_shm = None;
        let mut ext_auth = None;

        let mut has_ext = imsg::has_flag(self.header, flag::Z);
        while has_ext {
            let ext: u8 = self.codec.read(&mut *reader)?;
            let eodec = Zenoh080Header::new(ext);
            match imsg::mid(ext) {
                ext::QoS::ID => {
                    let (q, ext): (ext::QoS, bool) = eodec.read(&mut *reader)?;
                    ext_qos = Some(q);
                    has_ext = ext;
                }
                ext::SHM => {
                    let (s, ext): (ext::Shm, bool) = eodec.read(&mut *reader)?;
                    ext_shm = Some(s);
                    has_ext = ext;
                }
                ext::AUTH => {
                    let (a, ext): (ext::Auth, bool) = eodec.read(&mut *reader)?;
                    ext_auth = Some(a);
                    has_ext = ext;
                }
                _ => {
                    let (_, ext): (ZExtUnknown, bool) = eodec.read(&mut *reader)?;
                    has_ext = ext;
                }
            }
        }

        Ok(OpenAck {
            lease,
            initial_sn,
            ext_qos,
            ext_shm,
            ext_auth,
        })
    }
}
