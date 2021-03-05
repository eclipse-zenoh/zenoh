//
// Copyright (c) 2017, 2020 ADLINK Technology Inc.
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ADLINK zenoh team, <zenoh@adlink-labs.tech>
//
use super::{
    attachment, AuthenticatedPeerLink, PeerAuthenticator, PeerAuthenticatorOutput,
    PeerAuthenticatorTrait,
};
use super::{Locator, PeerId, Property, RBuf, WBuf, ZInt};
use async_std::fs;
use async_std::sync::{Arc, Mutex, RwLock};
use async_trait::async_trait;
use rand::{Rng, SeedableRng};
use std::collections::{HashMap, HashSet};
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::crypto::{hmac, PseudoRng};
use zenoh_util::properties::config::*;
use zenoh_util::properties::Properties;
use zenoh_util::{zasynclock, zasyncread, zasyncwrite};

const WBUF_SIZE: usize = 64;
const USRPWD_VERSION: ZInt = 0;

/// # Attachment decorator
///
/// ```text
/// The Attachment can decorate any message (i.e., SessionMessage and ZenohMessage) and it allows to
/// append to the message any additional information. Since the information contained in the
/// Attachement is relevant only to the layer that provided them (e.g., Session, Zenoh, User) it
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

impl WBuf {
    fn write_init_syn_property_usrpwd(&mut self, init_syn_property: &InitSynProperty) -> bool {
        self.write_zint(init_syn_property.version)
    }
}

impl RBuf {
    fn read_init_syn_property_usrpwd(&mut self) -> Option<InitSynProperty> {
        let version = self.read_zint()?;
        Some(InitSynProperty { version })
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

impl WBuf {
    fn write_init_ack_property_usrpwd(&mut self, init_ack_property: &InitAckProperty) -> bool {
        self.write_zint(init_ack_property.nonce)
    }
}

impl RBuf {
    fn read_init_ack_property_usrpwd(&mut self) -> Option<InitAckProperty> {
        let nonce = self.read_zint()?;
        Some(InitAckProperty { nonce })
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

impl WBuf {
    fn write_open_syn_property_usrpwd(&mut self, open_syn_property: &OpenSynProperty) -> bool {
        zcheck!(self.write_bytes_array(&open_syn_property.user));
        self.write_bytes_array(&open_syn_property.hmac)
    }
}

impl RBuf {
    fn read_open_syn_property_usrpwd(&mut self) -> Option<OpenSynProperty> {
        let user = self.read_bytes_array()?;
        let hmac = self.read_bytes_array()?;
        Some(OpenSynProperty { user, hmac })
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
    nonces: Mutex<HashMap<(Locator, Locator), (PeerId, ZInt)>>,
    authenticated: Mutex<HashMap<PeerId, Authenticated>>,
    prng: Mutex<PseudoRng>,
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
            nonces: Mutex::new(HashMap::new()),
            authenticated: Mutex::new(HashMap::new()),
            prng: Mutex::new(PseudoRng::from_entropy()),
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

    pub async fn from_properties(
        config: &ConfigProperties,
    ) -> ZResult<Option<UserPasswordAuthenticator>> {
        let mut lookup: HashMap<Vec<u8>, Vec<u8>> = HashMap::new();
        if let Some(dict) = config.get(&ZN_USER_PASSWORD_DICTIONARY_KEY) {
            let content = fs::read_to_string(dict).await.map_err(|e| {
                zerror2!(ZErrorKind::Other {
                    descr: format!("Invalid user-password dictionary file: {}", e)
                })
            })?;
            // Populate the user-password dictionary
            let mut ps = Properties::from(content);
            for (user, password) in ps.drain() {
                lookup.insert(user.into(), password.into());
            }
            log::debug!("User-password dictionary has been configured");
        }

        let mut credentials: Option<(Vec<u8>, Vec<u8>)> = None;
        if let Some(user) = config.get(&ZN_USER_KEY) {
            if let Some(password) = config.get(&ZN_PASSWORD_KEY) {
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
    async fn get_init_syn_properties(
        &self,
        _link: &AuthenticatedPeerLink,
        _peer_id: &PeerId,
    ) -> ZResult<PeerAuthenticatorOutput> {
        let mut res = PeerAuthenticatorOutput::default();
        // If credentials are not configured, don't initiate the USRPWD authentication
        if self.credentials.is_none() {
            return Ok(res);
        }

        let init_syn_property = InitSynProperty {
            version: USRPWD_VERSION,
        };
        let mut wbuf = WBuf::new(WBUF_SIZE, false);
        wbuf.write_init_syn_property_usrpwd(&init_syn_property);
        let rbuf: RBuf = wbuf.into();

        let prop = Property {
            key: attachment::authorization::USRPWD,
            value: rbuf.to_vec(),
        };
        res.properties.push(prop);
        Ok(res)
    }

    async fn handle_init_syn(
        &self,
        link: &AuthenticatedPeerLink,
        peer_id: &PeerId,
        sn_resolution: ZInt,
        properties: &[Property],
    ) -> ZResult<PeerAuthenticatorOutput> {
        let res = properties
            .iter()
            .find(|p| p.key == attachment::authorization::USRPWD);
        let mut rbuf: RBuf = match res {
            Some(p) => p.value.clone().into(),
            None => {
                return zerror!(ZErrorKind::InvalidMessage {
                    descr: format!("Received InitSyn with no attachment on link: {}", link),
                });
            }
        };
        let init_syn_property = match rbuf.read_init_syn_property_usrpwd() {
            Some(isa) => isa,
            None => {
                return zerror!(ZErrorKind::InvalidMessage {
                    descr: format!("Received InitSyn with invalid attachment on link: {}", link),
                });
            }
        };

        if init_syn_property.version > USRPWD_VERSION {
            return zerror!(ZErrorKind::InvalidMessage {
                descr: format!("Rejected InitSyn with invalid attachment on link: {}", link),
            });
        }

        // Create the InitAck attachment
        let nonce = zasynclock!(self.prng).gen_range(0..sn_resolution);
        let init_ack_property = InitAckProperty { nonce };
        // Encode the InitAck property
        let mut wbuf = WBuf::new(WBUF_SIZE, false);
        wbuf.write_init_ack_property_usrpwd(&init_ack_property);
        let rbuf: RBuf = wbuf.into();
        let prop = Property {
            key: attachment::authorization::USRPWD,
            value: rbuf.to_vec(),
        };

        // Insert the nonce in the set of sent nonces
        zasynclock!(self.nonces).insert(
            (link.src.clone(), link.dst.clone()),
            (peer_id.clone(), nonce),
        );

        let mut res = PeerAuthenticatorOutput::default();
        res.properties.push(prop);
        Ok(res)
    }

    async fn handle_init_ack(
        &self,
        link: &AuthenticatedPeerLink,
        _peer_id: &PeerId,
        _sn_resolution: ZInt,
        properties: &[Property],
    ) -> ZResult<PeerAuthenticatorOutput> {
        let mut res = PeerAuthenticatorOutput::default();
        // If credentials are not configured, don't continue the USRPWD authentication
        let credentials = match self.credentials.as_ref() {
            Some(cr) => cr,
            None => return Ok(res),
        };

        let tmp = properties
            .iter()
            .find(|p| p.key == attachment::authorization::USRPWD);
        let mut rbuf: RBuf = match tmp {
            Some(p) => p.value.clone().into(),
            None => {
                return zerror!(ZErrorKind::InvalidMessage {
                    descr: format!("Received InitAck with no attachment on link: {}", link),
                });
            }
        };
        let init_ack_property = match rbuf.read_init_ack_property_usrpwd() {
            Some(isa) => isa,
            None => {
                return zerror!(ZErrorKind::InvalidMessage {
                    descr: format!("Received InitAck with invalid attachment on link: {}", link),
                });
            }
        };

        // Create the HMAC of the password using the nonce received as a key (it's a challenge)
        let key = init_ack_property.nonce.to_le_bytes();
        let hmac = hmac::sign(&key, &credentials.password)?;
        // Create the OpenSyn attachment
        let open_syn_property = OpenSynProperty {
            user: credentials.user.clone(),
            hmac,
        };
        // Encode the InitAck attachment
        let mut wbuf = WBuf::new(WBUF_SIZE, false);
        wbuf.write_open_syn_property_usrpwd(&open_syn_property);
        let rbuf: RBuf = wbuf.into();
        let prop = Property {
            key: attachment::authorization::USRPWD,
            value: rbuf.to_vec(),
        };
        res.properties.push(prop);
        Ok(res)
    }

    async fn handle_open_syn(
        &self,
        link: &AuthenticatedPeerLink,
        properties: &[Property],
    ) -> ZResult<PeerAuthenticatorOutput> {
        let (peer_id, nonce) =
            match zasynclock!(self.nonces).remove(&(link.src.clone(), link.dst.clone())) {
                Some(tuple) => tuple,
                None => {
                    return zerror!(ZErrorKind::InvalidMessage {
                        descr: format!(
                            "Received OpenSyn but no nonce has been associated to link: {}",
                            link
                        ),
                    });
                }
            };

        let res = properties
            .iter()
            .find(|p| p.key == attachment::authorization::USRPWD);
        let mut rbuf: RBuf = match res {
            Some(p) => p.value.clone().into(),
            None => {
                return zerror!(ZErrorKind::InvalidMessage {
                    descr: format!("Received OpenSyn with no attachment on link: {}", link),
                });
            }
        };
        let open_syn_property = match rbuf.read_open_syn_property_usrpwd() {
            Some(osp) => osp,
            None => {
                return zerror!(ZErrorKind::InvalidMessage {
                    descr: format!("Received InitAck with invalid attachment on link: {}", link),
                });
            }
        };

        let password = match zasyncread!(self.lookup).get(&open_syn_property.user) {
            Some(password) => password.clone(),
            None => {
                return zerror!(ZErrorKind::InvalidMessage {
                    descr: format!("Received OpenSyn with invalid user on link: {}", link),
                });
            }
        };

        // Create the HMAC of the password using the nonce received as challenge
        let key = nonce.to_le_bytes();
        let hmac = hmac::sign(&key, &password)?;
        if hmac != open_syn_property.hmac {
            return zerror!(ZErrorKind::InvalidMessage {
                descr: format!("Received OpenSyn with invalid password on link: {}", link),
            });
        }

        // Check PID validity
        let mut guard = zasynclock!(self.authenticated);
        match guard.get_mut(&peer_id) {
            Some(auth) => {
                if open_syn_property.user != auth.credentials.user
                    || password != auth.credentials.password
                {
                    return zerror!(ZErrorKind::InvalidMessage {
                        descr: format!("Received OpenSyn with invalid password on link: {}", link),
                    });
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
                guard.insert(peer_id, auth);
            }
        }

        Ok(PeerAuthenticatorOutput::default())
    }

    async fn handle_open_ack(
        &self,
        _link: &AuthenticatedPeerLink,
        _properties: &[Property],
    ) -> ZResult<PeerAuthenticatorOutput> {
        Ok(PeerAuthenticatorOutput::default())
    }

    async fn handle_link_err(&self, link: &AuthenticatedPeerLink) {
        zasynclock!(self.nonces).remove(&(link.src.clone(), link.dst.clone()));

        // Need to check if it authenticated and remove it if this is the last link
        let mut guard = zasynclock!(self.authenticated);
        let mut to_del: Option<PeerId> = None;
        for (peer_id, auth) in guard.iter_mut() {
            auth.links.remove(&(link.src.clone(), link.dst.clone()));
            if auth.links.is_empty() {
                to_del = Some(peer_id.clone());
                break;
            }
        }
        if let Some(peer_id) = to_del.take() {
            guard.remove(&peer_id);
        }
    }

    async fn handle_close(&self, peer_id: &PeerId) {
        zasynclock!(self.authenticated).remove(peer_id);
    }
}

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
