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
#[cfg(feature = "auth_pubkey")]
pub(crate) mod pubkey;
#[cfg(feature = "auth_usrpwd")]
pub(crate) mod usrpwd;

use std::{convert::TryInto, marker::PhantomData};

use async_trait::async_trait;
#[cfg(feature = "auth_pubkey")]
pub use pubkey::*;
use rand::{CryptoRng, Rng};
use tokio::sync::{Mutex, RwLock};
#[cfg(feature = "auth_usrpwd")]
pub use usrpwd::*;
use zenoh_buffers::{
    reader::{DidntRead, HasReader, Reader, SiphonableReader},
    writer::{DidntWrite, HasWriter, Writer},
    ZBuf,
};
use zenoh_codec::{RCodec, WCodec, Zenoh080};
use zenoh_config::Config;
use zenoh_core::{bail, zerror, Error as ZError, Result as ZResult};
use zenoh_crypto::PseudoRng;
use zenoh_protocol::{
    common::{iext, ZExtUnknown},
    transport::{init, open},
};

use crate::unicast::establishment::{AcceptFsm, OpenFsm};

pub(crate) mod id {
    #[cfg(feature = "auth_pubkey")]
    pub(crate) const PUBKEY: u8 = 0x1;
    #[cfg(feature = "auth_usrpwd")]
    pub(crate) const USRPWD: u8 = 0x2;
}

#[derive(Debug, Default)]
pub struct Auth {
    #[cfg(feature = "auth_pubkey")]
    pubkey: Option<RwLock<AuthPubKey>>,
    #[cfg(feature = "auth_usrpwd")]
    usrpwd: Option<RwLock<AuthUsrPwd>>,
}

impl Auth {
    pub(crate) async fn from_config(config: &Config) -> ZResult<Self> {
        let auth = config.transport().auth();

        Ok(Self {
            #[cfg(feature = "auth_pubkey")]
            pubkey: AuthPubKey::from_config(auth.pubkey())?.map(RwLock::new),
            #[cfg(feature = "auth_usrpwd")]
            usrpwd: AuthUsrPwd::from_config(auth.usrpwd())
                .await?
                .map(RwLock::new),
        })
    }

    pub(crate) fn open<R>(&self, #[allow(unused)] prng: &mut R) -> StateOpen
    where
        R: Rng + CryptoRng,
    {
        StateOpen {
            #[cfg(feature = "auth_pubkey")]
            pubkey: self.pubkey.is_some().then_some(pubkey::StateOpen::new()),
            #[cfg(feature = "auth_usrpwd")]
            usrpwd: self
                .usrpwd
                .is_some()
                .then_some(usrpwd::StateOpen::new(prng)),
        }
    }

    pub(crate) fn accept<R>(&self, #[allow(unused)] prng: &mut R) -> StateAccept
    where
        R: Rng + CryptoRng,
    {
        StateAccept {
            #[cfg(feature = "auth_pubkey")]
            pubkey: self.pubkey.is_some().then_some(pubkey::StateAccept::new()),
            #[cfg(feature = "auth_usrpwd")]
            usrpwd: self
                .usrpwd
                .is_some()
                .then_some(usrpwd::StateAccept::new(prng)),
        }
    }

    pub(crate) fn fsm<'a>(&'a self, #[allow(unused)] prng: &'a Mutex<PseudoRng>) -> AuthFsm<'a> {
        AuthFsm {
            #[cfg(feature = "auth_pubkey")]
            pubkey: self.pubkey.as_ref().map(|x| AuthPubKeyFsm::new(x, prng)),
            #[cfg(feature = "auth_usrpwd")]
            usrpwd: self.usrpwd.as_ref().map(AuthUsrPwdFsm::new),
            _a: PhantomData,
        }
    }
}

#[cfg(feature = "test")]
impl Auth {
    pub const fn empty() -> Self {
        Self {
            #[cfg(feature = "auth_pubkey")]
            pubkey: None,
            #[cfg(feature = "auth_usrpwd")]
            usrpwd: None,
        }
    }

    #[cfg(feature = "auth_pubkey")]
    pub fn set_pubkey(&mut self, pubkey: Option<AuthPubKey>) {
        self.pubkey = pubkey.map(RwLock::new);
    }

    #[cfg(feature = "auth_pubkey")]
    pub fn get_pubkey(&self) -> Option<&RwLock<AuthPubKey>> {
        self.pubkey.as_ref()
    }

    #[cfg(feature = "auth_usrpwd")]
    pub fn set_usrpwd(&mut self, usrpwd: Option<AuthUsrPwd>) {
        self.usrpwd = usrpwd.map(RwLock::new);
    }

    #[cfg(feature = "auth_usrpwd")]
    pub fn get_usrpwd(&self) -> Option<&RwLock<AuthUsrPwd>> {
        self.usrpwd.as_ref()
    }
}

pub(crate) struct AuthFsm<'a> {
    #[cfg(feature = "auth_pubkey")]
    pubkey: Option<AuthPubKeyFsm<'a>>,
    #[cfg(feature = "auth_usrpwd")]
    usrpwd: Option<AuthUsrPwdFsm<'a>>,
    _a: PhantomData<&'a ()>, // Required only when all auth features are disabled
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct StateOpen {
    #[cfg(feature = "auth_pubkey")]
    pubkey: Option<pubkey::StateOpen>,
    #[cfg(feature = "auth_usrpwd")]
    usrpwd: Option<usrpwd::StateOpen>,
}

#[derive(Debug, PartialEq)]
pub(crate) struct StateAccept {
    #[cfg(feature = "auth_pubkey")]
    pubkey: Option<pubkey::StateAccept>,
    #[cfg(feature = "auth_usrpwd")]
    usrpwd: Option<usrpwd::StateAccept>,
}

impl StateAccept {
    #[cfg(test)]
    pub(crate) fn rand() -> Self {
        let mut rng = rand::thread_rng();
        Self {
            #[cfg(feature = "auth_pubkey")]
            pubkey: rng.gen_bool(0.5).then_some(pubkey::StateAccept::rand()),
            #[cfg(feature = "auth_usrpwd")]
            usrpwd: rng.gen_bool(0.5).then_some(usrpwd::StateAccept::rand()),
        }
    }
}

// Codec
impl<W> WCodec<&StateAccept, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &StateAccept) -> Self::Output {
        let mut count: usize = 0;
        let mut buff = vec![];
        let mut wbuf = buff.writer();

        #[cfg(feature = "auth_pubkey")]
        {
            if let Some(pubkey) = x.pubkey.as_ref() {
                self.write(&mut wbuf, id::PUBKEY)?;
                self.write(&mut wbuf, pubkey)?;
                count += 1;
            }
        }

        #[cfg(feature = "auth_usrpwd")]
        {
            if let Some(usrpwd) = x.usrpwd.as_ref() {
                self.write(&mut wbuf, id::USRPWD)?;
                self.write(&mut wbuf, usrpwd)?;
                count += 1;
            }
        }

        self.write(&mut *writer, count)?;
        if !buff.is_empty() {
            let mut rbuf = buff.reader();
            rbuf.siphon(&mut *writer).map_err(|_| DidntWrite)?;
        }

        Ok(())
    }
}

impl<R> RCodec<StateAccept, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<StateAccept, Self::Error> {
        let mut count: usize = self.read(&mut *reader)?;

        #[cfg(feature = "auth_pubkey")]
        let mut pubkey: Option<pubkey::StateAccept> = None;
        #[cfg(feature = "auth_usrpwd")]
        let mut usrpwd: Option<usrpwd::StateAccept> = None;

        while count > 0 {
            let e: u8 = self.read(&mut *reader)?;
            match e {
                #[cfg(feature = "auth_pubkey")]
                id::PUBKEY => {
                    pubkey = Some(self.read(&mut *reader)?);
                }
                #[cfg(feature = "auth_usrpwd")]
                id::USRPWD => {
                    usrpwd = Some(self.read(&mut *reader)?);
                }
                _ => return Err(DidntRead),
            }

            count -= 1;
        }

        let state = StateAccept {
            #[cfg(feature = "auth_pubkey")]
            pubkey,
            #[cfg(feature = "auth_usrpwd")]
            usrpwd,
        };
        Ok(state)
    }
}

macro_rules! ztryinto {
    ($x:expr, $s:expr) => {
        if let Some(x) = $x {
            Some(
                x.try_into()
                    .map_err(|_| zerror!("{} Decoding error.", $s))?,
            )
        } else {
            None
        }
    };
}

macro_rules! ztake {
    ($x:expr, $id:expr) => {
        $x.iter()
            .position(|x| x.id & iext::ID_MASK == $id)
            .map(|i| $x.remove(i))
    };
}

/*************************************/
/*              OPEN                 */
/*************************************/
#[async_trait]
impl<'a> OpenFsm for &'a AuthFsm<'a> {
    type Error = ZError;

    type SendInitSynIn = &'a StateOpen;
    type SendInitSynOut = Option<init::ext::Auth>;
    async fn send_init_syn(
        self,
        state: Self::SendInitSynIn,
    ) -> Result<Self::SendInitSynOut, Self::Error> {
        const S: &str = "Auth extension - Send InitSyn.";

        let mut exts: Vec<ZExtUnknown> = vec![];

        #[cfg(feature = "auth_pubkey")]
        {
            match (self.pubkey.as_ref(), state.pubkey.as_ref()) {
                (Some(e), Some(s)) => {
                    if let Some(e) = e.send_init_syn(s).await?.take() {
                        exts.push(e.into())
                    }
                }
                (None, None) => {}
                _ => bail!("{S} Invalid PubKey configuration."),
            }
        }

        #[cfg(feature = "auth_usrpwd")]
        {
            match (self.usrpwd.as_ref(), state.usrpwd.as_ref()) {
                (Some(e), Some(s)) => {
                    if let Some(e) = e.send_init_syn(s).await?.take() {
                        exts.push(e.into())
                    }
                }
                (None, None) => {}
                _ => bail!("{S} Invalid UsrPwd configuration."),
            }
        }

        let codec = Zenoh080::new();
        let mut buff = vec![];
        let mut writer = buff.writer();
        codec
            .write(&mut writer, exts.as_slice())
            .map_err(|_| zerror!("{S} Encoding error."))?;

        let output = (!buff.is_empty()).then_some(init::ext::Auth::new(buff.into()));
        Ok(output)
    }

    type RecvInitAckIn = (&'a mut StateOpen, Option<init::ext::Auth>);
    type RecvInitAckOut = ();
    async fn recv_init_ack(
        self,
        input: Self::RecvInitAckIn,
    ) -> Result<Self::RecvInitAckOut, Self::Error> {
        const S: &str = "Auth extension - Recv InitAck.";

        let (state, ext) = input;
        let ext = ext.unwrap_or(init::ext::Auth::new(ZBuf::empty()));

        let codec = Zenoh080::new();
        let mut reader = ext.value.reader();
        let mut exts: Vec<ZExtUnknown> = codec
            .read(&mut reader)
            .map_err(|_| zerror!("{S} Decoding error."))?;

        #[cfg(feature = "auth_pubkey")]
        {
            match (self.pubkey.as_ref(), state.pubkey.as_mut()) {
                (Some(e), Some(s)) => {
                    let x = ztake!(exts, id::PUBKEY);
                    e.recv_init_ack((s, ztryinto!(x, S))).await?;
                }
                (None, None) => {}
                _ => bail!("{S} Invalid PubKey configuration."),
            }
        }

        #[cfg(feature = "auth_usrpwd")]
        {
            match (self.usrpwd.as_ref(), state.usrpwd.as_mut()) {
                (Some(e), Some(s)) => {
                    let x = ztake!(exts, id::USRPWD);
                    e.recv_init_ack((s, ztryinto!(x, S))).await?;
                }
                (None, None) => {}
                _ => bail!("{S} Invalid UsrPwd configuration."),
            }
        }

        Ok(())
    }

    type SendOpenSynIn = &'a StateOpen;
    type SendOpenSynOut = Option<open::ext::Auth>;
    async fn send_open_syn(
        self,
        state: Self::SendOpenSynIn,
    ) -> Result<Self::SendOpenSynOut, Self::Error> {
        const S: &str = "Auth extension - Send OpenSyn.";

        let mut exts: Vec<ZExtUnknown> = vec![];

        #[cfg(feature = "auth_pubkey")]
        {
            match (self.pubkey.as_ref(), state.pubkey.as_ref()) {
                (Some(e), Some(s)) => {
                    if let Some(e) = e.send_open_syn(s).await?.take() {
                        exts.push(e.into())
                    }
                }
                (None, None) => {}
                _ => bail!("{S} Invalid PubKey configuration."),
            }
        }

        #[cfg(feature = "auth_usrpwd")]
        {
            match (self.usrpwd.as_ref(), state.usrpwd.as_ref()) {
                (Some(e), Some(s)) => {
                    if let Some(e) = e.send_open_syn(s).await?.take() {
                        exts.push(e.into())
                    }
                }
                (None, None) => {}
                _ => bail!("{S} Invalid UsrPwd configuration."),
            }
        }

        let codec = Zenoh080::new();
        let mut buff = vec![];
        let mut writer = buff.writer();
        codec
            .write(&mut writer, exts.as_slice())
            .map_err(|_| zerror!("{S} Encoding error."))?;

        let output = (!buff.is_empty()).then_some(open::ext::Auth::new(buff.into()));
        Ok(output)
    }

    type RecvOpenAckIn = (&'a mut StateOpen, Option<open::ext::Auth>);
    type RecvOpenAckOut = ();
    async fn recv_open_ack(
        self,
        input: Self::RecvOpenAckIn,
    ) -> Result<Self::RecvOpenAckOut, Self::Error> {
        const S: &str = "Auth extension - Recv OpenAck.";

        let (state, ext) = input;
        let ext = ext.unwrap_or(init::ext::Auth::new(ZBuf::empty()));

        let codec = Zenoh080::new();
        let mut reader = ext.value.reader();
        let mut exts: Vec<ZExtUnknown> = codec
            .read(&mut reader)
            .map_err(|_| zerror!("{S} Decoding error."))?;

        #[cfg(feature = "auth_pubkey")]
        {
            match (self.pubkey.as_ref(), state.pubkey.as_mut()) {
                (Some(e), Some(s)) => {
                    let x = ztake!(exts, id::PUBKEY);
                    e.recv_open_ack((s, ztryinto!(x, S))).await?;
                }
                (None, None) => {}
                _ => bail!("{S} Invalid PubKey configuration."),
            }
        }

        #[cfg(feature = "auth_usrpwd")]
        {
            match (self.usrpwd.as_ref(), state.usrpwd.as_mut()) {
                (Some(e), Some(s)) => {
                    let x = ztake!(exts, id::USRPWD);
                    e.recv_open_ack((s, ztryinto!(x, S))).await?;
                }
                (None, None) => {}
                _ => bail!("{S} Invalid UsrPwd configuration."),
            }
        }

        Ok(())
    }
}

/*************************************/
/*            ACCEPT                 */
/*************************************/
pub(crate) struct RecvOpenSynOut {
    #[cfg(feature = "auth_usrpwd")]
    pub(crate) auth_id: UsrPwdId,
}

#[async_trait]
impl<'a> AcceptFsm for &'a AuthFsm<'a> {
    type Error = ZError;

    type RecvInitSynIn = (&'a mut StateAccept, Option<init::ext::Auth>);
    type RecvInitSynOut = ();
    async fn recv_init_syn(
        self,
        input: Self::RecvInitSynIn,
    ) -> Result<Self::RecvInitSynOut, Self::Error> {
        const S: &str = "Auth extension - Recv InitSyn.";

        let (state, ext) = input;
        let ext = ext.unwrap_or(init::ext::Auth::new(ZBuf::empty()));

        let codec = Zenoh080::new();
        let mut reader = ext.value.reader();
        let mut exts: Vec<ZExtUnknown> = codec
            .read(&mut reader)
            .map_err(|_| zerror!("{S} Decoding error."))?;

        #[cfg(feature = "auth_pubkey")]
        {
            match (self.pubkey.as_ref(), state.pubkey.as_mut()) {
                (Some(e), Some(s)) => {
                    let x = ztake!(exts, id::PUBKEY);
                    e.recv_init_syn((s, ztryinto!(x, S))).await?;
                }
                (None, None) => {}
                _ => bail!("{S} Invalid PubKey configuration."),
            }
        }

        #[cfg(feature = "auth_usrpwd")]
        {
            match (self.usrpwd.as_ref(), state.usrpwd.as_mut()) {
                (Some(e), Some(s)) => {
                    let x = ztake!(exts, id::USRPWD);
                    e.recv_init_syn((s, ztryinto!(x, S))).await?;
                }
                (None, None) => {}
                _ => bail!("{S} Invalid UsrPwd configuration."),
            }
        }

        Ok(())
    }

    type SendInitAckIn = &'a StateAccept;
    type SendInitAckOut = Option<init::ext::Auth>;
    async fn send_init_ack(
        self,
        state: Self::SendInitAckIn,
    ) -> Result<Self::SendInitAckOut, Self::Error> {
        const S: &str = "Auth extension - Send InitAck.";

        let mut exts: Vec<ZExtUnknown> = vec![];

        #[cfg(feature = "auth_pubkey")]
        {
            match (self.pubkey.as_ref(), state.pubkey.as_ref()) {
                (Some(e), Some(s)) => {
                    if let Some(e) = e.send_init_ack(s).await?.take() {
                        exts.push(e.into())
                    }
                }
                (None, None) => {}
                _ => bail!("{S} Invalid PubKey configuration."),
            }
        }

        #[cfg(feature = "auth_usrpwd")]
        {
            match (self.usrpwd.as_ref(), state.usrpwd.as_ref()) {
                (Some(e), Some(s)) => {
                    if let Some(e) = e.send_init_ack(s).await?.take() {
                        exts.push(e.into())
                    }
                }
                (None, None) => {}
                _ => bail!("{S} Invalid UsrPwd configuration."),
            }
        }

        let codec = Zenoh080::new();
        let mut buff = vec![];
        let mut writer = buff.writer();
        codec
            .write(&mut writer, exts.as_slice())
            .map_err(|_| zerror!("{S} Encoding error."))?;

        let output = (!buff.is_empty()).then_some(init::ext::Auth::new(buff.into()));
        Ok(output)
    }

    type RecvOpenSynIn = (&'a mut StateAccept, Option<open::ext::Auth>);

    type RecvOpenSynOut = RecvOpenSynOut;

    async fn recv_open_syn(
        self,
        input: Self::RecvOpenSynIn,
    ) -> Result<Self::RecvOpenSynOut, Self::Error> {
        const S: &str = "Auth extension - Recv OpenSyn.";

        let (state, ext) = input;
        let ext = ext.unwrap_or(init::ext::Auth::new(ZBuf::empty()));

        let codec = Zenoh080::new();
        let mut reader = ext.value.reader();
        let mut exts: Vec<ZExtUnknown> = codec
            .read(&mut reader)
            .map_err(|_| zerror!("{S} Decoding error."))?;

        #[cfg(feature = "auth_pubkey")]
        {
            match (self.pubkey.as_ref(), state.pubkey.as_mut()) {
                (Some(e), Some(s)) => {
                    let x = ztake!(exts, id::PUBKEY);
                    e.recv_open_syn((s, ztryinto!(x, S))).await?;
                }
                (None, None) => {}
                _ => bail!("{S} Invalid PubKey configuration."),
            }
        }

        #[cfg(feature = "auth_usrpwd")]
        let auth_id: UsrPwdId;

        #[cfg(feature = "auth_usrpwd")]
        {
            match (self.usrpwd.as_ref(), state.usrpwd.as_mut()) {
                (Some(e), Some(s)) => {
                    let x = ztake!(exts, id::USRPWD);
                    let username = e.recv_open_syn((s, ztryinto!(x, S))).await?;
                    auth_id = UsrPwdId(Some(username));
                }
                (None, None) => {
                    auth_id = UsrPwdId(None);
                }
                _ => bail!("{S} Invalid UsrPwd configuration."),
            }
        }
        Ok(RecvOpenSynOut {
            #[cfg(feature = "auth_usrpwd")]
            auth_id,
        })
    }

    type SendOpenAckIn = &'a StateAccept;
    type SendOpenAckOut = Option<open::ext::Auth>;
    async fn send_open_ack(
        self,
        state: Self::SendOpenAckIn,
    ) -> Result<Self::SendOpenAckOut, Self::Error> {
        const S: &str = "Auth extension - Send OpenAck.";

        let mut exts: Vec<ZExtUnknown> = vec![];

        #[cfg(feature = "auth_pubkey")]
        {
            match (self.pubkey.as_ref(), state.pubkey.as_ref()) {
                (Some(e), Some(s)) => {
                    if let Some(e) = e.send_open_ack(s).await?.take() {
                        exts.push(e.into())
                    }
                }
                (None, None) => {}
                _ => bail!("{S} Invalid PubKey configuration."),
            }
        }

        #[cfg(feature = "auth_usrpwd")]
        {
            match (self.usrpwd.as_ref(), state.usrpwd.as_ref()) {
                (Some(e), Some(s)) => {
                    if let Some(e) = e.send_open_ack(s).await?.take() {
                        exts.push(e.into())
                    }
                }
                (None, None) => {}
                _ => bail!("{S} Invalid UsrPwd configuration."),
            }
        }

        let codec = Zenoh080::new();
        let mut buff = vec![];
        let mut writer = buff.writer();
        codec
            .write(&mut writer, exts.as_slice())
            .map_err(|_| zerror!("{S} Encoding error."))?;

        let output = (!buff.is_empty()).then_some(open::ext::Auth::new(buff.into()));
        Ok(output)
    }
}
