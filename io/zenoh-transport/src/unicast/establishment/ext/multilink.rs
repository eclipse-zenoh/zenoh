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
use async_trait::async_trait;
use rand::{CryptoRng, Rng};
use rsa::{BigUint, RsaPrivateKey, RsaPublicKey};
use tokio::sync::{Mutex, RwLock};
use zenoh_buffers::{
    reader::{DidntRead, HasReader, Reader},
    writer::{DidntWrite, Writer},
};
use zenoh_codec::{RCodec, WCodec, Zenoh080};
use zenoh_core::{zerror, Error as ZError, Result as ZResult};
use zenoh_crypto::PseudoRng;
use zenoh_protocol::transport::{init, open};

use crate::unicast::establishment::{
    ext::auth::pubkey::{self, AuthPubKey, AuthPubKeyFsm, ZPublicKey},
    AcceptFsm, OpenFsm,
};

const KEY_SIZE: usize = 512;

// Extension Fsm
pub(crate) struct MultiLink {
    pubkey: Option<RwLock<AuthPubKey>>,
}

impl MultiLink {
    pub(crate) fn make<R>(rng: &mut R, is_multilink: bool) -> ZResult<Self>
    where
        R: Rng + CryptoRng,
    {
        if is_multilink {
            let pri_key = RsaPrivateKey::new(rng, KEY_SIZE)?;
            let pub_key = RsaPublicKey::from(&pri_key);
            let mut auth = AuthPubKey::new(pub_key.into(), pri_key.into());
            auth.disable_lookup();
            Ok(Self {
                pubkey: Some(RwLock::new(auth)),
            })
        } else {
            Ok(Self { pubkey: None })
        }
    }

    pub(crate) fn open(&self, is_multilink: bool) -> StateOpen {
        StateOpen {
            pubkey: is_multilink.then_some((
                pubkey::StateOpen::new(),
                RsaPublicKey::new_unchecked(BigUint::new(vec![]), BigUint::new(vec![])).into(),
            )),
        }
    }

    pub(crate) fn accept(&self, is_multilink: bool) -> StateAccept {
        StateAccept {
            pubkey: is_multilink.then_some((
                pubkey::StateAccept::new(),
                RsaPublicKey::new_unchecked(BigUint::new(vec![]), BigUint::new(vec![])).into(),
            )),
        }
    }

    pub(crate) fn fsm<'a>(&'a self, prng: &'a Mutex<PseudoRng>) -> MultiLinkFsm<'a> {
        MultiLinkFsm {
            fsm: self
                .pubkey
                .is_some()
                .then(|| AuthPubKeyFsm::new(self.pubkey.as_ref().unwrap(), prng)),
        }
    }
}

pub(crate) struct MultiLinkFsm<'a> {
    fsm: Option<AuthPubKeyFsm<'a>>,
}

/*************************************/
/*              OPEN                 */
/*************************************/
pub(crate) struct StateOpen {
    pubkey: Option<(pubkey::StateOpen, ZPublicKey)>,
}

impl StateOpen {
    pub(crate) fn multilink(&self) -> Option<ZPublicKey> {
        self.pubkey.as_ref().map(|(_, p)| p.clone())
    }
}

#[async_trait]
impl<'a> OpenFsm for &'a MultiLinkFsm<'a> {
    type Error = ZError;

    type SendInitSynIn = &'a StateOpen;
    type SendInitSynOut = Option<init::ext::MultiLink>;
    async fn send_init_syn(
        self,
        input: Self::SendInitSynIn,
    ) -> Result<Self::SendInitSynOut, Self::Error> {
        let (pubkey, fsm) = match (input.pubkey.as_ref(), self.fsm.as_ref()) {
            (Some(pubkey), Some(fsm)) => (pubkey, fsm),
            _ => return Ok(None),
        };

        let r = fsm.send_init_syn(&pubkey.0).await?.map(|x| x.transmute());
        Ok(r)
    }

    type RecvInitAckIn = (&'a mut StateOpen, Option<init::ext::MultiLink>);
    type RecvInitAckOut = ();
    async fn recv_init_ack(
        self,
        input: Self::RecvInitAckIn,
    ) -> Result<Self::RecvInitAckOut, Self::Error> {
        const S: &str = "MultiLink extension - Recv InitAck.";

        let (state, mut ext) = input;
        let (mut pubkey, fsm) = match (state.pubkey.take(), self.fsm.as_ref()) {
            (Some(pubkey), Some(fsm)) => (pubkey, fsm),
            _ => return Ok(()),
        };

        match ext.take() {
            Some(ext) => {
                let codec = Zenoh080::new();
                let mut reader = ext.value.reader();
                let init_ack: pubkey::InitAck = codec
                    .read(&mut reader)
                    .map_err(|_| zerror!("{S} Decoding error."))?;

                fsm.recv_init_ack((&mut pubkey.0, Some(ext.transmute())))
                    .await?;

                state.pubkey = Some((pubkey.0, init_ack.bob_pubkey));
            }
            None => {
                state.pubkey = None;
            }
        }
        Ok(())
    }

    type SendOpenSynIn = &'a StateOpen;
    type SendOpenSynOut = Option<open::ext::MultiLinkSyn>;
    async fn send_open_syn(
        self,
        input: Self::SendOpenSynIn,
    ) -> Result<Self::SendOpenSynOut, Self::Error> {
        let (pubkey, fsm) = match (input.pubkey.as_ref(), self.fsm.as_ref()) {
            (Some(pubkey), Some(fsm)) => (pubkey, fsm),
            _ => return Ok(None),
        };

        let r = fsm.send_open_syn(&pubkey.0).await?.map(|x| x.transmute());
        Ok(r)
    }

    type RecvOpenAckIn = (&'a mut StateOpen, Option<open::ext::MultiLinkAck>);
    type RecvOpenAckOut = ();
    async fn recv_open_ack(
        self,
        input: Self::RecvOpenAckIn,
    ) -> Result<Self::RecvOpenAckOut, Self::Error> {
        let (state, mut ext) = input;
        let (pubkey, fsm) = match (state.pubkey.as_mut(), self.fsm.as_ref()) {
            (Some(pubkey), Some(fsm)) => (pubkey, fsm),
            _ => return Ok(()),
        };

        match ext.take() {
            Some(ext) => {
                fsm.recv_open_ack((&mut pubkey.0, Some(ext.transmute())))
                    .await?;
            }
            None => {
                state.pubkey = None;
            }
        }

        Ok(())
    }
}

/*************************************/
/*            ACCEPT                 */
/*************************************/
#[derive(Debug, PartialEq)]
pub(crate) struct StateAccept {
    pubkey: Option<(pubkey::StateAccept, ZPublicKey)>,
}

impl StateAccept {
    pub(crate) fn multilink(&self) -> Option<ZPublicKey> {
        self.pubkey.as_ref().map(|(_, p)| p.clone())
    }

    #[cfg(test)]
    pub(crate) fn rand() -> Self {
        let mut rng = rand::thread_rng();
        let pubkey = if rng.gen_bool(0.5) {
            let n = BigUint::from_bytes_le(&[
                0x41, 0x74, 0xc6, 0x40, 0x18, 0x63, 0xbd, 0x59, 0xe6, 0x0d, 0xe9, 0x23, 0x3e, 0x95,
                0xca, 0xb4, 0x5d, 0x17, 0x3d, 0x14, 0xdd, 0xbb, 0x16, 0x4a, 0x49, 0xeb, 0x43, 0x27,
                0x79, 0x3e, 0x75, 0x67, 0xd6, 0xf6, 0x7f, 0xe7, 0xbf, 0xb5, 0x1d, 0xf6, 0x27, 0x80,
                0xca, 0x26, 0x35, 0xa2, 0xc5, 0x4c, 0x96, 0x50, 0xaa, 0x9f, 0xf4, 0x47, 0xbe, 0x06,
                0x9c, 0xd1, 0xec, 0xfd, 0x1e, 0x81, 0xe9, 0xc4,
            ]);
            let e = BigUint::from_bytes_le(&[0x01, 0x00, 0x01]);
            let pub_key = RsaPublicKey::new(n, e).unwrap();
            Some((pubkey::StateAccept::rand(), pub_key.into()))
        } else {
            None
        };
        Self { pubkey }
    }
}

impl<W> WCodec<&StateAccept, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &StateAccept) -> Self::Output {
        match x.pubkey.as_ref() {
            Some((s, p)) => {
                self.write(&mut *writer, 1u8)?;
                self.write(&mut *writer, s)?;
                self.write(&mut *writer, p)
            }
            None => self.write(&mut *writer, 0u8),
        }
    }
}

impl<R> RCodec<StateAccept, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<StateAccept, Self::Error> {
        let is_multilink: u8 = self.read(&mut *reader)?;
        let pubkey = if is_multilink == 1 {
            let s: pubkey::StateAccept = self.read(&mut *reader)?;
            let p: ZPublicKey = self.read(&mut *reader)?;
            Some((s, p))
        } else {
            None
        };
        Ok(StateAccept { pubkey })
    }
}

#[async_trait]
impl<'a> AcceptFsm for &'a MultiLinkFsm<'a> {
    type Error = ZError;

    type RecvInitSynIn = (&'a mut StateAccept, Option<init::ext::MultiLink>);
    type RecvInitSynOut = ();
    async fn recv_init_syn(
        self,
        input: Self::RecvInitSynIn,
    ) -> Result<Self::RecvInitSynOut, Self::Error> {
        const S: &str = "MultiLink extension - Recv InitSyn.";

        let (state, mut ext) = input;
        let (mut pubkey, fsm) = match (state.pubkey.take(), self.fsm.as_ref()) {
            (Some(pubkey), Some(fsm)) => (pubkey, fsm),
            _ => return Ok(()),
        };

        match ext.take() {
            Some(ext) => {
                let codec = Zenoh080::new();
                let mut reader = ext.value.reader();
                let init_syn: pubkey::InitSyn = codec
                    .read(&mut reader)
                    .map_err(|_| zerror!("{S} Decoding error."))?;

                fsm.recv_init_syn((&mut pubkey.0, Some(ext.transmute())))
                    .await?;

                state.pubkey = Some((pubkey.0, init_syn.alice_pubkey));
            }
            None => {
                state.pubkey = None;
            }
        }

        Ok(())
    }

    type SendInitAckIn = &'a StateAccept;
    type SendInitAckOut = Option<init::ext::MultiLink>;
    async fn send_init_ack(
        self,
        input: Self::SendInitAckIn,
    ) -> Result<Self::SendInitAckOut, Self::Error> {
        let (pubkey, fsm) = match (input.pubkey.as_ref(), self.fsm.as_ref()) {
            (Some(pubkey), Some(fsm)) => (pubkey, fsm),
            _ => return Ok(None),
        };

        let r = fsm.send_init_ack(&pubkey.0).await?.map(|x| x.transmute());
        Ok(r)
    }

    type RecvOpenSynIn = (&'a mut StateAccept, Option<open::ext::MultiLinkSyn>);
    type RecvOpenSynOut = ();
    async fn recv_open_syn(
        self,
        input: Self::RecvOpenSynIn,
    ) -> Result<Self::RecvOpenSynOut, Self::Error> {
        let (state, ext) = input;
        let (pubkey, fsm) = match (state.pubkey.as_mut(), self.fsm.as_ref()) {
            (Some(pubkey), Some(fsm)) => (pubkey, fsm),
            _ => return Ok(()),
        };

        fsm.recv_open_syn((&mut pubkey.0, ext.map(|x| x.transmute())))
            .await
    }

    type SendOpenAckIn = &'a StateAccept;
    type SendOpenAckOut = Option<open::ext::MultiLinkAck>;
    async fn send_open_ack(
        self,
        input: Self::SendOpenAckIn,
    ) -> Result<Self::SendOpenAckOut, Self::Error> {
        let (pubkey, fsm) = match (input.pubkey.as_ref(), self.fsm.as_ref()) {
            (Some(pubkey), Some(fsm)) => (pubkey, fsm),
            _ => return Ok(None),
        };

        let r = fsm.send_open_ack(&pubkey.0).await?.map(|x| x.transmute());
        Ok(r)
    }
}
