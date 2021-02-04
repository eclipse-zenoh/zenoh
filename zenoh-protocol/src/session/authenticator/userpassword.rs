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
use super::{attachment, AuthenticatedPeerLink, PeerAuthenticator, PeerAuthenticatorTrait};
use crate::core::{PeerId, Property, ZInt};
use crate::io::{RBuf, WBuf};
use crate::link::Locator;
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
/// - 0x20 => UserPassword authentication
/// ```

/*************************************/
/*             InitSyn               */
/*************************************/
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |0 0 1|  ATTCH  |
/// +-+-+-+---------+
/// ~    version    ~
/// +---------------+
struct InitSynProperty {
    version: ZInt,
}

impl WBuf {
    fn write_init_syn_property(&mut self, init_syn_property: &InitSynProperty) -> bool {
        self.write_zint(init_syn_property.version)
    }
}

impl RBuf {
    fn read_init_syn_property(&mut self) -> Option<InitSynProperty> {
        let version = self.read_zint()?;
        Some(InitSynProperty { version })
    }
}

/*************************************/
/*             InitAck               */
/*************************************/
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |0 0 1|  ATTCH  |
/// +-+-+-+---------+
/// ~     nonce     ~
/// +---------------+
struct InitAckProperty {
    nonce: ZInt,
}

impl WBuf {
    fn write_init_ack_property(&mut self, init_ack_property: &InitAckProperty) -> bool {
        self.write_zint(init_ack_property.nonce)
    }
}

impl RBuf {
    fn read_init_ack_property(&mut self) -> Option<InitAckProperty> {
        let nonce = self.read_zint()?;
        Some(InitAckProperty { nonce })
    }
}

/*************************************/
/*             OpenSyn               */
/*************************************/
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |0 0 1|  ATTCH  |
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
    fn write_open_syn_property(&mut self, open_syn_property: &OpenSynProperty) -> bool {
        zcheck!(self.write_bytes_array(&open_syn_property.user));
        self.write_bytes_array(&open_syn_property.hmac)
    }
}

impl RBuf {
    fn read_open_syn_property(&mut self) -> Option<OpenSynProperty> {
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
    credentials: Credentials,
    nonces: Mutex<HashMap<(Locator, Locator), (PeerId, ZInt)>>,
    authenticated: Mutex<HashMap<PeerId, Authenticated>>,
    prng: Mutex<PseudoRng>,
}

impl UserPasswordAuthenticator {
    pub fn new(
        lookup: HashMap<Vec<u8>, Vec<u8>>,
        credentials: (Vec<u8>, Vec<u8>),
    ) -> UserPasswordAuthenticator {
        UserPasswordAuthenticator {
            lookup: RwLock::new(lookup),
            credentials: Credentials {
                user: credentials.0,
                password: credentials.1,
            },
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
        if let Some(user) = config.get(&ZN_USER_KEY) {
            if let Some(password) = config.get(&ZN_PASSWORD_KEY) {
                // We have both user password parameter defined. Check if we
                // need to build the user-password lookup dictionary for incoming
                // connections, e.g. on the router.
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
                }
                // Create the UserPassword Authenticator based on provided info
                let upa = UserPasswordAuthenticator::new(
                    lookup,
                    (user.to_string().into(), password.to_string().into()),
                );
                log::debug!("User-password authentication is enabled");

                return Ok(Some(upa));
            }
        }
        Ok(None)
    }
}

#[async_trait]
impl PeerAuthenticatorTrait for UserPasswordAuthenticator {
    async fn get_init_syn_properties(
        &self,
        _link: &AuthenticatedPeerLink,
        _peer_id: &PeerId,
    ) -> ZResult<Vec<Property>> {
        let init_syn_property = InitSynProperty {
            version: USRPWD_VERSION,
        };
        let mut wbuf = WBuf::new(WBUF_SIZE, false);
        wbuf.write_init_syn_property(&init_syn_property);
        let rbuf: RBuf = wbuf.into();

        let prop = Property {
            key: attachment::authorization::USRPWD,
            value: rbuf.to_vec(),
        };
        Ok(vec![prop])
    }

    async fn handle_init_syn(
        &self,
        link: &AuthenticatedPeerLink,
        peer_id: &PeerId,
        sn_resolution: ZInt,
        properties: &[Property],
    ) -> ZResult<Vec<Property>> {
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
        let init_syn_property = match rbuf.read_init_syn_property() {
            Some(isa) => isa,
            None => {
                return zerror!(ZErrorKind::InvalidMessage {
                    descr: format!("Received InitSyn with invalid attachment on link: {}", link),
                });
            }
        };

        if init_syn_property.version != USRPWD_VERSION {
            return zerror!(ZErrorKind::InvalidMessage {
                descr: format!("Rejected InitSyn with invalid attachment on link: {}", link),
            });
        }

        // Create the InitAck attachment
        let nonce = zasynclock!(self.prng).gen_range(0..sn_resolution);
        let init_ack_property = InitAckProperty { nonce };
        // Encode the InitAck property
        let mut wbuf = WBuf::new(WBUF_SIZE, false);
        wbuf.write_init_ack_property(&init_ack_property);
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

        Ok(vec![prop])
    }

    async fn handle_init_ack(
        &self,
        link: &AuthenticatedPeerLink,
        _peer_id: &PeerId,
        _sn_resolution: ZInt,
        properties: &[Property],
    ) -> ZResult<Vec<Property>> {
        let res = properties
            .iter()
            .find(|p| p.key == attachment::authorization::USRPWD);
        let mut rbuf: RBuf = match res {
            Some(p) => p.value.clone().into(),
            None => {
                return zerror!(ZErrorKind::InvalidMessage {
                    descr: format!("Received InitAck with no attachment on link: {}", link),
                });
            }
        };
        let init_ack_property = match rbuf.read_init_ack_property() {
            Some(isa) => isa,
            None => {
                return zerror!(ZErrorKind::InvalidMessage {
                    descr: format!("Received InitAck with invalid attachment on link: {}", link),
                });
            }
        };

        // Create the HMAC of the password using the nonce received as a key (it's a challenge)
        let key = init_ack_property.nonce.to_le_bytes();
        let hmac = hmac::sign(&key, &self.credentials.password)?;
        // Create the OpenSyn attachment
        let open_syn_property = OpenSynProperty {
            user: self.credentials.user.clone(),
            hmac,
        };
        // Encode the InitAck attachment
        let mut wbuf = WBuf::new(WBUF_SIZE, false);
        wbuf.write_open_syn_property(&open_syn_property);
        let rbuf: RBuf = wbuf.into();
        let prop = Property {
            key: attachment::authorization::USRPWD,
            value: rbuf.to_vec(),
        };

        Ok(vec![prop])
    }

    async fn handle_open_syn(
        &self,
        link: &AuthenticatedPeerLink,
        properties: &[Property],
    ) -> ZResult<Vec<Property>> {
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
        let open_syn_property = match rbuf.read_open_syn_property() {
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

        Ok(vec![])
    }

    async fn handle_open_ack(
        &self,
        _link: &AuthenticatedPeerLink,
        _properties: &[Property],
    ) -> ZResult<()> {
        Ok(())
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
