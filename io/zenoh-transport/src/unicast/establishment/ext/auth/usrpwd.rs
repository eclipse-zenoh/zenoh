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
use crate::establishment::{AcceptFsm, OpenFsm};
use async_std::{fs, sync::RwLock};
use async_trait::async_trait;
use rand::{CryptoRng, Rng};
use std::{collections::HashMap, fmt};
use zenoh_buffers::{
    reader::{DidntRead, HasReader, Reader},
    writer::{DidntWrite, HasWriter, Writer},
};
use zenoh_codec::{RCodec, WCodec, Zenoh080};
use zenoh_collections::Properties;
use zenoh_config::UsrPwdConf;
use zenoh_core::{bail, zasyncread, zerror, Error as ZError, Result as ZResult};
use zenoh_crypto::hmac;
use zenoh_protocol::{
    common::{ZExtUnit, ZExtZBuf, ZExtZInt},
    core::ZInt,
};

// Authenticator
type User = Vec<u8>;
type Password = Vec<u8>;

pub struct AuthUsrPwd {
    lookup: HashMap<User, Password>,
    credentials: Option<(User, Password)>,
}

impl AuthUsrPwd {
    pub fn new(credentials: Option<(User, Password)>) -> Self {
        Self {
            lookup: HashMap::new(),
            credentials,
        }
    }

    pub async fn add_user(&mut self, user: User, password: Password) -> ZResult<()> {
        self.lookup.insert(user, password);
        Ok(())
    }

    pub async fn del_user(&mut self, user: &User) -> ZResult<()> {
        self.lookup.remove(user);
        Ok(())
    }

    pub async fn from_config(config: &UsrPwdConf) -> ZResult<Option<Self>> {
        const S: &str = "UsrPwd extension - From config.";

        let mut lookup: HashMap<User, Password> = HashMap::new();
        if let Some(dict) = config.dictionary_file() {
            let content = fs::read_to_string(dict)
                .await
                .map_err(|e| zerror!("{S} Invalid user-password dictionary file: {}.", e))?;

            // Populate the user-password dictionary
            let mut ps = Properties::from(content);
            for (user, password) in ps.drain() {
                lookup.insert(user.as_bytes().to_owned(), password.as_bytes().to_owned());
            }
            log::debug!("{S} User-password dictionary has been configured.");
        }

        let mut credentials: Option<(User, Password)> = None;
        if let Some(user) = config.user() {
            if let Some(password) = config.password() {
                log::debug!("{S} User-password has been configured.");
                credentials = Some((user.as_bytes().to_owned(), password.as_bytes().to_owned()));
            }
        }

        if !lookup.is_empty() || credentials.is_some() {
            log::debug!("{S} User-password authentication is enabled.");
            Ok(Some(Self {
                lookup,
                credentials,
            }))
        } else {
            Ok(None)
        }
    }
}

impl fmt::Debug for AuthUsrPwd {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.credentials.as_ref() {
            Some(c) => write!(
                f,
                "User: '{}', Password: '***'",
                String::from_utf8_lossy(&c.0)
            )?,
            None => write!(f, "User: '', Password: ''")?,
        }
        write!(f, "Dictionary: {{")?;
        for (i, (u, _)) in self.lookup.iter().enumerate() {
            if i != 0 {
                write!(f, ",")?;
            }
            write!(f, " {}", String::from_utf8_lossy(u))?;
        }
        write!(f, " }}")
    }
}

// OpenFsm / AcceptFsm
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct StateOpen {
    nonce: ZInt,
}

impl StateOpen {
    pub(crate) fn new<R>(prng: &mut R) -> Self
    where
        R: Rng + CryptoRng,
    {
        Self { nonce: prng.gen() }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct StateAccept {
    nonce: ZInt,
}

impl StateAccept {
    pub(crate) fn new<R>(prng: &mut R) -> Self
    where
        R: Rng + CryptoRng,
    {
        Self { nonce: prng.gen() }
    }

    #[cfg(feature = "test")]
    #[allow(unused)] // Used for testing
    pub(crate) fn rand() -> Self {
        let mut rng = rand::thread_rng();
        Self::new(&mut rng)
    }
}

// Codec
impl<W> WCodec<&StateAccept, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &StateAccept) -> Self::Output {
        self.write(&mut *writer, x.nonce)
    }
}

impl<R> RCodec<StateAccept, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<StateAccept, Self::Error> {
        let nonce: ZInt = self.read(&mut *reader)?;
        Ok(StateAccept { nonce })
    }
}

pub(crate) struct AuthUsrPwdFsm<'a> {
    inner: &'a RwLock<AuthUsrPwd>,
}

impl<'a> AuthUsrPwdFsm<'a> {
    pub(super) const fn new(inner: &'a RwLock<AuthUsrPwd>) -> Self {
        Self { inner }
    }
}

/*************************************/
/*             InitSyn               */
/*************************************/
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// +---------------+
///
/// ZExtUnit

/*************************************/
/*             InitAck               */
/*************************************/
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// ~     nonce     ~
/// +---------------+
///
/// ZExtZInt

/*************************************/
/*             OpenSyn               */
/*************************************/
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// ~     user      ~
/// +---------------+
/// ~     hash      ~
/// +---------------+
///
/// ZExtZBuf
struct OpenSyn {
    user: Vec<u8>,
    hmac: Vec<u8>,
}

impl<W> WCodec<&OpenSyn, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &OpenSyn) -> Self::Output {
        self.write(&mut *writer, x.user.as_slice())?;
        self.write(&mut *writer, x.hmac.as_slice())?;
        Ok(())
    }
}

impl<R> RCodec<OpenSyn, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<OpenSyn, Self::Error> {
        let user: Vec<u8> = self.read(&mut *reader)?;
        let hmac: Vec<u8> = self.read(&mut *reader)?;
        Ok(OpenSyn { user, hmac })
    }
}

/*************************************/
/*             OpenAck               */
/*************************************/
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// +---------------+
///
/// ZExtUnit

#[async_trait]
impl<'a> OpenFsm for AuthUsrPwdFsm<'a> {
    type Error = ZError;

    type SendInitSynIn = &'a StateOpen;
    type SendInitSynOut = Option<ZExtUnit<{ super::id::USRPWD }>>;
    async fn send_init_syn(
        &self,
        _input: Self::SendInitSynIn,
    ) -> Result<Self::SendInitSynOut, Self::Error> {
        let output = zasyncread!(self.inner)
            .credentials
            .is_some()
            .then_some(ZExtUnit::new());
        Ok(output)
    }

    type RecvInitAckIn = (&'a mut StateOpen, Option<ZExtZInt<{ super::id::USRPWD }>>);
    type RecvInitAckOut = ();
    async fn recv_init_ack(
        &self,
        input: Self::RecvInitAckIn,
    ) -> Result<Self::RecvInitAckOut, Self::Error> {
        const S: &str = "UsrPwd extension - Recv InitSyn.";

        if zasyncread!(self.inner).credentials.is_none() {
            return Ok(());
        };

        let (state, mut ext_userwpd) = input;
        let ext_usrpwd = ext_userwpd
            .take()
            .ok_or_else(|| zerror!("{S} Decoding error."))?;
        state.nonce = ext_usrpwd.value;

        Ok(())
    }

    type SendOpenSynIn = &'a StateOpen;
    type SendOpenSynOut = Option<ZExtZBuf<{ super::id::USRPWD }>>;
    async fn send_open_syn(
        &self,
        state: Self::SendOpenSynIn,
    ) -> Result<Self::SendOpenSynOut, Self::Error> {
        const S: &str = "UsrPwd extension - Send OpenSyn.";

        // If credentials are not configured, don't continue the USRPWD authentication
        let r_inner = zasyncread!(self.inner);
        let (user, password) = match r_inner.credentials.as_ref() {
            Some(cr) => cr,
            None => return Ok(None),
        };

        // Create the HMAC of the password using the nonce received as a key (it's a challenge)
        let key = state.nonce.to_le_bytes();
        let hmac = hmac::sign(&key, password).map_err(|_| zerror!("{S} Encoding error."))?;
        // Create the OpenSyn extension
        let open_syn = OpenSyn {
            user: user.to_vec(),
            hmac,
        };
        drop(r_inner);

        let codec = Zenoh080::new();
        let mut buff = vec![];
        let mut writer = buff.writer();
        codec
            .write(&mut writer, &open_syn)
            .map_err(|_| zerror!("{S} Encoding error."))?;

        let output = Some(ZExtZBuf::new(buff.into()));
        Ok(output)
    }

    type RecvOpenAckIn = (&'a mut StateOpen, Option<ZExtUnit<{ super::id::USRPWD }>>);
    type RecvOpenAckOut = ();
    async fn recv_open_ack(
        &self,
        input: Self::RecvOpenAckIn,
    ) -> Result<Self::RecvOpenAckOut, Self::Error> {
        const S: &str = "UsrPwd extension - Recv OpenAck.";

        let (_, ext) = input;
        if zasyncread!(self.inner).credentials.is_some() && ext.is_none() {
            bail!("{S} Expected extension.");
        }

        Ok(())
    }
}

/*************************************/
/*            ACCEPT                 */
/*************************************/
#[async_trait]
impl<'a> AcceptFsm for AuthUsrPwdFsm<'a> {
    type Error = ZError;

    type RecvInitSynIn = (&'a mut StateAccept, Option<ZExtUnit<{ super::id::USRPWD }>>);
    type RecvInitSynOut = ();
    async fn recv_init_syn(
        &self,
        input: Self::RecvInitSynIn,
    ) -> Result<Self::RecvInitSynOut, Self::Error> {
        const S: &str = "UsrPwd extension - Recv InitSyn.";

        let (_, ext_usrpwd) = input;
        if ext_usrpwd.is_none() {
            bail!("{S} Expected extension.");
        }

        Ok(())
    }

    type SendInitAckIn = &'a StateAccept;
    type SendInitAckOut = Option<ZExtZInt<{ super::id::USRPWD }>>;
    async fn send_init_ack(
        &self,
        state: Self::SendInitAckIn,
    ) -> Result<Self::SendInitAckOut, Self::Error> {
        Ok(Some(ZExtZInt::new(state.nonce)))
    }

    type RecvOpenSynIn = (&'a mut StateAccept, Option<ZExtZBuf<{ super::id::USRPWD }>>);
    type RecvOpenSynOut = ();
    async fn recv_open_syn(
        &self,
        input: Self::RecvOpenSynIn,
    ) -> Result<Self::RecvOpenSynOut, Self::Error> {
        const S: &str = "UsrPwd extension - Recv OpenSyn.";

        let (state, mut ext_usrpwd) = input;
        let ext_usrpwd = ext_usrpwd
            .take()
            .ok_or_else(|| zerror!("{S} Expected extension."))?;

        let codec = Zenoh080::new();
        let mut reader = ext_usrpwd.value.reader();
        let open_syn: OpenSyn = codec
            .read(&mut reader)
            .map_err(|_| zerror!("{S} Decoding error."))?;

        let r_inner = zasyncread!(self.inner);
        let pwd = r_inner
            .lookup
            .get(&open_syn.user)
            .ok_or_else(|| zerror!("{S} Invalid user."))?;

        // Create the HMAC of the password using the nonce received as challenge
        let key = state.nonce.to_le_bytes();
        let hmac = hmac::sign(&key, pwd).map_err(|_| zerror!("{S} Encoding error."))?;
        if hmac != open_syn.hmac {
            bail!("{S} Invalid password.");
        }

        Ok(())
    }

    type SendOpenAckIn = &'a StateAccept;
    type SendOpenAckOut = Option<ZExtUnit<{ super::id::USRPWD }>>;
    async fn send_open_ack(
        &self,
        _input: Self::SendOpenAckIn,
    ) -> Result<Self::SendOpenAckOut, Self::Error> {
        Ok(Some(ZExtUnit::new()))
    }
}
