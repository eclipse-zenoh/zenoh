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
use std::{collections::HashMap, fmt};

use async_trait::async_trait;
use rand::{CryptoRng, Rng};
use tokio::sync::RwLock;
use zenoh_buffers::{
    reader::{DidntRead, HasReader, Reader},
    writer::{DidntWrite, HasWriter, Writer},
};
use zenoh_codec::{RCodec, WCodec, Zenoh080};
use zenoh_config::UsrPwdConf;
use zenoh_core::{bail, zasyncread, zerror, Error as ZError, Result as ZResult};
use zenoh_crypto::hmac;
use zenoh_protocol::common::{ZExtUnit, ZExtZ64, ZExtZBuf};

use crate::unicast::establishment::{ext::auth::id, AcceptFsm, OpenFsm};

mod ext {
    use zenoh_protocol::{zextunit, zextz64, zextzbuf};

    use super::{id::USRPWD, ZExtUnit, ZExtZ64, ZExtZBuf};

    pub(super) type InitSyn = zextunit!(USRPWD, false);
    pub(super) type InitAck = zextz64!(USRPWD, false);
    pub(super) type OpenSyn = zextzbuf!(USRPWD, false);
    pub(super) type OpenAck = zextunit!(USRPWD, false);
}

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
            let content = tokio::fs::read_to_string(dict)
                .await
                .map_err(|e| zerror!("{S} Invalid user-password dictionary file: {}.", e))?;

            // Populate the user-password dictionary
            // The config file is expected to be in the form of:
            //      usr1:pwd1
            //      usr2:pwd2
            //      usr3:pwd3
            // I.e.: one <user>:<password> entry per line
            for l in content.lines() {
                let line = l.trim();
                if line.is_empty() {
                    continue;
                }
                let idx = line.find(':').ok_or_else(|| {
                    zerror!("{S} Invalid user-password dictionary file: invalid format.")
                })?;
                let user = line[..idx].trim().as_bytes().to_owned();
                if user.is_empty() {
                    bail!("{S} Invalid user-password dictionary file: empty user.")
                }
                let password = line[idx + 1..].trim().as_bytes().to_owned();
                if password.is_empty() {
                    bail!("{S} Invalid user-password dictionary file: empty password.")
                }
                lookup.insert(user, password);
            }
            tracing::debug!("{S} User-password dictionary has been configured.");
        }

        let mut credentials: Option<(User, Password)> = None;
        if let Some(user) = config.user() {
            if let Some(password) = config.password() {
                tracing::debug!("{S} User-password has been configured.");
                credentials = Some((user.as_bytes().to_owned(), password.as_bytes().to_owned()));
            }
        }

        if !lookup.is_empty() || credentials.is_some() {
            tracing::debug!("{S} User-password authentication is enabled.");
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
                "User: '{}', Password: '***', ",
                String::from_utf8_lossy(&c.0)
            )?,
            None => write!(f, "User: '', Password: '', ")?,
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
    nonce: u64,
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
    nonce: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct UsrPwdId(pub Option<Vec<u8>>);

impl StateAccept {
    pub(crate) fn new<R>(prng: &mut R) -> Self
    where
        R: Rng + CryptoRng,
    {
        Self { nonce: prng.gen() }
    }

    #[cfg(all(test, feature = "test"))]
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
        let nonce: u64 = self.read(&mut *reader)?;
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
/*             OpenSyn               */
/*************************************/
/// ```text
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// ~     user      ~
/// +---------------+
/// ~     hash      ~
/// +---------------+
///
/// ZExtZBuf
/// ```
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
/// ```text
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// +---------------+
///
/// ZExtUnit
/// ```

#[async_trait]
impl<'a> OpenFsm for &'a AuthUsrPwdFsm<'a> {
    type Error = ZError;

    type SendInitSynIn = &'a StateOpen;
    type SendInitSynOut = Option<ext::InitSyn>;
    async fn send_init_syn(
        self,
        _input: Self::SendInitSynIn,
    ) -> Result<Self::SendInitSynOut, Self::Error> {
        let output = zasyncread!(self.inner)
            .credentials
            .is_some()
            .then_some(ZExtUnit::new());
        Ok(output)
    }

    type RecvInitAckIn = (&'a mut StateOpen, Option<ext::InitAck>);
    type RecvInitAckOut = ();
    async fn recv_init_ack(
        self,
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
    type SendOpenSynOut = Option<ext::OpenSyn>;
    async fn send_open_syn(
        self,
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

    type RecvOpenAckIn = (&'a mut StateOpen, Option<ext::OpenAck>);
    type RecvOpenAckOut = ();
    async fn recv_open_ack(
        self,
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
impl<'a> AcceptFsm for &'a AuthUsrPwdFsm<'a> {
    type Error = ZError;

    type RecvInitSynIn = (&'a mut StateAccept, Option<ext::InitSyn>);
    type RecvInitSynOut = ();
    async fn recv_init_syn(
        self,
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
    type SendInitAckOut = Option<ext::InitAck>;
    async fn send_init_ack(
        self,
        state: Self::SendInitAckIn,
    ) -> Result<Self::SendInitAckOut, Self::Error> {
        Ok(Some(ZExtZ64::new(state.nonce)))
    }

    type RecvOpenSynIn = (&'a mut StateAccept, Option<ext::OpenSyn>);
    type RecvOpenSynOut = Vec<u8>; //value of userid is returned if recvopensynout is processed as valid
    async fn recv_open_syn(
        self,
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
        let username = open_syn.user.to_owned();
        Ok(username)
    }

    type SendOpenAckIn = &'a StateAccept;
    type SendOpenAckOut = Option<ext::OpenAck>;
    async fn send_open_ack(
        self,
        _input: Self::SendOpenAckIn,
    ) -> Result<Self::SendOpenAckOut, Self::Error> {
        Ok(Some(ZExtUnit::new()))
    }
}

mod tests {
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn authenticator_usrpwd_config() {
        async fn inner() {
            use std::{fs::File, io::Write};

            use zenoh_config::UsrPwdConf;

            use super::AuthUsrPwd;

            /* [CONFIG] */
            let f1 = "zenoh-test-auth-usrpwd.txt";

            let mut config = UsrPwdConf::default();
            config.set_user(Some("usr1".to_owned())).unwrap();
            config.set_password(Some("pwd1".to_owned())).unwrap();
            config.set_dictionary_file(Some(f1.to_owned())).unwrap();

            macro_rules! zconfig {
                () => {
                    File::options()
                        .create(true)
                        .write(true)
                        .truncate(true)
                        .open(f1)
                        .unwrap()
                };
            }
            // Valid config
            let mut c = zconfig!();
            writeln!(c, "usr1:pwd1").unwrap();
            drop(c);
            assert!(AuthUsrPwd::from_config(&config).await.unwrap().is_some());
            // Invalid config
            let mut c = zconfig!();
            writeln!(c, "usr1").unwrap();
            drop(c);
            assert!(AuthUsrPwd::from_config(&config).await.is_err());
            // Empty password
            let mut c = zconfig!();
            writeln!(c, "usr1:").unwrap();
            drop(c);
            assert!(AuthUsrPwd::from_config(&config).await.is_err());
            // Empty user
            let mut c = zconfig!();
            writeln!(c, ":pwd1").unwrap();
            drop(c);
            assert!(AuthUsrPwd::from_config(&config).await.is_err());
            // Empty user and password
            let mut c = zconfig!();
            writeln!(c, ":").unwrap();
            drop(c);
            assert!(AuthUsrPwd::from_config(&config).await.is_err());

            let _ = std::fs::remove_file(f1);
        }

        inner().await;
    }
}
