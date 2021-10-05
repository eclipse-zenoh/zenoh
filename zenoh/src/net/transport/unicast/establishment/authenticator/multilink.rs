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
    AuthenticatedPeerLink, PeerAuthenticator, PeerAuthenticatorId, PeerAuthenticatorOutput,
    PeerAuthenticatorTrait,
};
use super::{Locator, PeerId, Property, WBuf, ZBuf, ZInt};
use async_std::fs;
use async_std::sync::{Arc, Mutex, RwLock};
use async_trait::async_trait;
use num_traits::cast::ToPrimitive;
use rand::{Rng, SeedableRng};
use rsa::{BigUint, PublicKeyParts, RsaPrivateKey, RsaPublicKey};
use std::collections::{HashMap, HashSet};
use std::convert::TryInto;
use std::hash::Hash;
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::crypto::PseudoRng;
use zenoh_util::properties::config::*;
use zenoh_util::properties::Properties;
use zenoh_util::{zasynclock, zasyncread, zasyncwrite};

const WBUF_SIZE: usize = 64;
const MULTILINK_VERSION: ZInt = 0;

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

impl WBuf {
    fn write_rsa_pub_key(&mut self, pub_key: &RsaPublicKey) -> bool {
        let n = match pub_key.n().to_u64() {
            Some(n) => n,
            None => return false,
        };
        let e = match pub_key.e().to_u64() {
            Some(e) => e,
            None => return false,
        };

        self.write_zint(n) && self.write_zint(e)
    }
}

impl ZBuf {
    fn read_rsa_pub_key(&mut self) -> Option<RsaPublicKey> {
        let n: BigUint = self.read_zint()?.into();
        let e: BigUint = self.read_zint()?.into();

        RsaPublicKey::new(n, e).ok()
    }
}

/*************************************/
/*             InitSyn               */
/*************************************/
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |0 0 0|  ATTCH  |
/// +-+-+-+---------+
/// ~    version    ~
/// +---------------+
/// ~  public key   ~
/// +---------------+
struct InitSynProperty {
    version: ZInt,
    alice_pubkey: RsaPublicKey,
}

impl WBuf {
    fn write_init_syn_property_multilink(&mut self, init_syn_property: &InitSynProperty) -> bool {
        self.write_zint(init_syn_property.version)
            && self.write_rsa_pub_key(&init_syn_property.alice_pubkey)
    }
}

impl ZBuf {
    fn read_init_syn_property_multilink(&mut self) -> Option<InitSynProperty> {
        let version = self.read_zint()?;
        let alice_pubkey = self.read_rsa_pub_key()?;

        Some(InitSynProperty {
            version,
            alice_pubkey,
        })
    }
}

/*************************************/
/*             InitAck               */
/*************************************/
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |0 0 0|  ATTCH  |
/// +-+-+-+---------+
/// ~  public key   ~
/// +---------------+
/// ~ ciphered nonce~
/// +---------------+
struct InitAckProperty {
    bob_pubkey: RsaPublicKey,
    nonce_encrypted_with_alice_pubkey: Vec<u8>,
}

impl WBuf {
    fn write_init_ack_property_multilink(&mut self, init_ack_property: &InitAckProperty) -> bool {
        self.write_rsa_pub_key(&init_ack_property.bob_pubkey)
            && self.write_bytes_array(
                init_ack_property
                    .nonce_encrypted_with_alice_pubkey
                    .as_slice(),
            )
    }
}

impl ZBuf {
    fn read_init_ack_property_multilink(&mut self) -> Option<InitAckProperty> {
        let bob_pubkey = self.read_rsa_pub_key()?;
        let nonce_encrypted_with_alice_pubkey = self.read_bytes_array()?;
        Some(InitAckProperty {
            bob_pubkey,
            nonce_encrypted_with_alice_pubkey,
        })
    }
}

/*************************************/
/*             OpenSyn               */
/*************************************/
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |0 0 0|  ATTCH  |
/// +-+-+-+---------+
/// ~ ciphered nonce~
/// +---------------+
struct OpenSynProperty {
    nonce_encrypted_with_bob_pubkey: Vec<u8>,
}

impl WBuf {
    fn write_open_syn_property_multilink(&mut self, open_syn_property: &OpenSynProperty) -> bool {
        self.write_bytes_array(open_syn_property.nonce_encrypted_with_bob_pubkey.as_slice())
    }
}

impl ZBuf {
    fn read_open_syn_property_multilink(&mut self) -> Option<OpenSynProperty> {
        let nonce_encrypted_with_bob_pubkey = self.read_bytes_array()?;
        Some(OpenSynProperty {
            nonce_encrypted_with_bob_pubkey,
        })
    }
}

/*************************************/
/*          Authenticator            */
/*************************************/
struct InnerState {
    prng: PseudoRng,
    known_keys: HashSet<RsaPublicKey>,
    pending: HashMap<PeerId, (RsaPublicKey, ZInt)>,
    authenticated: HashMap<PeerId, RsaPublicKey>,
}

pub struct MultiLinkAuthenticator {
    pub_key: RsaPublicKey,
    prv_key: RsaPrivateKey,
    state: RwLock<InnerState>,
}

impl MultiLinkAuthenticator {
    pub fn new(pub_key: RsaPublicKey, prv_key: RsaPrivateKey) -> MultiLinkAuthenticator {
        MultiLinkAuthenticator {
            pub_key,
            prv_key,
            state: RwLock::new(InnerState {
                prng: PseudoRng::from_entropy(),
                known_keys: HashSet::new(),
                pending: HashMap::new(),
                authenticated: HashMap::new(),
            }),
        }
    }

    pub async fn add_key(&self, key: RsaPublicKey) -> ZResult<()> {
        let mut guard = zasyncwrite!(self.state);
        guard.known_keys.insert(key);
        Ok(())
    }

    pub async fn del_key(&self, key: &RsaPublicKey) -> ZResult<()> {
        let mut guard = zasyncwrite!(self.state);
        guard.known_keys.remove(key);
        Ok(())
    }

    pub async fn from_config(
        _config: &ConfigProperties,
    ) -> ZResult<Option<MultiLinkAuthenticator>> {
        // @TODO
        let mut prng = PseudoRng::from_entropy();

        let bits = 4096;
        let prv_key = RsaPrivateKey::new(&mut prng, bits).map_err(|e| {
            zerror2!(ZErrorKind::Other {
                descr: format!("Failed to create Rsa Private Key: {}", e)
            })
        })?;
        let pub_key = RsaPublicKey::from(&prv_key);

        let mla = MultiLinkAuthenticator {
            pub_key,
            prv_key,
            state: RwLock::new(InnerState {
                prng,
                known_keys: HashSet::new(),
                pending: HashMap::new(),
                authenticated: HashMap::new(),
            }),
        };

        Ok(Some(mla))
    }
}

#[async_trait]
impl PeerAuthenticatorTrait for MultiLinkAuthenticator {
    fn id(&self) -> PeerAuthenticatorId {
        PeerAuthenticatorId::MultiLink
    }

    async fn get_init_syn_properties(
        &self,
        _link: &AuthenticatedPeerLink,
        _peer_id: &PeerId,
    ) -> ZResult<PeerAuthenticatorOutput> {
        let mut res = PeerAuthenticatorOutput::new();
        let init_syn_property = InitSynProperty {
            version: MULTILINK_VERSION,
            alice_pubkey: self.pub_key.clone(),
        };

        let mut wbuf = WBuf::new(WBUF_SIZE, false);
        wbuf.write_init_syn_property_multilink(&init_syn_property);
        let zbuf: ZBuf = wbuf.into();

        let prop = Property {
            key: PeerAuthenticatorId::MultiLink as ZInt,
            value: zbuf.to_vec(),
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
            .find(|p| p.key == PeerAuthenticatorId::MultiLink as ZInt);

        let mut guard = zasyncwrite!(self.state);
        match res {
            Some(init_syn) => {
                match guard.authenticated.get(peer_id) {
                    Some(pid) => {
                        // let a =
                    }
                    None => guard.authenticated.insert(),
                }
            }
            None => {}
        };

        let nonce = zasynclock!(self.prng).gen_range(0..sn_resolution);

        // let init_syn_property = match zbuf.read_init_syn_property_multilink() {
        //     Some(isa) => isa,
        //     None => {
        //         let guard = zlock!(self.state);
        //         // if guard.authenticated.get()
        //         return zerror!(ZErrorKind::InvalidMessage {
        //             descr: format!("Received InitSyn with invalid attachment on link: {}", link),
        //         });
        //     }
        // };

        // if init_syn_property.version > MULTILINK_VERSION {
        //     return zerror!(ZErrorKind::InvalidMessage {
        //         descr: format!("Rejected InitSyn with invalid attachment on link: {}", link),
        //     });
        // }

        // // Create the InitAck attachment
        // let nonce = zasynclock!(self.prng).gen_range(0..sn_resolution);
        // let init_ack_property = InitAckProperty { nonce };
        // // Encode the InitAck property
        // let mut wbuf = WBuf::new(WBUF_SIZE, false);
        // wbuf.write_init_ack_property_multilink(&init_ack_property);
        // let zbuf: ZBuf = wbuf.into();
        // let prop = Property {
        //     key: PeerAuthenticatorId::MultiLink as ZInt,
        //     value: zbuf.to_vec(),
        // };

        // // Insert the nonce in the set of sent nonces
        // zasynclock!(self.nonces).insert((link.src.clone(), link.dst.clone()), (*peer_id, nonce));

        // let mut res = PeerAuthenticatorOutput::new();
        // res.properties.push(prop);
        // Ok(res)

        return zerror!(ZErrorKind::InvalidMessage {
            descr: format!("Rejected InitSyn with invalid attachment on link: {}", link),
        });
    }

    async fn handle_init_ack(
        &self,
        link: &AuthenticatedPeerLink,
        _peer_id: &PeerId,
        _sn_resolution: ZInt,
        properties: &[Property],
    ) -> ZResult<PeerAuthenticatorOutput> {
        // let mut res = PeerAuthenticatorOutput::new();
        // // If credentials are not configured, don't continue the MULTILINK authentication
        // let credentials = match self.credentials.as_ref() {
        //     Some(cr) => cr,
        //     None => return Ok(res),
        // };

        // let tmp = properties
        //     .iter()
        //     .find(|p| p.key == PeerAuthenticatorId::MultiLink as ZInt);
        // let mut zbuf: ZBuf = match tmp {
        //     Some(p) => p.value.clone().into(),
        //     None => {
        //         return zerror!(ZErrorKind::InvalidMessage {
        //             descr: format!("Received InitAck with no attachment on link: {}", link),
        //         });
        //     }
        // };
        // let init_ack_property = match zbuf.read_init_ack_property_multilink() {
        //     Some(isa) => isa,
        //     None => {
        //         return zerror!(ZErrorKind::InvalidMessage {
        //             descr: format!("Received InitAck with invalid attachment on link: {}", link),
        //         });
        //     }
        // };

        // // Create the HMAC of the password using the nonce received as a key (it's a challenge)
        // let key = init_ack_property.nonce.to_le_bytes();
        // let hmac = hmac::sign(&key, &credentials.password)?;
        // // Create the OpenSyn attachment
        // let open_syn_property = OpenSynProperty {
        //     user: credentials.user.clone(),
        //     hmac,
        // };
        // // Encode the InitAck attachment
        // let mut wbuf = WBuf::new(WBUF_SIZE, false);
        // wbuf.write_open_syn_property_multilink(&open_syn_property);
        // let zbuf: ZBuf = wbuf.into();
        // let prop = Property {
        //     key: PeerAuthenticatorId::MultiLink as ZInt,
        //     value: zbuf.to_vec(),
        // };
        // res.properties.push(prop);
        // Ok(res)

        return zerror!(ZErrorKind::InvalidMessage {
            descr: format!("Rejected InitSyn with invalid attachment on link: {}", link),
        });
    }

    async fn handle_open_syn(
        &self,
        link: &AuthenticatedPeerLink,
        properties: &[Property],
    ) -> ZResult<PeerAuthenticatorOutput> {
        // let (peer_id, nonce) =
        //     match zasynclock!(self.nonces).remove(&(link.src.clone(), link.dst.clone())) {
        //         Some(tuple) => tuple,
        //         None => {
        //             return zerror!(ZErrorKind::InvalidMessage {
        //                 descr: format!(
        //                     "Received OpenSyn but no nonce has been associated to link: {}",
        //                     link
        //                 ),
        //             });
        //         }
        //     };

        // let res = properties
        //     .iter()
        //     .find(|p| p.key == PeerAuthenticatorId::MultiLink as ZInt);
        // let mut zbuf: ZBuf = match res {
        //     Some(p) => p.value.clone().into(),
        //     None => {
        //         return zerror!(ZErrorKind::InvalidMessage {
        //             descr: format!("Received OpenSyn with no attachment on link: {}", link),
        //         });
        //     }
        // };
        // let open_syn_property = match zbuf.read_open_syn_property_multilink() {
        //     Some(osp) => osp,
        //     None => {
        //         return zerror!(ZErrorKind::InvalidMessage {
        //             descr: format!("Received InitAck with invalid attachment on link: {}", link),
        //         });
        //     }
        // };

        // let password = match zasyncread!(self.lookup).get(&open_syn_property.user) {
        //     Some(password) => password.clone(),
        //     None => {
        //         return zerror!(ZErrorKind::InvalidMessage {
        //             descr: format!("Received OpenSyn with invalid user on link: {}", link),
        //         });
        //     }
        // };

        // // Create the HMAC of the password using the nonce received as challenge
        // let key = nonce.to_le_bytes();
        // let hmac = hmac::sign(&key, &password)?;
        // if hmac != open_syn_property.hmac {
        //     return zerror!(ZErrorKind::InvalidMessage {
        //         descr: format!("Received OpenSyn with invalid password on link: {}", link),
        //     });
        // }

        // // Check PID validity
        // let mut guard = zasynclock!(self.authenticated);
        // match guard.get_mut(&peer_id) {
        //     Some(auth) => {
        //         if open_syn_property.user != auth.credentials.user
        //             || password != auth.credentials.password
        //         {
        //             return zerror!(ZErrorKind::InvalidMessage {
        //                 descr: format!("Received OpenSyn with invalid password on link: {}", link),
        //             });
        //         }
        //         auth.links.insert((link.src.clone(), link.dst.clone()));
        //     }
        //     None => {
        //         let credentials = Credentials {
        //             user: open_syn_property.user,
        //             password,
        //         };
        //         let mut links = HashSet::new();
        //         links.insert((link.src.clone(), link.dst.clone()));
        //         let auth = Authenticated { credentials, links };
        //         guard.insert(peer_id, auth);
        //     }
        // }

        Ok(PeerAuthenticatorOutput::new())
    }

    async fn handle_open_ack(
        &self,
        _link: &AuthenticatedPeerLink,
        _properties: &[Property],
    ) -> ZResult<PeerAuthenticatorOutput> {
        Ok(PeerAuthenticatorOutput::new())
    }

    async fn handle_link_err(&self, link: &AuthenticatedPeerLink) {
        // zasynclock!(self.nonces).remove(&(link.src.clone(), link.dst.clone()));

        // // Need to check if it authenticated and remove it if this is the last link
        // let mut guard = zasynclock!(self.authenticated);
        // let mut to_del: Option<PeerId> = None;
        // for (peer_id, auth) in guard.iter_mut() {
        //     auth.links.remove(&(link.src.clone(), link.dst.clone()));
        //     if auth.links.is_empty() {
        //         to_del = Some(*peer_id);
        //         break;
        //     }
        // }
        // if let Some(peer_id) = to_del.take() {
        //     guard.remove(&peer_id);
        // }
    }

    async fn handle_close(&self, peer_id: &PeerId) {
        zasyncwrite!(self.state).authenticated.remove(peer_id);
    }
}

impl From<Arc<MultiLinkAuthenticator>> for PeerAuthenticator {
    fn from(v: Arc<MultiLinkAuthenticator>) -> PeerAuthenticator {
        PeerAuthenticator(v)
    }
}

impl From<MultiLinkAuthenticator> for PeerAuthenticator {
    fn from(v: MultiLinkAuthenticator) -> PeerAuthenticator {
        Self::from(Arc::new(v))
    }
}
