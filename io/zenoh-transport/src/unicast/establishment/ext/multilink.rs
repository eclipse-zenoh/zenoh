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
use crate::{
    establishment::AcceptFsm,
    unicast::establishment::{
        ext::auth::pubkey::{self, AuthPubKey, AuthPubKeyFsm, ZPublicKey},
        OpenFsm,
    },
};
use async_std::sync::{Mutex, RwLock};
use async_trait::async_trait;
use rand::{CryptoRng, Rng};
use rsa::{RsaPrivateKey, RsaPublicKey};
use std::collections::HashMap;
use zenoh_buffers::{
    reader::{DidntRead, HasReader, Reader},
    writer::{DidntWrite, Writer},
};
use zenoh_codec::{RCodec, WCodec, Zenoh080};
use zenoh_core::{bail, zasynclock, zasyncwrite, zerror, Error as ZError, Result as ZResult};
use zenoh_crypto::PseudoRng;
use zenoh_protocol::{
    core::ZenohId,
    transport::{init, open},
};

const KEY_SIZE: usize = 512;

// Extension Fsm
pub(crate) struct MultiLink {
    known: Mutex<HashMap<ZenohId, Option<ZPublicKey>>>,
    pubkey: RwLock<AuthPubKey>,
}

impl MultiLink {
    pub(crate) fn make<R>(rng: &mut R) -> ZResult<Self>
    where
        R: Rng + CryptoRng,
    {
        let pri_key = RsaPrivateKey::new(rng, KEY_SIZE)?;
        let pub_key = RsaPublicKey::from(&pri_key);
        let s = Self {
            known: Mutex::new(HashMap::new()),
            pubkey: RwLock::new(AuthPubKey::new(pub_key.into(), pri_key.into())),
        };
        Ok(s)
    }

    pub(crate) fn open(&self, is_multilink: bool) -> StateOpen {
        StateOpen {
            pubkey: is_multilink.then_some(pubkey::StateOpen::new()),
        }
    }

    pub(crate) fn accept(&self, is_multilink: bool) -> StateAccept {
        StateAccept {
            pubkey: is_multilink.then_some(pubkey::StateAccept::new()),
        }
    }

    pub(crate) fn fsm<'a>(&'a self, prng: &'a Mutex<PseudoRng>) -> MultiLinkFsm<'a> {
        MultiLinkFsm {
            known: &self.known,
            pubkey: &self.pubkey,
            fsm: AuthPubKeyFsm::new(&self.pubkey, prng),
        }
    }

    pub(crate) async fn close(&self, zid: ZenohId) -> ZResult<()> {
        if let Some(Some(pk)) = zasynclock!(self.known).remove(&zid) {
            zasyncwrite!(self.pubkey).del_pubkey(&pk).await?;
        }
        Ok(())
    }
}

pub(crate) struct MultiLinkFsm<'a> {
    known: &'a Mutex<HashMap<ZenohId, Option<ZPublicKey>>>,
    pubkey: &'a RwLock<AuthPubKey>,
    fsm: AuthPubKeyFsm<'a>,
}

/*************************************/
/*              OPEN                 */
/*************************************/
pub(crate) struct StateOpen {
    pubkey: Option<pubkey::StateOpen>,
}

impl StateOpen {
    pub(crate) const fn is_multilink(&self) -> bool {
        self.pubkey.is_some()
    }
}

#[async_trait]
impl<'a> OpenFsm for MultiLinkFsm<'a> {
    type Error = ZError;

    type SendInitSynIn = &'a StateOpen;
    type SendInitSynOut = Option<init::ext::MultiLink>;
    async fn send_init_syn(
        &self,
        input: Self::SendInitSynIn,
    ) -> Result<Self::SendInitSynOut, Self::Error> {
        let pubkey = match input.pubkey.as_ref() {
            Some(pubkey) => pubkey,
            None => return Ok(None),
        };

        let r = self.fsm.send_init_syn(pubkey).await?.map(|x| x.transmute());
        Ok(r)
    }

    type RecvInitAckIn = (&'a mut StateOpen, Option<init::ext::MultiLink>);
    type RecvInitAckOut = ();
    async fn recv_init_ack(
        &self,
        input: Self::RecvInitAckIn,
    ) -> Result<Self::RecvInitAckOut, Self::Error> {
        let (state, mut ext) = input;
        let pubkey = match state.pubkey.as_mut() {
            Some(pubkey) => pubkey,
            None => return Ok(()),
        };

        match ext.take() {
            Some(ext) => {
                self.fsm
                    .recv_init_ack((pubkey, Some(ext.transmute())))
                    .await
            }
            None => {
                state.pubkey = None;
                Ok(())
            }
        }
    }

    type SendOpenSynIn = &'a StateOpen;
    type SendOpenSynOut = Option<open::ext::MultiLinkSyn>;
    async fn send_open_syn(
        &self,
        input: Self::SendOpenSynIn,
    ) -> Result<Self::SendOpenSynOut, Self::Error> {
        let pubkey = match input.pubkey.as_ref() {
            Some(pubkey) => pubkey,
            None => return Ok(None),
        };

        let r = self.fsm.send_open_syn(pubkey).await?.map(|x| x.transmute());
        Ok(r)
    }

    type RecvOpenAckIn = (&'a mut StateOpen, Option<open::ext::MultiLinkAck>);
    type RecvOpenAckOut = ();
    async fn recv_open_ack(
        &self,
        input: Self::RecvOpenAckIn,
    ) -> Result<Self::RecvOpenAckOut, Self::Error> {
        let (state, mut ext) = input;
        let pubkey = match state.pubkey.as_mut() {
            Some(pubkey) => pubkey,
            None => return Ok(()),
        };

        match ext.take() {
            Some(ext) => {
                self.fsm
                    .recv_open_ack((pubkey, Some(ext.transmute())))
                    .await
            }
            None => {
                state.pubkey = None;
                Ok(())
            }
        }
    }
}

/*************************************/
/*            ACCEPT                 */
/*************************************/
#[derive(Debug, PartialEq)]
pub(crate) struct StateAccept {
    pubkey: Option<pubkey::StateAccept>,
}

impl StateAccept {
    #[cfg(test)]
    pub(crate) fn rand() -> Self {
        let mut rng = rand::thread_rng();
        Self {
            pubkey: rng.gen_bool(0.5).then_some(pubkey::StateAccept::rand()),
        }
    }
}

impl<W> WCodec<&StateAccept, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &StateAccept) -> Self::Output {
        match x.pubkey.as_ref() {
            Some(p) => {
                self.write(&mut *writer, 1u8)?;
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
        Ok(StateAccept {
            pubkey: if is_multilink == 1 {
                Some(self.read(&mut *reader)?)
            } else {
                None
            },
        })
    }
}

#[async_trait]
impl<'a> AcceptFsm for MultiLinkFsm<'a> {
    type Error = ZError;

    type RecvInitSynIn = (&'a mut StateAccept, Option<init::ext::MultiLink>, ZenohId);
    type RecvInitSynOut = ();
    async fn recv_init_syn(
        &self,
        input: Self::RecvInitSynIn,
    ) -> Result<Self::RecvInitSynOut, Self::Error> {
        const S: &str = "MultiLink extension - Recv InitSyn.";

        let (state, mut ext, zid) = input;
        let pubkey = match state.pubkey.as_mut() {
            Some(pubkey) => pubkey,
            None => return Ok(()),
        };

        match ext.take() {
            Some(ext) => {
                // Verify if the configuration is coherent with what we have in memory
                let codec = Zenoh080::new();
                let mut reader = ext.value.reader();
                let init_syn: pubkey::InitSyn = codec
                    .read(&mut reader)
                    .map_err(|_| zerror!("{S} Decoding error."))?;

                {
                    let mut m_known = zasynclock!(self.known);
                    match m_known.get(&zid) {
                        Some(Some(pk)) if pk != &init_syn.alice_pubkey => {
                            bail!("{S} Spoofing detected.");
                        }
                        Some(None) => bail!("{S} Spoofing detected."),
                        _ => {}
                    }
                    m_known.insert(zid, Some(init_syn.alice_pubkey.clone()));
                    zasyncwrite!(self.pubkey)
                        .add_pubkey(init_syn.alice_pubkey)
                        .await?;
                }

                self.fsm
                    .recv_open_syn((pubkey, Some(ext.transmute())))
                    .await
            }
            None => {
                // Verify if the configuration is coherent with what we have in memory
                let mut m_known = zasynclock!(self.known);
                match m_known.get(&zid) {
                    Some(Some(_)) => {
                        bail!("{S} Spoofing detected.");
                    }
                    Some(None) => {}
                    None => {
                        let _ = m_known.insert(zid, None);
                    }
                }
                state.pubkey = None;
                Ok(())
            }
        }
    }

    type SendInitAckIn = &'a StateAccept;
    type SendInitAckOut = Option<init::ext::MultiLink>;
    async fn send_init_ack(
        &self,
        input: Self::SendInitAckIn,
    ) -> Result<Self::SendInitAckOut, Self::Error> {
        let pubkey = match input.pubkey.as_ref() {
            Some(pubkey) => pubkey,
            None => return Ok(None),
        };

        let r = self.fsm.send_init_ack(pubkey).await?.map(|x| x.transmute());
        Ok(r)
    }

    type RecvOpenSynIn = (&'a mut StateAccept, Option<open::ext::MultiLinkSyn>);
    type RecvOpenSynOut = ();
    async fn recv_open_syn(
        &self,
        input: Self::RecvOpenSynIn,
    ) -> Result<Self::RecvOpenSynOut, Self::Error> {
        let (state, ext) = input;
        let pubkey = match state.pubkey.as_mut() {
            Some(pubkey) => pubkey,
            None => return Ok(()),
        };

        self.fsm
            .recv_open_syn((pubkey, ext.map(|x| x.transmute())))
            .await
    }

    type SendOpenAckIn = &'a StateAccept;
    type SendOpenAckOut = Option<open::ext::MultiLinkAck>;
    async fn send_open_ack(
        &self,
        input: Self::SendOpenAckIn,
    ) -> Result<Self::SendOpenAckOut, Self::Error> {
        let pubkey = match input.pubkey.as_ref() {
            Some(pubkey) => pubkey,
            None => return Ok(None),
        };

        let r = self.fsm.send_open_ack(pubkey).await?.map(|x| x.transmute());
        Ok(r)
    }
}
