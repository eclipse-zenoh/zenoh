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
use std::convert::TryFrom;

use zenoh_buffers::{
    reader::{DidntRead, HasReader, Reader},
    writer::{DidntWrite, HasWriter, Writer},
};
use zenoh_codec::{RCodec, WCodec, Zenoh080};
use zenoh_crypto::{BlockCipher, PseudoRng};
use zenoh_protocol::{
    core::{Resolution, WhatAmI, ZenohIdProto},
    transport::BatchSize,
};

use crate::unicast::establishment::ext;

#[derive(Debug, PartialEq)]
pub(crate) struct Cookie {
    pub(crate) zid: ZenohIdProto,
    pub(crate) whatami: WhatAmI,
    pub(crate) resolution: Resolution,
    pub(crate) batch_size: BatchSize,
    pub(crate) nonce: u64,
    // Extensions
    pub(crate) ext_qos: ext::qos::StateAccept,
    #[cfg(feature = "transport_multilink")]
    pub(crate) ext_mlink: ext::multilink::StateAccept,
    #[cfg(feature = "shared-memory")]
    pub(crate) ext_shm: ext::shm::StateAccept,
    #[cfg(feature = "transport_auth")]
    pub(crate) ext_auth: ext::auth::StateAccept,
    pub(crate) ext_lowlatency: ext::lowlatency::StateAccept,
    #[cfg(feature = "transport_compression")]
    pub(crate) ext_compression: ext::compression::StateAccept,
    pub(crate) ext_patch: ext::patch::StateAccept,
}

impl<W> WCodec<&Cookie, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &Cookie) -> Self::Output {
        self.write(&mut *writer, &x.zid)?;
        let wai: u8 = x.whatami.into();
        self.write(&mut *writer, wai)?;
        self.write(&mut *writer, x.resolution.as_u8())?;
        self.write(&mut *writer, x.batch_size)?;
        self.write(&mut *writer, x.nonce)?;
        // Extensions
        self.write(&mut *writer, &x.ext_qos)?;
        #[cfg(feature = "transport_multilink")]
        self.write(&mut *writer, &x.ext_mlink)?;
        #[cfg(feature = "shared-memory")]
        self.write(&mut *writer, &x.ext_shm)?;
        #[cfg(feature = "transport_auth")]
        self.write(&mut *writer, &x.ext_auth)?;
        self.write(&mut *writer, &x.ext_lowlatency)?;
        #[cfg(feature = "transport_compression")]
        self.write(&mut *writer, &x.ext_compression)?;
        self.write(&mut *writer, &x.ext_patch)?;

        Ok(())
    }
}

impl<R> RCodec<Cookie, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Cookie, Self::Error> {
        let zid: ZenohIdProto = self.read(&mut *reader)?;
        let wai: u8 = self.read(&mut *reader)?;
        let whatami = WhatAmI::try_from(wai).map_err(|_| DidntRead)?;
        let resolution: u8 = self.read(&mut *reader)?;
        let resolution = Resolution::from(resolution);
        let batch_size: BatchSize = self.read(&mut *reader)?;
        let nonce: u64 = self.read(&mut *reader)?;
        // Extensions
        let ext_qos: ext::qos::StateAccept = self.read(&mut *reader)?;
        #[cfg(feature = "transport_multilink")]
        let ext_mlink: ext::multilink::StateAccept = self.read(&mut *reader)?;
        #[cfg(feature = "shared-memory")]
        let ext_shm: ext::shm::StateAccept = self.read(&mut *reader)?;
        #[cfg(feature = "transport_auth")]
        let ext_auth: ext::auth::StateAccept = self.read(&mut *reader)?;
        let ext_lowlatency: ext::lowlatency::StateAccept = self.read(&mut *reader)?;
        #[cfg(feature = "transport_compression")]
        let ext_compression: ext::compression::StateAccept = self.read(&mut *reader)?;
        let ext_patch: ext::patch::StateAccept = self.read(&mut *reader)?;

        let cookie = Cookie {
            zid,
            whatami,
            resolution,
            batch_size,
            nonce,
            ext_qos,
            #[cfg(feature = "transport_multilink")]
            ext_mlink,
            #[cfg(feature = "shared-memory")]
            ext_shm,
            #[cfg(feature = "transport_auth")]
            ext_auth,
            ext_lowlatency,
            #[cfg(feature = "transport_compression")]
            ext_compression,
            ext_patch,
        };

        Ok(cookie)
    }
}

pub(super) struct Zenoh080Cookie<'a> {
    pub(super) cipher: &'a BlockCipher,
    pub(super) prng: &'a mut PseudoRng,
    pub(super) codec: Zenoh080,
}

impl<W> WCodec<&Cookie, &mut W> for &mut Zenoh080Cookie<'_>
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &Cookie) -> Self::Output {
        let mut buff = vec![];
        let mut support = buff.writer();

        self.codec.write(&mut support, x)?;

        let encrypted = self.cipher.encrypt(buff, self.prng);
        self.codec.write(&mut *writer, encrypted.as_slice())?;

        Ok(())
    }
}

impl<R> RCodec<Cookie, &mut R> for &mut Zenoh080Cookie<'_>
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Cookie, Self::Error> {
        let bytes: Vec<u8> = self.codec.read(&mut *reader)?;
        let decrypted = self.cipher.decrypt(bytes).map_err(|_| DidntRead)?;

        let mut reader = decrypted.reader();
        let cookie: Cookie = self.codec.read(&mut reader)?;

        Ok(cookie)
    }
}

impl Cookie {
    #[cfg(test)]
    pub(crate) fn rand() -> Self {
        use rand::Rng;

        let mut rng = rand::thread_rng();

        Self {
            zid: ZenohIdProto::default(),
            whatami: WhatAmI::rand(),
            resolution: Resolution::rand(),
            batch_size: rng.gen(),
            nonce: rng.gen(),
            ext_qos: ext::qos::StateAccept::rand(),
            #[cfg(feature = "transport_multilink")]
            ext_mlink: ext::multilink::StateAccept::rand(),
            #[cfg(feature = "shared-memory")]
            ext_shm: ext::shm::StateAccept::rand(),
            #[cfg(feature = "transport_auth")]
            ext_auth: ext::auth::StateAccept::rand(),
            ext_lowlatency: ext::lowlatency::StateAccept::rand(),
            #[cfg(feature = "transport_compression")]
            ext_compression: ext::compression::StateAccept::rand(),
            ext_patch: ext::patch::StateAccept::rand(),
        }
    }
}

mod tests {
    #[test]
    fn codec_cookie() {
        use rand::{Rng, SeedableRng};
        use zenoh_buffers::ZBuf;

        use super::*;

        const NUM_ITER: usize = 1_000;

        macro_rules! run_single {
            ($type:ty, $rand:expr, $wcode:expr, $rcode:expr, $buff:expr) => {
                for _ in 0..NUM_ITER {
                    let x: $type = $rand;

                    $buff.clear();
                    let mut writer = $buff.writer();
                    $wcode.write(&mut writer, &x).unwrap();

                    let mut reader = $buff.reader();
                    let y: $type = $rcode.read(&mut reader).unwrap();
                    assert!(!reader.can_read());

                    assert_eq!(x, y);
                }
            };
        }

        macro_rules! run {
            ($type:ty, $rand:expr, $codec:expr) => {
                println!("Vec<u8>: codec {}", std::any::type_name::<$type>());
                let mut buffer = vec![];
                run_single!($type, $rand, $codec, $codec, buffer);

                println!("ZBuf: codec {}", std::any::type_name::<$type>());
                let mut buffer = ZBuf::empty();
                run_single!($type, $rand, $codec, $codec, buffer);
            };
        }

        let codec = Zenoh080::new();
        run!(Cookie, Cookie::rand(), codec);

        let mut prng = PseudoRng::from_entropy();
        let mut key = [0u8; BlockCipher::BLOCK_SIZE];
        prng.fill(&mut key[..]);

        let cipher = BlockCipher::new(key);
        let mut codec = Zenoh080Cookie {
            prng: &mut prng,
            cipher: &cipher,
            codec: Zenoh080::new(),
        };

        run!(Cookie, Cookie::rand(), codec);
    }
}
