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
use super::{properties, AuthenticatedPeerLink, PeerAuthenticatorTrait};
use crate::core::{PeerId, Property, ZInt};
use crate::io::{RBuf, WBuf};
use crate::link::Locator;
use async_std::sync::{Mutex, RwLock};
use async_trait::async_trait;
use rand::Rng;
use std::collections::HashMap;
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
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
    fn write_initsyn_property(&mut self, initsyn_property: &InitSynProperty) -> bool {
        self.write_zint(initsyn_property.version)
    }
}

impl RBuf {
    fn read_initsyn_property(&mut self) -> Option<InitSynProperty> {
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
    fn write_initack_property(&mut self, initack_property: &InitAckProperty) -> bool {
        self.write_zint(initack_property.nonce)
    }
}

impl RBuf {
    fn read_initack_property(&mut self) -> Option<InitAckProperty> {
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
    user: String,
    hash: String,
}

impl WBuf {
    fn write_opensyn_property(&mut self, opensyn_property: &OpenSynProperty) -> bool {
        zcheck!(self.write_string(&opensyn_property.user));
        self.write_string(&opensyn_property.hash)
    }
}

impl RBuf {
    fn read_opensyn_property(&mut self) -> Option<OpenSynProperty> {
        let user = self.read_string()?;
        let hash = self.read_string()?;
        Some(OpenSynProperty { user, hash })
    }
}

/*************************************/
/*          Authenticator            */
/*************************************/
pub struct Credentials {
    user: String,
    password: String,
}

pub struct UserPasswordAuthenticator {
    lookup: RwLock<HashMap<String, String>>,
    credentials: Credentials,
    nonces: Mutex<HashMap<(Locator, Locator), ZInt>>,
}

impl UserPasswordAuthenticator {
    pub fn new(
        lookup: HashMap<String, String>,
        credentials: (String, String),
    ) -> UserPasswordAuthenticator {
        UserPasswordAuthenticator {
            lookup: RwLock::new(lookup),
            credentials: Credentials {
                user: credentials.0,
                password: credentials.1,
            },
            nonces: Mutex::new(HashMap::new()),
        }
    }

    pub async fn add_user(&self, user: String, password: String) -> ZResult<()> {
        let mut guard = zasyncwrite!(self.lookup);
        guard.insert(user, password);
        Ok(())
    }

    pub async fn del_user(&self, user: &str) -> ZResult<()> {
        let mut guard = zasyncwrite!(self.lookup);
        guard.remove(user);
        Ok(())
    }
}

#[async_trait]
impl PeerAuthenticatorTrait for UserPasswordAuthenticator {
    async fn get_init_syn_properties(
        &self,
        _link: &AuthenticatedPeerLink,
        _peer_id: &PeerId,
    ) -> ZResult<Vec<Property>> {
        let initsyn_property = InitSynProperty {
            version: USRPWD_VERSION,
        };
        let mut wbuf = WBuf::new(WBUF_SIZE, false);
        wbuf.write_initsyn_property(&initsyn_property);
        let rbuf: RBuf = wbuf.into();

        let prop = Property {
            key: properties::authorization::USRPWD,
            value: rbuf.to_vec(),
        };
        Ok(vec![prop])
    }

    async fn handle_init_syn(
        &self,
        link: &AuthenticatedPeerLink,
        _peer_id: &PeerId,
        sn_resolution: ZInt,
        properties: &[Property],
    ) -> ZResult<Vec<Property>> {
        let res = properties
            .iter()
            .find(|p| p.key == properties::authorization::USRPWD);
        let mut rbuf: RBuf = match res {
            Some(p) => p.value.clone().into(),
            None => {
                return zerror!(ZErrorKind::InvalidMessage {
                    descr: format!("Received InitSyn with no attachment on link: {}", link),
                });
            }
        };
        let initsyn_property = match rbuf.read_initsyn_property() {
            Some(isa) => isa,
            None => {
                return zerror!(ZErrorKind::InvalidMessage {
                    descr: format!("Received InitSyn with invalid attachment on link: {}", link),
                });
            }
        };

        if initsyn_property.version != USRPWD_VERSION {
            return zerror!(ZErrorKind::InvalidMessage {
                descr: format!("Rejected InitSyn with invalid attachment on link: {}", link),
            });
        }

        // Create the InitAck attachment
        let nonce = {
            let mut rng = rand::thread_rng();
            rng.gen_range(0..sn_resolution)
        };
        let initack_property = InitAckProperty { nonce };
        // Encode the InitAck property
        let mut wbuf = WBuf::new(WBUF_SIZE, false);
        wbuf.write_initack_property(&initack_property);
        let rbuf: RBuf = wbuf.into();
        let prop = Property {
            key: properties::authorization::USRPWD,
            value: rbuf.to_vec(),
        };

        // Insert the nonce in the set of sent nonces
        zasynclock!(self.nonces).insert((link.src.clone(), link.dst.clone()), nonce);

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
            .find(|p| p.key == properties::authorization::USRPWD);
        let mut rbuf: RBuf = match res {
            Some(p) => p.value.clone().into(),
            None => {
                return zerror!(ZErrorKind::InvalidMessage {
                    descr: format!("Received InitAck with no attachment on link: {}", link),
                });
            }
        };
        let _initack_property = match rbuf.read_initack_property() {
            Some(isa) => isa,
            None => {
                return zerror!(ZErrorKind::InvalidMessage {
                    descr: format!("Received InitAck with invalid attachment on link: {}", link),
                });
            }
        };

        // Create the OpenSyn attachment
        let opensyn_property = OpenSynProperty {
            user: self.credentials.user.clone(),
            hash: self.credentials.password.clone(),
        };
        // Encode the InitAck attachment
        let mut wbuf = WBuf::new(WBUF_SIZE, false);
        wbuf.write_opensyn_property(&opensyn_property);
        let rbuf: RBuf = wbuf.into();
        let prop = Property {
            key: properties::authorization::USRPWD,
            value: rbuf.to_vec(),
        };

        Ok(vec![prop])
    }

    async fn handle_open_syn(
        &self,
        link: &AuthenticatedPeerLink,
        properties: &[Property],
    ) -> ZResult<Vec<Property>> {
        let _nonce = match zasynclock!(self.nonces).remove(&(link.src.clone(), link.dst.clone())) {
            Some(nonce) => nonce,
            None => {
                return zerror!(ZErrorKind::InvalidMessage {
                    descr: format!(
                        "Received OpenSyn but no nonce has been created for link: {}",
                        link
                    ),
                });
            }
        };

        let res = properties
            .iter()
            .find(|p| p.key == properties::authorization::USRPWD);
        let mut rbuf: RBuf = match res {
            Some(p) => p.value.clone().into(),
            None => {
                return zerror!(ZErrorKind::InvalidMessage {
                    descr: format!("Received OpenSyn with no attachment on link: {}", link),
                });
            }
        };
        let opensyn_property = match rbuf.read_opensyn_property() {
            Some(osp) => osp,
            None => {
                return zerror!(ZErrorKind::InvalidMessage {
                    descr: format!("Received InitAck with invalid attachment on link: {}", link),
                });
            }
        };

        let password = match zasyncread!(self.lookup).get(&opensyn_property.user) {
            Some(password) => password.clone(),
            None => {
                return zerror!(ZErrorKind::InvalidMessage {
                    descr: format!("Received OpenSyn with invalid user on link: {}", link),
                });
            }
        };

        if password != opensyn_property.hash {
            return zerror!(ZErrorKind::InvalidMessage {
                descr: format!("Received OpenSyn with invalid password on link: {}", link),
            });
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
    }
}
