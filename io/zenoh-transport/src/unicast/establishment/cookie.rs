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
use super::properties::EstablishmentProperties;
use zenoh_buffers::{
    reader::{DidntRead, HasReader, Reader},
    writer::{DidntWrite, HasWriter, Writer},
};
use zenoh_codec::{RCodec, WCodec, Zenoh060};
use zenoh_crypto::{BlockCipher, PseudoRng};
use zenoh_protocol::core::{Property, WhatAmI, ZInt, ZenohId};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cookie {
    pub whatami: WhatAmI,
    pub zid: ZenohId,
    pub sn_resolution: ZInt,
    pub is_qos: bool,
    pub nonce: ZInt,
    pub properties: EstablishmentProperties,
}

impl<W> WCodec<&Cookie, &mut W> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &Cookie) -> Self::Output {
        let wai: ZInt = x.whatami.into();
        self.write(&mut *writer, wai)?;
        self.write(&mut *writer, &x.zid)?;
        self.write(&mut *writer, x.sn_resolution)?;
        let is_qos = u8::from(x.is_qos);
        self.write(&mut *writer, is_qos)?;
        self.write(&mut *writer, x.nonce)?;
        self.write(&mut *writer, x.properties.as_slice())?;

        Ok(())
    }
}

impl<R> RCodec<Cookie, &mut R> for Zenoh060
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Cookie, Self::Error> {
        let wai: ZInt = self.read(&mut *reader)?;
        let whatami = WhatAmI::try_from(wai).ok_or(DidntRead)?;
        let zid: ZenohId = self.read(&mut *reader)?;
        let sn_resolution: ZInt = self.read(&mut *reader)?;
        let is_qos: u8 = self.read(&mut *reader)?;
        let is_qos = is_qos == 1;
        let nonce: ZInt = self.read(&mut *reader)?;
        let mut ps: Vec<Property> = self.read(&mut *reader)?;
        let mut properties = EstablishmentProperties::new();
        for p in ps.drain(..) {
            properties.insert(p).map_err(|_| DidntRead)?;
        }

        let cookie = Cookie {
            whatami,
            zid,
            sn_resolution,
            is_qos,
            nonce,
            properties,
        };

        Ok(cookie)
    }
}

pub(super) struct Zenoh060Cookie<'a> {
    pub(super) cipher: &'a BlockCipher,
    pub(super) prng: &'a mut PseudoRng,
    pub(super) codec: Zenoh060,
}

impl<W> WCodec<&Cookie, &mut W> for &mut Zenoh060Cookie<'_>
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

impl<R> RCodec<Cookie, &mut R> for &mut Zenoh060Cookie<'_>
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
    pub fn rand() -> Self {
        use rand::Rng;

        let mut rng = rand::thread_rng();

        Self {
            whatami: WhatAmI::rand(),
            zid: ZenohId::default(),
            sn_resolution: rng.gen(),
            is_qos: rng.gen_bool(0.5),
            nonce: rng.gen(),
            properties: EstablishmentProperties::rand(),
        }
    }
}

mod tests {
    #[test]
    fn codec_cookie() {
        use super::*;
        use rand::{Rng, SeedableRng};
        use zenoh_buffers::ZBuf;

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
                let mut buffer = ZBuf::default();
                run_single!($type, $rand, $codec, $codec, buffer);
            };
        }

        let codec = Zenoh060::default();
        run!(Cookie, Cookie::rand(), codec);

        let mut prng = PseudoRng::from_entropy();
        let mut key = [0u8; BlockCipher::BLOCK_SIZE];
        prng.fill(&mut key[..]);

        let cipher = BlockCipher::new(key);
        let mut codec = Zenoh060Cookie {
            prng: &mut prng,
            cipher: &cipher,
            codec: Zenoh060::default(),
        };

        run!(Cookie, Cookie::rand(), codec);
    }
}
