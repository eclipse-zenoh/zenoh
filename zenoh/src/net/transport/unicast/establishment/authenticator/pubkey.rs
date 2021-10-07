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
use super::{PeerId, Property, WBuf, ZBuf, ZInt};
use crate::config::Config;
use crate::net::transport::unicast::establishment::Cookie;
use async_std::sync::{Arc, Mutex};
use async_trait::async_trait;
use rand::SeedableRng;
use rsa::{BigUint, PaddingScheme, PublicKey, PublicKeyParts, RsaPrivateKey, RsaPublicKey};
use std::collections::{HashMap, HashSet};
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::crypto::PseudoRng;
use zenoh_util::zasynclock;

const WBUF_SIZE: usize = 64;
const MULTILINK_VERSION: ZInt = 1;

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
        self.write_bytes_array(pub_key.n().to_bytes_le().as_slice())
            && self.write_bytes_array(pub_key.e().to_bytes_le().as_slice())
    }
}

impl ZBuf {
    fn read_rsa_pub_key(&mut self) -> Option<RsaPublicKey> {
        let n = BigUint::from_bytes_le(self.read_bytes_array()?.as_slice());
        let e = BigUint::from_bytes_le(self.read_bytes_array()?.as_slice());

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
    known_keys: Option<HashSet<RsaPublicKey>>,
    authenticated: HashMap<PeerId, Option<RsaPublicKey>>,
}

pub struct PubKeyAuthenticator {
    pub_key: RsaPublicKey,
    prv_key: RsaPrivateKey,
    state: Mutex<InnerState>,
}

impl PubKeyAuthenticator {
    pub fn new() -> PubKeyAuthenticator {
        let mut prng = PseudoRng::from_entropy();
        let bits = 512;
        let prv_key = RsaPrivateKey::new(&mut prng, bits)
            .map_err(|e| {
                zerror2!(ZErrorKind::Other {
                    descr: format!("Failed to create Rsa Private Key: {}", e)
                })
            })
            .unwrap(); // @TODO: avoid unwrap
        let pub_key = RsaPublicKey::from(&prv_key);

        PubKeyAuthenticator {
            pub_key,
            prv_key,
            state: Mutex::new(InnerState {
                prng,
                known_keys: None,
                authenticated: HashMap::new(),
            }),
        }
    }

    pub async fn add_key(&self, key: RsaPublicKey) -> ZResult<()> {
        let mut guard = zasynclock!(self.state);
        match guard.known_keys.as_mut() {
            Some(kk) => {
                kk.insert(key);
            }
            None => {
                let mut hs = HashSet::new();
                hs.insert(key);
                guard.known_keys = Some(hs);
            }
        }
        Ok(())
    }

    pub async fn del_key(&self, key: &RsaPublicKey) -> ZResult<()> {
        let mut guard = zasynclock!(self.state);
        if let Some(kk) = guard.known_keys.as_mut() {
            kk.remove(key);
        }
        Ok(())
    }

    pub async fn from_config(_config: &Config) -> ZResult<Option<PubKeyAuthenticator>> {
        // @TODO: support config
        Ok(None)
    }
}

impl Default for PubKeyAuthenticator {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PeerAuthenticatorTrait for PubKeyAuthenticator {
    fn id(&self) -> PeerAuthenticatorId {
        PeerAuthenticatorId::MultiLink
    }

    async fn get_init_syn_properties(
        &self,
        link: &AuthenticatedPeerLink,
        _peer_id: &PeerId,
    ) -> ZResult<PeerAuthenticatorOutput> {
        let mut output = PeerAuthenticatorOutput::new();
        let init_syn_property = InitSynProperty {
            version: MULTILINK_VERSION,
            alice_pubkey: self.pub_key.clone(),
        };

        let mut wbuf = WBuf::new(WBUF_SIZE, false);
        let res = wbuf.write_init_syn_property_multilink(&init_syn_property);
        if !res {
            let e = format!("Failed to serialize InitSyn on link: {}", link);
            return zerror!(ZErrorKind::InvalidMessage { descr: e });
        }

        let zbuf: ZBuf = wbuf.into();
        let prop = Property {
            key: PeerAuthenticatorId::MultiLink as ZInt,
            value: zbuf.to_vec(),
        };

        output.properties.push(prop);

        Ok(output)
    }

    async fn handle_init_syn(
        &self,
        link: &AuthenticatedPeerLink,
        cookie: &mut Cookie,
        properties: &[Property],
    ) -> ZResult<PeerAuthenticatorOutput> {
        let res = properties
            .iter()
            .find(|p| p.key == PeerAuthenticatorId::MultiLink as ZInt);

        match res {
            // The connecting zenoh peer wants to do multilink
            Some(mlp) => {
                // Decode the multilink attachment
                let mut zbuf: ZBuf = mlp.value.clone().into();

                let init_syn_property =
                    zbuf.read_init_syn_property_multilink().ok_or_else(|| {
                        let e = format!("Received invalid InitSyn on link: {}", link);
                        zerror2!(ZErrorKind::InvalidMessage { descr: e })
                    })?;

                // Check if we are compatible
                if init_syn_property.version != MULTILINK_VERSION {
                    let e = format!("PubKey version not supported on link: {}", link);
                    return zerror!(ZErrorKind::InvalidMessage { descr: e });
                }

                // Check if the peer is already present
                let mut guard = zasynclock!(self.state);
                match guard.authenticated.get(&cookie.pid) {
                    Some(alice_pubkey) => {
                        // Check if the public key is the same
                        match alice_pubkey.as_ref() {
                            Some(apk) => {
                                // Check if pub key is used consistently
                                if apk != &init_syn_property.alice_pubkey {
                                    let e = format!("Invalid multilink pub key on link: {}", link);
                                    return zerror!(ZErrorKind::InvalidMessage { descr: e });
                                }
                            }
                            None => {
                                // The peer is already present but no previous multilink intereset
                                // was declared. Rejecting for inconsistent declaration.
                                let e = format!("Unexpected multilink pub key on link: {}", link);
                                return zerror!(ZErrorKind::InvalidMessage { descr: e });
                            }
                        }
                    }
                    None => {
                        // It's the first time we see this peer, check if it is authorized it
                        if let Some(kk) = guard.known_keys.as_ref() {
                            if kk.get(&init_syn_property.alice_pubkey).is_none() {
                                // The peer is already present but no previous multilink intereset
                                // was declared. Rejecting for inconsistent declaration.
                                let e = format!("Unauthorized multilink pub key on link: {}", link);
                                return zerror!(ZErrorKind::InvalidMessage { descr: e });
                            }
                        }

                        guard
                            .authenticated
                            .insert(cookie.pid, Some(init_syn_property.alice_pubkey.clone()));
                    }
                }

                // Create the InitAck attachment
                let mut wbuf = WBuf::new(WBUF_SIZE, false);
                let res = wbuf.write_zint(cookie.nonce);
                if !res {
                    let e = format!("Failed to serialize InitAck on link: {}", link);
                    return zerror!(ZErrorKind::InvalidMessage { descr: e });
                }

                let nonce_bytes: ZBuf = wbuf.into();
                let nonce_encrypted_with_alice_pubkey = init_syn_property
                    .alice_pubkey
                    .encrypt(
                        &mut guard.prng,
                        PaddingScheme::PKCS1v15Encrypt,
                        nonce_bytes.contiguous().as_slice(),
                    )
                    .map_err(|e| {
                        zerror2!(ZErrorKind::Other {
                            descr: e.to_string()
                        })
                    })?;

                let init_ack_property = InitAckProperty {
                    bob_pubkey: self.pub_key.clone(),
                    nonce_encrypted_with_alice_pubkey,
                };

                // Encode the InitAck property
                let mut wbuf = WBuf::new(WBUF_SIZE, false);
                let res = wbuf.write_init_ack_property_multilink(&init_ack_property);
                if !res {
                    let e = format!("Failed to serialize InitAck on link: {}", link);
                    return zerror!(ZErrorKind::InvalidMessage { descr: e });
                }

                let zbuf: ZBuf = wbuf.into();
                let prop = Property {
                    key: PeerAuthenticatorId::MultiLink as ZInt,
                    value: zbuf.to_vec(),
                };

                // Encode the attachment
                let mut output = PeerAuthenticatorOutput::new();
                output.properties.insert(prop)?;

                // Store the public key in the cookie
                let mut wbuf = WBuf::new(WBUF_SIZE, false);
                let res = wbuf.write_rsa_pub_key(&init_syn_property.alice_pubkey);
                if !res {
                    let e = format!("Failed to serialize InitAck on link: {}", link);
                    return zerror!(ZErrorKind::InvalidMessage { descr: e });
                }

                let zbuf: ZBuf = wbuf.into();
                let prop = Property {
                    key: PeerAuthenticatorId::MultiLink as ZInt,
                    value: zbuf.to_vec(),
                };
                cookie.properties.insert(prop)?;

                Ok(output)
            }
            // The connecting zenoh peer does not want to do multilink
            None => {
                let mut guard = zasynclock!(self.state);
                match guard.authenticated.get(&cookie.pid) {
                    // Check if the peer is already present
                    Some(_) => {
                        // The peer is already present but no multilink intereset is declared.
                        // Rejecting for inconsistent declaration.
                        let e = format!("No multilink supported on link: {}", link);
                        return zerror!(ZErrorKind::InvalidMessage { descr: e });
                    }
                    None => {
                        // It's the first time we see this peer, insert it
                        guard.authenticated.insert(cookie.pid, None);
                    }
                }

                // No properties need to be included in the InitAck attachment
                Ok(PeerAuthenticatorOutput::new())
            }
        }
    }

    async fn handle_init_ack(
        &self,
        link: &AuthenticatedPeerLink,
        _peer_id: &PeerId,
        _sn_resolution: ZInt,
        properties: &[Property],
    ) -> ZResult<PeerAuthenticatorOutput> {
        let res = properties
            .iter()
            .find(|p| p.key == PeerAuthenticatorId::MultiLink as ZInt);

        let mut output = PeerAuthenticatorOutput::new();
        if let Some(mlp) = res {
            let mut zbuf: ZBuf = mlp.value.clone().into();
            let init_ack_property = zbuf.read_init_ack_property_multilink().ok_or_else(|| {
                let e = format!("Received invalid InitSyn on link: {}", link);
                zerror2!(ZErrorKind::InvalidMessage { descr: e })
            })?;

            let nonce = self
                .prv_key
                .decrypt(
                    PaddingScheme::PKCS1v15Encrypt,
                    init_ack_property
                        .nonce_encrypted_with_alice_pubkey
                        .as_slice(),
                )
                .map_err(|e| {
                    zerror2!(ZErrorKind::Other {
                        descr: e.to_string()
                    })
                })?;

            // Create the OpenSyn attachment
            let mut guard = zasynclock!(self.state);
            let nonce_encrypted_with_bob_pubkey = init_ack_property
                .bob_pubkey
                .encrypt(&mut guard.prng, PaddingScheme::PKCS1v15Encrypt, &nonce[..])
                .map_err(|e| {
                    zerror2!(ZErrorKind::Other {
                        descr: e.to_string()
                    })
                })?;
            drop(guard);

            let open_syn_property = OpenSynProperty {
                nonce_encrypted_with_bob_pubkey,
            };

            // Encode the OpenSyn property
            let mut wbuf = WBuf::new(WBUF_SIZE, false);
            let res = wbuf.write_open_syn_property_multilink(&open_syn_property);
            if !res {
                let e = format!("Failed to serialize OpenSyn on link: {}", link);
                return zerror!(ZErrorKind::InvalidMessage { descr: e });
            }

            let zbuf: ZBuf = wbuf.into();
            let prop = Property {
                key: PeerAuthenticatorId::MultiLink as ZInt,
                value: zbuf.to_vec(),
            };

            output.properties.insert(prop)?;
        }

        Ok(output)
    }

    async fn handle_open_syn(
        &self,
        link: &AuthenticatedPeerLink,
        properties: &[Property],
        cookie: &Cookie,
    ) -> ZResult<PeerAuthenticatorOutput> {
        let att = properties
            .iter()
            .find(|p| p.key == PeerAuthenticatorId::MultiLink as ZInt);

        let cke = cookie
            .properties
            .iter()
            .find(|p| p.key == PeerAuthenticatorId::MultiLink as ZInt);

        match (att, cke) {
            (Some(att), Some(cke)) => {
                let mut zbuf: ZBuf = att.value.clone().into();
                let open_syn_property =
                    zbuf.read_open_syn_property_multilink().ok_or_else(|| {
                        let e = format!("Received invalid InitSyn on link: {}", link);
                        zerror2!(ZErrorKind::InvalidMessage { descr: e })
                    })?;

                let mut nonce_bytes: ZBuf = self
                    .prv_key
                    .decrypt(
                        PaddingScheme::PKCS1v15Encrypt,
                        open_syn_property.nonce_encrypted_with_bob_pubkey.as_slice(),
                    )
                    .map_err(|e| {
                        zerror2!(ZErrorKind::Other {
                            descr: e.to_string()
                        })
                    })?
                    .into();
                let nonce = nonce_bytes.read_zint().ok_or_else(|| {
                    let e = format!("Received invalid InitSyn on link: {}", link);
                    zerror2!(ZErrorKind::InvalidMessage { descr: e })
                })?;

                if nonce != cookie.nonce {
                    let e = format!("Received invalid nonce on link: {}", link);
                    return zerror!(ZErrorKind::InvalidMessage { descr: e });
                }

                let mut zbuf: ZBuf = cke.value.clone().into();
                let alice_pubkey = zbuf.read_rsa_pub_key().ok_or_else(|| {
                    let e = format!("Received invalid InitSyn on link: {}", link);
                    zerror2!(ZErrorKind::InvalidMessage { descr: e })
                })?;

                let mut guard = zasynclock!(self.state);
                match guard.authenticated.get(&cookie.pid) {
                    Some(apk) => match apk {
                        Some(apk) => {
                            // Check if the public key is still correct
                            if apk != &alice_pubkey {
                                let e = format!("Invalid multilink pub key on link: {}", link);
                                return zerror!(ZErrorKind::InvalidMessage { descr: e });
                            }
                        }
                        None => {
                            // The peer did not previously express interest in multilink
                            let e = format!("Invalid multilink pub key on link: {}", link);
                            return zerror!(ZErrorKind::InvalidMessage { descr: e });
                        }
                    },
                    None => {
                        // Finally store the public key
                        guard.authenticated.insert(cookie.pid, Some(alice_pubkey));
                    }
                }
            }
            (None, None) => {}
            _ => {
                let e = format!("Received invalid nonce on link: {}", link);
                return zerror!(ZErrorKind::InvalidMessage { descr: e });
            }
        }

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
        // Need to check if it authenticated and remove it if this is the last link
        if let Some(pid) = link.peer_id.as_ref() {
            zasynclock!(self.state).authenticated.remove(pid);
        }
    }

    async fn handle_close(&self, peer_id: &PeerId) {
        zasynclock!(self.state).authenticated.remove(peer_id);
    }
}

impl From<Arc<PubKeyAuthenticator>> for PeerAuthenticator {
    fn from(v: Arc<PubKeyAuthenticator>) -> PeerAuthenticator {
        PeerAuthenticator(v)
    }
}

impl From<PubKeyAuthenticator> for PeerAuthenticator {
    fn from(v: PubKeyAuthenticator) -> PeerAuthenticator {
        Self::from(Arc::new(v))
    }
}
