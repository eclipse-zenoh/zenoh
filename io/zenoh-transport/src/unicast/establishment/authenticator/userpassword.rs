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
use super::{
    AuthenticatedPeerLink, PeerAuthenticator, PeerAuthenticatorId, PeerAuthenticatorTrait,
};
use super::{Locator, ZInt, ZenohId};
use crate::unicast::establishment::Cookie;
use async_std::fs;
use async_std::sync::{Mutex, RwLock};
use async_trait::async_trait;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use zenoh_buffers::{
    reader::{DidntRead, HasReader, Reader},
    writer::{DidntWrite, HasWriter, Writer},
};
use zenoh_cfg_properties::Properties;
use zenoh_codec::{RCodec, WCodec, Zenoh060};
use zenoh_config::Config;
use zenoh_core::{zasynclock, zasyncread, zasyncwrite};
use zenoh_crypto::hmac;
use zenoh_result::{bail, zerror, ZResult};

const USRPWD_VERSION: ZInt = 1;

/// # Attachment decorator
///
/// ```text
/// The Attachment can decorate any message (i.e., TransportMessage and ZenohMessage) and it allows to
/// append to the message any additional information. Since the information contained in the
/// Attachement is relevant only to the layer that provided them (e.g., Transport, Zenoh, User) it
/// is the duty of that layer to serialize and de-serialize the attachment whenever deemed necessary.
///
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// | ENC |  ATTCH  |
/// +-+-+-+---------+
/// ~   Attachment  ~
/// +---------------+
///
/// ENC values:
/// - 0x00 => Zenoh Properties
/// ```

/*************************************/
/*             InitSyn               */
/*************************************/
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |0 0 0|  ATTCH  |
/// +-+-+-+---------+
/// ~    version    ~
/// +---------------+
struct InitSynProperty {
    version: ZInt,
}

impl<W> WCodec<&InitSynProperty, &mut W> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &InitSynProperty) -> Self::Output {
        self.write(&mut *writer, x.version)?;
        Ok(())
    }
}

impl<R> RCodec<InitSynProperty, &mut R> for Zenoh060
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<InitSynProperty, Self::Error> {
        let version: ZInt = self.read(&mut *reader)?;
        Ok(InitSynProperty { version })
    }
}

/*************************************/
/*             InitAck               */
/*************************************/
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |0 0 0|  ATTCH  |
/// +-+-+-+---------+
/// ~     nonce     ~
/// +---------------+
struct InitAckProperty {
    nonce: ZInt,
}

impl<W> WCodec<&InitAckProperty, &mut W> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &InitAckProperty) -> Self::Output {
        self.write(&mut *writer, x.nonce)?;
        Ok(())
    }
}

impl<R> RCodec<InitAckProperty, &mut R> for Zenoh060
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<InitAckProperty, Self::Error> {
        let nonce: ZInt = self.read(&mut *reader)?;
        Ok(InitAckProperty { nonce })
    }
}

/*************************************/
/*             OpenSyn               */
/*************************************/
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |0 0 0|  ATTCH  |
/// +-+-+-+---------+
/// ~     user      ~
/// +---------------+
/// ~     hash      ~
/// +---------------+
struct OpenSynProperty {
    user: Vec<u8>,
    hmac: Vec<u8>,
}

impl<W> WCodec<&OpenSynProperty, &mut W> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &OpenSynProperty) -> Self::Output {
        self.write(&mut *writer, x.user.as_slice())?;
        self.write(&mut *writer, x.hmac.as_slice())?;
        Ok(())
    }
}

impl<R> RCodec<OpenSynProperty, &mut R> for Zenoh060
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<OpenSynProperty, Self::Error> {
        let user: Vec<u8> = self.read(&mut *reader)?;
        let hmac: Vec<u8> = self.read(&mut *reader)?;
        Ok(OpenSynProperty { user, hmac })
    }
}

/*************************************/
/*          Authenticator            */
/*************************************/
struct Credentials {
    user: Vec<u8>,
    password: Vec<u8>,
}

struct Authenticated {
    credentials: Credentials,
    links: HashSet<(Locator, Locator)>,
}

pub struct UserPasswordAuthenticator {
    lookup: RwLock<HashMap<Vec<u8>, Vec<u8>>>,
    credentials: Option<Credentials>,
    authenticated: Mutex<HashMap<ZenohId, Authenticated>>,
}

impl UserPasswordAuthenticator {
    pub fn new(
        lookup: HashMap<Vec<u8>, Vec<u8>>,
        mut credentials: Option<(Vec<u8>, Vec<u8>)>,
    ) -> UserPasswordAuthenticator {
        let credentials = credentials.take().map(|cr| Credentials {
            user: cr.0,
            password: cr.1,
        });
        UserPasswordAuthenticator {
            lookup: RwLock::new(lookup),
            credentials,
            authenticated: Mutex::new(HashMap::new()),
        }
    }

    pub async fn add_user(&self, user: Vec<u8>, password: Vec<u8>) -> ZResult<()> {
        let mut guard = zasyncwrite!(self.lookup);
        guard.insert(user, password);
        Ok(())
    }

    pub async fn del_user(&self, user: &[u8]) -> ZResult<()> {
        let mut guard = zasyncwrite!(self.lookup);
        guard.remove(user);
        Ok(())
    }

    pub async fn from_config(config: &Config) -> ZResult<Option<UserPasswordAuthenticator>> {
        let c = config.transport().auth().usrpwd();

        let mut lookup: HashMap<Vec<u8>, Vec<u8>> = HashMap::new();
        if let Some(dict) = c.dictionary_file() {
            let content = fs::read_to_string(dict)
                .await
                .map_err(|e| zerror!("Invalid user-password dictionary file: {}", e))?;
            // Populate the user-password dictionary
            let mut ps = Properties::from(content);
            for (user, password) in ps.drain() {
                lookup.insert(user.into(), password.into());
            }
            log::debug!("User-password dictionary has been configured");
        }

        let mut credentials: Option<(Vec<u8>, Vec<u8>)> = None;
        if let Some(user) = c.user() {
            if let Some(password) = c.password() {
                log::debug!("User and password have been configured");
                credentials = Some((user.to_string().into(), password.to_string().into()));
            }
        }

        if !lookup.is_empty() || credentials.is_some() {
            log::debug!("User-password authentication is enabled");
            Ok(Some(UserPasswordAuthenticator::new(lookup, credentials)))
        } else {
            Ok(None)
        }
    }
}

#[async_trait]
impl PeerAuthenticatorTrait for UserPasswordAuthenticator {
    fn id(&self) -> PeerAuthenticatorId {
        PeerAuthenticatorId::UserPassword
    }

    async fn close(&self) {
        // No cleanup needed
    }

    async fn get_init_syn_properties(
        &self,
        link: &AuthenticatedPeerLink,
        _peer_id: &ZenohId,
    ) -> ZResult<Option<Vec<u8>>> {
        // If credentials are not configured, don't initiate the USRPWD authentication
        if self.credentials.is_none() {
            return Ok(None);
        }

        let init_syn_property = InitSynProperty {
            version: USRPWD_VERSION,
        };
        let mut wbuf = vec![];
        let codec = Zenoh060::default();
        let mut writer = wbuf.writer();
        codec
            .write(&mut writer, &init_syn_property)
            .map_err(|_| zerror!("Error in encoding InitSyn for UsrPwd on link: {}", link))?;
        let attachment = wbuf;

        Ok(Some(attachment))
    }

    async fn handle_init_syn(
        &self,
        link: &AuthenticatedPeerLink,
        cookie: &Cookie,
        property: Option<Vec<u8>>,
    ) -> ZResult<(Option<Vec<u8>>, Option<Vec<u8>>)> {
        let p = property.ok_or_else(|| {
            zerror!(
                "Received InitSyn with no UsrPwd attachment on link: {}",
                link
            )
        })?;

        let codec = Zenoh060::default();

        let mut reader = p.reader();
        let init_syn_property: InitSynProperty = codec.read(&mut reader).map_err(|_| {
            zerror!(
                "Received InitSyn with invalid UsrPwd attachment on link: {}",
                link
            )
        })?;
        if init_syn_property.version > USRPWD_VERSION {
            bail!("Rejected InitSyn with invalid attachment on link: {}", link)
        }

        // Create the InitAck attachment
        let init_ack_property = InitAckProperty {
            nonce: cookie.nonce,
        };
        let mut wbuf = vec![];
        let mut writer = wbuf.writer();
        codec
            .write(&mut writer, &init_ack_property)
            .map_err(|_| zerror!("Error in encoding InitAck for UsrPwd on link: {}", link))?;
        let attachment = wbuf;

        Ok((Some(attachment), None))
    }

    async fn handle_init_ack(
        &self,
        link: &AuthenticatedPeerLink,
        _peer_id: &ZenohId,
        _sn_resolution: ZInt,
        property: Option<Vec<u8>>,
    ) -> ZResult<Option<Vec<u8>>> {
        // If credentials are not configured, don't continue the USRPWD authentication
        let credentials = match self.credentials.as_ref() {
            Some(cr) => cr,
            None => return Ok(None),
        };

        let p = property.ok_or_else(|| {
            zerror!(
                "Received InitAck with no UsrPwd attachment on link: {}",
                link
            )
        })?;

        let codec = Zenoh060::default();

        let mut reader = p.reader();
        let init_ack_property: InitAckProperty = codec.read(&mut reader).map_err(|_| {
            zerror!(
                "Received InitAck with invalid UsrPwd attachment on link: {}",
                link
            )
        })?;

        // Create the HMAC of the password using the nonce received as a key (it's a challenge)
        let key = init_ack_property.nonce.to_le_bytes();
        let hmac = hmac::sign(&key, &credentials.password)?;
        // Create the OpenSyn attachment
        let open_syn_property = OpenSynProperty {
            user: credentials.user.clone(),
            hmac,
        };
        // Encode the InitAck attachment
        let mut wbuf = vec![];
        let mut writer = wbuf.writer();
        codec
            .write(&mut writer, &open_syn_property)
            .map_err(|_| zerror!("Error in encoding OpenSyn for UsrPwd on link: {}", link))?;
        let attachment = wbuf;

        Ok(Some(attachment))
    }

    async fn handle_open_syn(
        &self,
        link: &AuthenticatedPeerLink,
        cookie: &Cookie,
        property: (Option<Vec<u8>>, Option<Vec<u8>>),
    ) -> ZResult<Option<Vec<u8>>> {
        let (attachment, _cookie) = property;
        let a = attachment.ok_or_else(|| {
            zerror!(
                "Received OpenSyn with no UsrPwd attachment on link: {}",
                link
            )
        })?;

        let codec = Zenoh060::default();

        let mut reader = a.reader();
        let open_syn_property: OpenSynProperty = codec.read(&mut reader).map_err(|_| {
            zerror!(
                "Received OpenSyn with invalid UsrPwd attachment on link: {}",
                link
            )
        })?;
        let password = match zasyncread!(self.lookup).get(&open_syn_property.user) {
            Some(password) => password.clone(),
            None => bail!("Received OpenSyn with invalid user on link: {}", link),
        };

        // Create the HMAC of the password using the nonce received as challenge
        let key = cookie.nonce.to_le_bytes();
        let hmac = hmac::sign(&key, &password)?;
        if hmac != open_syn_property.hmac {
            bail!("Received OpenSyn with invalid password on link: {}", link)
        }

        // Check PID validity
        let mut guard = zasynclock!(self.authenticated);
        match guard.get_mut(&cookie.zid) {
            Some(auth) => {
                if open_syn_property.user != auth.credentials.user
                    || password != auth.credentials.password
                {
                    bail!("Received OpenSyn with invalid password on link: {}", link)
                }
                auth.links.insert((link.src.clone(), link.dst.clone()));
            }
            None => {
                let credentials = Credentials {
                    user: open_syn_property.user,
                    password,
                };
                let mut links = HashSet::new();
                links.insert((link.src.clone(), link.dst.clone()));
                let auth = Authenticated { credentials, links };
                guard.insert(cookie.zid, auth);
            }
        }

        Ok(None)
    }

    async fn handle_open_ack(
        &self,
        _link: &AuthenticatedPeerLink,
        _property: Option<Vec<u8>>,
    ) -> ZResult<Option<Vec<u8>>> {
        Ok(None)
    }

    async fn handle_link_err(&self, link: &AuthenticatedPeerLink) {
        // Need to check if it authenticated and remove it if this is the last link
        let mut guard = zasynclock!(self.authenticated);
        let mut to_del: Option<ZenohId> = None;
        for (peer_id, auth) in guard.iter_mut() {
            auth.links.remove(&(link.src.clone(), link.dst.clone()));
            if auth.links.is_empty() {
                to_del = Some(*peer_id);
                break;
            }
        }
        if let Some(peer_id) = to_del.take() {
            guard.remove(&peer_id);
        }
    }

    async fn handle_close(&self, peer_id: &ZenohId) {
        zasynclock!(self.authenticated).remove(peer_id);
    }
}

//noinspection ALL
impl From<Arc<UserPasswordAuthenticator>> for PeerAuthenticator {
    fn from(v: Arc<UserPasswordAuthenticator>) -> PeerAuthenticator {
        PeerAuthenticator(v)
    }
}

impl From<UserPasswordAuthenticator> for PeerAuthenticator {
    fn from(v: UserPasswordAuthenticator) -> PeerAuthenticator {
        Self::from(Arc::new(v))
    }
}
