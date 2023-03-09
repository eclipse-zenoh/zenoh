// //
// // Copyright (c) 2022 ZettaScale Technology
// //
// // This program and the accompanying materials are made available under the
// // terms of the Eclipse Public License 2.0 which is available at
// // http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// // which is available at https://www.apache.org/licenses/LICENSE-2.0.
// //
// // SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
// //
// // Contributors:
// //   ZettaScale Zenoh Team, <zenoh@zettascale.tech>
// //
// use super::{
//     AuthenticatedLink, TransportAuthenticator, TransportAuthenticatorTrait, ZNodeAuthenticatorId,
// };
// use super::{Locator, ZInt, ZenohId};
// use crate::unicast::establishment::Cookie;
use crate::establishment::{AcceptFsm, OpenFsm};
use async_std::fs;
use async_trait::async_trait;
use rand::Rng;
use std::collections::HashMap;
use zenoh_buffers::{
    reader::{DidntRead, HasReader, Reader},
    writer::{DidntWrite, HasWriter, Writer},
};
use zenoh_codec::{RCodec, WCodec, Zenoh080};
use zenoh_collections::Properties;
use zenoh_config::UsrPwdConf;
use zenoh_core::{bail, zerror, Error as ZError, Result as ZResult};
use zenoh_crypto::hmac;
use zenoh_protocol::{
    common::{ZExtUnit, ZExtZBuf, ZExtZInt},
    core::ZInt,
};

// Authenticator
type User = Vec<u8>;
type Password = Vec<u8>;

#[derive(Debug)]
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
        let mut lookup: HashMap<User, Password> = HashMap::new();
        if let Some(dict) = config.dictionary_file() {
            let content = fs::read_to_string(dict)
                .await
                .map_err(|e| zerror!("Invalid user-password dictionary file: {}", e))?;

            // Populate the user-password dictionary
            let mut ps = Properties::from(content);
            for (user, password) in ps.drain() {
                lookup.insert(user.as_bytes().to_owned(), password.as_bytes().to_owned());
            }
            log::debug!("User-password dictionary has been configured");
        }

        let mut credentials: Option<(User, Password)> = None;
        if let Some(user) = config.user() {
            if let Some(password) = config.password() {
                log::debug!("User and password have been configured");
                credentials = Some((user.as_bytes().to_owned(), password.as_bytes().to_owned()));
            }
        }

        if !lookup.is_empty() || credentials.is_some() {
            log::debug!("User-password authentication is enabled");
            Ok(Some(Self {
                lookup,
                credentials,
            }))
        } else {
            Ok(None)
        }
    }
}

// OpenFsm / AcceptFsm
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct State {
    nonce: ZInt,
}

impl State {
    pub(crate) fn new<R>(prng: &mut R) -> Self
    where
        R: Rng,
    {
        Self { nonce: prng.gen() }
    }

    #[cfg(test)]
    pub(crate) fn rand() -> Self {
        let mut rng = rand::thread_rng();
        Self::new(&mut rng)
    }
}

// Codec
impl<W> WCodec<&State, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &State) -> Self::Output {
        self.write(&mut *writer, x.nonce)
    }
}

impl<R> RCodec<State, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<State, Self::Error> {
        let nonce: ZInt = self.read(&mut *reader)?;
        Ok(State { nonce })
    }
}

pub(crate) struct AuthUsrPwdFsm<'a> {
    inner: &'a AuthUsrPwd,
}

impl<'a> AuthUsrPwdFsm<'a> {
    pub(super) const fn new(a: &'a AuthUsrPwd) -> Self {
        Self { inner: a }
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

    type SendInitSynIn = &'a State;
    type SendInitSynOut = Option<ZExtUnit<{ super::id::USRPWD }>>;
    async fn send_init_syn(
        &self,
        _input: Self::SendInitSynIn,
    ) -> Result<Self::SendInitSynOut, Self::Error> {
        let output = self.inner.credentials.is_some().then_some(ZExtUnit::new());
        Ok(output)
    }

    type RecvInitAckIn = (&'a mut State, Option<ZExtZInt<{ super::id::USRPWD }>>);
    type RecvInitAckOut = ();
    async fn recv_init_ack(
        &self,
        input: Self::RecvInitAckIn,
    ) -> Result<Self::RecvInitAckOut, Self::Error> {
        const S: &str = "UsrPwd extension - Recv InitSyn.";

        if self.inner.credentials.is_none() {
            return Ok(());
        };

        let (state, mut ext_userwpd) = input;
        let ext_usrpwd = ext_userwpd
            .take()
            .ok_or_else(|| zerror!("{} Decoding error.", S))?;
        state.nonce = ext_usrpwd.value;

        Ok(())
    }

    type SendOpenSynIn = &'a State;
    type SendOpenSynOut = Option<ZExtZBuf<{ super::id::USRPWD }>>;
    async fn send_open_syn(
        &self,
        state: Self::SendOpenSynIn,
    ) -> Result<Self::SendOpenSynOut, Self::Error> {
        const S: &str = "UsrPwd extension - Send OpenSyn.";

        // If credentials are not configured, don't continue the USRPWD authentication
        let (user, password) = match self.inner.credentials.as_ref() {
            Some(cr) => cr,
            None => return Ok(None),
        };

        // Create the HMAC of the password using the nonce received as a key (it's a challenge)
        let key = state.nonce.to_le_bytes();
        let hmac = hmac::sign(&key, password).map_err(|_| zerror!("{} Encoding error.", S))?;
        // Create the OpenSyn extension
        let open_syn = OpenSyn {
            user: user.to_vec(),
            hmac,
        };
        let codec = Zenoh080::new();
        let mut buff = vec![];
        let mut writer = buff.writer();
        codec
            .write(&mut writer, &open_syn)
            .map_err(|_| zerror!("{} Encoding error.", S))?;

        let output = Some(ZExtZBuf::new(buff.into()));
        Ok(output)
    }

    type RecvOpenAckIn = (&'a mut State, Option<ZExtUnit<{ super::id::USRPWD }>>);
    type RecvOpenAckOut = ();
    async fn recv_open_ack(
        &self,
        input: Self::RecvOpenAckIn,
    ) -> Result<Self::RecvOpenAckOut, Self::Error> {
        const S: &str = "UsrPwd extension - Recv OpenAck.";

        let (_, ext) = input;
        if self.inner.credentials.is_some() && ext.is_none() {
            bail!("{} Expected extension.", S);
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

    type RecvInitSynIn = (&'a mut State, Option<ZExtUnit<{ super::id::USRPWD }>>);
    type RecvInitSynOut = ();
    async fn recv_init_syn(
        &self,
        input: Self::RecvInitSynIn,
    ) -> Result<Self::RecvInitSynOut, Self::Error> {
        const S: &str = "UsrPwd extension - Recv InitSyn.";

        let (_, ext_usrpwd) = input;
        if ext_usrpwd.is_none() {
            bail!("{} Expected extension.", S);
        }

        Ok(())
    }

    type SendInitAckIn = &'a State;
    type SendInitAckOut = Option<ZExtZInt<{ super::id::USRPWD }>>;
    async fn send_init_ack(
        &self,
        state: Self::SendInitAckIn,
    ) -> Result<Self::SendInitAckOut, Self::Error> {
        Ok(Some(ZExtZInt::new(state.nonce)))
    }

    type RecvOpenSynIn = (&'a mut State, Option<ZExtZBuf<{ super::id::USRPWD }>>);
    type RecvOpenSynOut = ();
    async fn recv_open_syn(
        &self,
        input: Self::RecvOpenSynIn,
    ) -> Result<Self::RecvOpenSynOut, Self::Error> {
        const S: &str = "UsrPwd extension - Recv OpenSyn.";

        let (state, mut ext_usrpwd) = input;
        let ext_usrpwd = ext_usrpwd
            .take()
            .ok_or_else(|| zerror!("{} Expected extension.", S))?;

        let codec = Zenoh080::new();
        let mut reader = ext_usrpwd.value.reader();
        let open_syn: OpenSyn = codec
            .read(&mut reader)
            .map_err(|_| zerror!("{} Decoding error.", S))?;

        let pwd = self
            .inner
            .lookup
            .get(&open_syn.user)
            .ok_or_else(|| zerror!("{} Invalid user.", S))?;

        // Create the HMAC of the password using the nonce received as challenge
        let key = state.nonce.to_le_bytes();
        let hmac = hmac::sign(&key, pwd).map_err(|_| zerror!("{} Encoding error.", S))?;
        if hmac != open_syn.hmac {
            bail!("{} Invalid password.", S);
        }

        Ok(())
    }

    type SendOpenAckIn = &'a State;
    type SendOpenAckOut = Option<ZExtUnit<{ super::id::USRPWD }>>;
    async fn send_open_ack(
        &self,
        _input: Self::SendOpenAckIn,
    ) -> Result<Self::SendOpenAckOut, Self::Error> {
        Ok(Some(ZExtUnit::new()))
    }
}

// #[async_trait]
// impl TransportAuthenticatorTrait for UserPasswordAuthenticator {
//     fn id(&self) -> ZNodeAuthenticatorId {
//         ZNodeAuthenticatorId::UserPassword
//     }

//     async fn close(&self) {
//         // No cleanup needed
//     }

//     async fn get_init_syn_properties(
//         &self,
//         link: &AuthenticatedLink,
//         _node_id: &ZenohId,
//     ) -> ZResult<Option<Vec<u8>>> {
//         // If credentials are not configured, don't initiate the USRPWD authentication
//         if self.credentials.is_none() {
//             return Ok(None);
//         }

//         let init_syn_property = InitSynProperty {
//             version: USRPWD_VERSION,
//         };
//         let mut wbuf = vec![];
//         let codec = Zenoh080::new();
//         let mut writer = wbuf.writer();
//         codec
//             .write(&mut writer, &init_syn_property)
//             .map_err(|_| zerror!("Error in encoding InitSyn for UsrPwd on link: {}", link))?;
//         let attachment = wbuf;

//         Ok(Some(attachment))
//     }

//     async fn handle_init_syn(
//         &self,
//         link: &AuthenticatedLink,
//         cookie: &Cookie,
//         property: Option<Vec<u8>>,
//     ) -> ZResult<(Option<Vec<u8>>, Option<Vec<u8>>)> {
//         let p = property.ok_or_else(|| {
//             zerror!(
//                 "Received InitSyn with no UsrPwd attachment on link: {}",
//                 link
//             )
//         })?;

//         let codec = Zenoh080::new();

//         let mut reader = p.reader();
//         let init_syn_property: InitSynProperty = codec.read(&mut reader).map_err(|_| {
//             zerror!(
//                 "Received InitSyn with invalid UsrPwd attachment on link: {}",
//                 link
//             )
//         })?;
//         if init_syn_property.version > USRPWD_VERSION {
//             bail!("Rejected InitSyn with invalid attachment on link: {}", link)
//         }

//         // Create the InitAck attachment
//         let init_ack_property = InitAckProperty {
//             nonce: cookie.nonce,
//         };
//         let mut wbuf = vec![];
//         let mut writer = wbuf.writer();
//         codec
//             .write(&mut writer, &init_ack_property)
//             .map_err(|_| zerror!("Error in encoding InitAck for UsrPwd on link: {}", link))?;
//         let attachment = wbuf;

//         Ok((Some(attachment), None))
//     }

//     async fn handle_init_ack(
//         &self,
//         link: &AuthenticatedLink,
//         _node_id: &ZenohId,
//         _sn_resolution: ZInt,
//         property: Option<Vec<u8>>,
//     ) -> ZResult<Option<Vec<u8>>> {
//         // If credentials are not configured, don't continue the USRPWD authentication
//         let credentials = match self.credentials.as_ref() {
//             Some(cr) => cr,
//             None => return Ok(None),
//         };

//         let p = property.ok_or_else(|| {
//             zerror!(
//                 "Received InitAck with no UsrPwd attachment on link: {}",
//                 link
//             )
//         })?;

//         let codec = Zenoh080::new();

//         let mut reader = p.reader();
//         let init_ack_property: InitAckProperty = codec.read(&mut reader).map_err(|_| {
//             zerror!(
//                 "Received InitAck with invalid UsrPwd attachment on link: {}",
//                 link
//             )
//         })?;

//         // Create the HMAC of the password using the nonce received as a key (it's a challenge)
//         let key = init_ack_property.nonce.to_le_bytes();
//         let hmac = hmac::sign(&key, &credentials.password)?;
//         // Create the OpenSyn attachment
//         let open_syn_property = OpenSynProperty {
//             user: credentials.user.clone(),
//             hmac,
//         };
//         // Encode the InitAck attachment
//         let mut wbuf = vec![];
//         let mut writer = wbuf.writer();
//         codec
//             .write(&mut writer, &open_syn_property)
//             .map_err(|_| zerror!("Error in encoding OpenSyn for UsrPwd on link: {}", link))?;
//         let attachment = wbuf;

//         Ok(Some(attachment))
//     }

//     async fn handle_open_syn(
//         &self,
//         link: &AuthenticatedLink,
//         cookie: &Cookie,
//         property: (Option<Vec<u8>>, Option<Vec<u8>>),
//     ) -> ZResult<Option<Vec<u8>>> {
//         let (attachment, _cookie) = property;
//         let a = attachment.ok_or_else(|| {
//             zerror!(
//                 "Received OpenSyn with no UsrPwd attachment on link: {}",
//                 link
//             )
//         })?;

//         let codec = Zenoh080::new();

//         let mut reader = a.reader();
//         let open_syn_property: OpenSynProperty = codec.read(&mut reader).map_err(|_| {
//             zerror!(
//                 "Received OpenSyn with invalid UsrPwd attachment on link: {}",
//                 link
//             )
//         })?;
//         let password = match zasyncread!(self.lookup).get(&open_syn_property.user) {
//             Some(password) => password.clone(),
//             None => bail!("Received OpenSyn with invalid user on link: {}", link),
//         };

//         // Create the HMAC of the password using the nonce received as challenge
//         let key = cookie.nonce.to_le_bytes();
//         let hmac = hmac::sign(&key, &password)?;
//         if hmac != open_syn_property.hmac {
//             bail!("Received OpenSyn with invalid password on link: {}", link)
//         }

//         // Check PID validity
//         let mut guard = zasynclock!(self.authenticated);
//         match guard.get_mut(&cookie.zid) {
//             Some(auth) => {
//                 if open_syn_property.user != auth.credentials.user
//                     || password != auth.credentials.password
//                 {
//                     bail!("Received OpenSyn with invalid password on link: {}", link)
//                 }
//                 auth.links.insert((link.src.clone(), link.dst.clone()));
//             }
//             None => {
//                 let credentials = Credentials {
//                     user: open_syn_property.user,
//                     password,
//                 };
//                 let mut links = HashSet::new();
//                 links.insert((link.src.clone(), link.dst.clone()));
//                 let auth = Authenticated { credentials, links };
//                 guard.insert(cookie.zid, auth);
//             }
//         }

//         Ok(None)
//     }

//     async fn handle_open_ack(
//         &self,
//         _link: &AuthenticatedLink,
//         _property: Option<Vec<u8>>,
//     ) -> ZResult<Option<Vec<u8>>> {
//         Ok(None)
//     }

//     async fn handle_link_err(&self, link: &AuthenticatedLink) {
//         // Need to check if it authenticated and remove it if this is the last link
//         let mut guard = zasynclock!(self.authenticated);
//         let mut to_del: Option<ZenohId> = None;
//         for (node_id, auth) in guard.iter_mut() {
//             auth.links.remove(&(link.src.clone(), link.dst.clone()));
//             if auth.links.is_empty() {
//                 to_del = Some(*node_id);
//                 break;
//             }
//         }
//         if let Some(node_id) = to_del.take() {
//             guard.remove(&node_id);
//         }
//     }

//     async fn handle_close(&self, node_id: &ZenohId) {
//         zasynclock!(self.authenticated).remove(node_id);
//     }
// }

// //noinspection ALL
// impl From<Arc<UserPasswordAuthenticator>> for TransportAuthenticator {
//     fn from(v: Arc<UserPasswordAuthenticator>) -> TransportAuthenticator {
//         TransportAuthenticator(v)
//     }
// }

// impl From<UserPasswordAuthenticator> for TransportAuthenticator {
//     fn from(v: UserPasswordAuthenticator) -> TransportAuthenticator {
//         Self::from(Arc::new(v))
//     }
// }
