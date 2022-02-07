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
    AuthenticatedPeerLink, PeerAuthenticator, PeerAuthenticatorId, PeerAuthenticatorTrait,
};
use super::{WBuf, ZBuf, ZInt, ZenohId};
use crate::config::Config;
use crate::net::protocol::core::SeqNumBytes;
use crate::net::transport::unicast::establishment::Cookie;
use async_std::sync::{Arc, Mutex};
use async_trait::async_trait;
use rand::SeedableRng;
use rsa::pkcs1::{FromRsaPrivateKey, FromRsaPublicKey};
use rsa::{BigUint, PaddingScheme, PublicKey, PublicKeyParts, RsaPrivateKey, RsaPublicKey};
use std::collections::HashMap;
use std::path::Path;
use zenoh_util::core::Result as ZResult;
use zenoh_util::crypto::PseudoRng;
use zenoh_util::properties::config::ZN_AUTH_RSA_KEY_SIZE_DEFAULT;
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
    known_keys: Option<Vec<RsaPublicKey>>,
    authenticated: HashMap<ZenohId, Option<RsaPublicKey>>,
}

pub struct PubKeyAuthenticator {
    pub_key: RsaPublicKey,
    pri_key: RsaPrivateKey,
    state: Mutex<InnerState>,
}

impl PubKeyAuthenticator {
    pub fn new(pub_key: RsaPublicKey, pri_key: RsaPrivateKey) -> PubKeyAuthenticator {
        PubKeyAuthenticator {
            pub_key,
            pri_key,
            state: Mutex::new(InnerState {
                prng: PseudoRng::from_entropy(),
                known_keys: None,
                authenticated: HashMap::new(),
            }),
        }
    }

    pub fn make() -> ZResult<PubKeyAuthenticator> {
        let mut prng = PseudoRng::from_entropy();
        let bits = zparse!(ZN_AUTH_RSA_KEY_SIZE_DEFAULT)?;
        let pri_key = RsaPrivateKey::new(&mut prng, bits)?;
        let pub_key = RsaPublicKey::from(&pri_key);

        let pka = PubKeyAuthenticator {
            pub_key,
            pri_key,
            state: Mutex::new(InnerState {
                prng,
                known_keys: None,
                authenticated: HashMap::new(),
            }),
        };
        Ok(pka)
    }

    pub async fn add_key(&self, key: RsaPublicKey) -> ZResult<()> {
        let mut guard = zasynclock!(self.state);
        match guard.known_keys.as_mut() {
            Some(kk) => {
                if !kk.iter().any(|x| x == &key) {
                    kk.push(key);
                }
            }
            None => {
                let hs = vec![key];
                guard.known_keys = Some(hs);
            }
        }
        Ok(())
    }

    pub async fn del_key(&self, key: &RsaPublicKey) -> ZResult<()> {
        let mut guard = zasynclock!(self.state);
        if let Some(kk) = guard.known_keys.as_mut() {
            if let Some(i) = kk.iter().position(|x| x == key) {
                kk.remove(i);
            }
        }
        Ok(())
    }

    pub async fn from_config(config: &Config) -> ZResult<Option<PubKeyAuthenticator>> {
        let c = config.transport().auth().pubkey();

        // @TODO: support PubKey keys import

        // First, check if PEM keys are provided
        match (c.public_key_pem(), c.private_key_pem()) {
            (Some(public), Some(private)) => {
                let pub_key = RsaPublicKey::from_pkcs1_pem(public)
                    .map_err(|e| zerror!("Rsa Public Key: {}", e))?;
                let pri_key = RsaPrivateKey::from_pkcs1_pem(private)
                    .map_err(|e| zerror!("Rsa Private Key: {}", e))?;
                return Ok(Some(Self::new(pub_key, pri_key)));
            }
            (Some(_), None) => {
                bail!("Missing Rsa Private Key: PEM")
            }
            (None, Some(_)) => {
                bail!("Missing Rsa Public Key: PEM")
            }
            (None, None) => {}
        }

        // Second, check if PEM files are provided
        match (c.public_key_file(), c.private_key_file()) {
            (Some(public), Some(private)) => {
                let path = Path::new(public);
                let pub_key = RsaPublicKey::read_pkcs1_pem_file(path)
                    .map_err(|e| zerror!("Rsa Public Key: {}", e))?;
                let path = Path::new(private);
                let pri_key = RsaPrivateKey::read_pkcs1_pem_file(path)
                    .map_err(|e| zerror!("Rsa Private Key: {}", e))?;
                return Ok(Some(Self::new(pub_key, pri_key)));
            }
            (Some(_), None) => {
                bail!("Missing Rsa Private Key: file")
            }
            (None, Some(_)) => {
                bail!("Missing Rsa Public Key: file")
            }
            (None, None) => {}
        }

        Ok(None)
    }
}

#[async_trait]
impl PeerAuthenticatorTrait for PubKeyAuthenticator {
    fn id(&self) -> PeerAuthenticatorId {
        PeerAuthenticatorId::PublicKey
    }

    async fn close(&self) {
        // No cleanup needed
    }

    async fn get_init_syn_properties(
        &self,
        link: &AuthenticatedPeerLink,
        _zid: &ZenohId,
    ) -> ZResult<Option<Vec<u8>>> {
        let init_syn_property = InitSynProperty {
            version: MULTILINK_VERSION,
            alice_pubkey: self.pub_key.clone(),
        };

        let mut wbuf = WBuf::new(WBUF_SIZE, false);
        let res = wbuf.write_init_syn_property_multilink(&init_syn_property);
        if !res {
            bail!("Failed to serialize InitSyn on link: {}", link);
        }

        let attachment: ZBuf = wbuf.into();
        Ok(Some(attachment.to_vec()))
    }

    async fn handle_init_syn(
        &self,
        link: &AuthenticatedPeerLink,
        cookie: &Cookie,
        property: Option<Vec<u8>>,
    ) -> ZResult<(Option<Vec<u8>>, Option<Vec<u8>>)> {
        match property {
            // The connecting zenoh peer wants to do multilink
            Some(pk) => {
                // Decode the multilink attachment
                let mut zbuf: ZBuf = pk.into();

                let init_syn_property = zbuf
                    .read_init_syn_property_multilink()
                    .ok_or_else(|| zerror!("Received invalid InitSyn on link: {}", link))?;

                // Check if we are compatible
                if init_syn_property.version != MULTILINK_VERSION {
                    bail!("PubKey version not supported on link: {}", link);
                }

                // Check if the peer is already present
                let mut guard = zasynclock!(self.state);
                match guard.authenticated.get(&cookie.zid) {
                    Some(alice_pubkey) => {
                        // Check if the public key is the same
                        match alice_pubkey.as_ref() {
                            Some(apk) => {
                                // Check if pub key is used consistently
                                if apk != &init_syn_property.alice_pubkey {
                                    bail!("Invalid multilink pub key on link: {}", link);
                                }
                            }
                            None => {
                                // The peer is already present but no previous multilink intereset
                                // was declared. Rejecting for inconsistent declaration.
                                bail!("Unexpected multilink pub key on link: {}", link);
                            }
                        }
                    }
                    None => {
                        // It's the first time we see this peer, check if it is authorized it
                        if let Some(kk) = guard.known_keys.as_ref() {
                            if !kk.iter().any(|x| x == &init_syn_property.alice_pubkey) {
                                // The peer is already present but no previous multilink intereset
                                // was declared. Rejecting for inconsistent declaration.
                                bail!("Unauthorized multilink pub key on link: {}", link);
                            }
                        }

                        guard
                            .authenticated
                            .insert(cookie.zid, Some(init_syn_property.alice_pubkey.clone()));
                    }
                }

                // Create the InitAck attachment
                let mut wbuf = WBuf::new(WBUF_SIZE, false);
                let res = wbuf.write_zint(cookie.nonce);
                if !res {
                    bail!("Failed to serialize InitAck on link: {}", link);
                }

                let nonce_bytes: ZBuf = wbuf.into();
                let nonce_encrypted_with_alice_pubkey = init_syn_property.alice_pubkey.encrypt(
                    &mut guard.prng,
                    PaddingScheme::PKCS1v15Encrypt,
                    nonce_bytes.contiguous().as_slice(),
                )?;

                let init_ack_property = InitAckProperty {
                    bob_pubkey: self.pub_key.clone(),
                    nonce_encrypted_with_alice_pubkey,
                };

                // Store the public key in the cookie
                let mut wbuf = WBuf::new(WBUF_SIZE, false);
                let res = wbuf.write_rsa_pub_key(&init_syn_property.alice_pubkey);
                if !res {
                    bail!("Failed to serialize InitAck on link: {}", link);
                }
                let cookie: ZBuf = wbuf.into();

                // Encode the InitAck property
                let mut wbuf = WBuf::new(WBUF_SIZE, false);
                let res = wbuf.write_init_ack_property_multilink(&init_ack_property);
                if !res {
                    bail!("Failed to serialize InitAck on link: {}", link);
                }
                let attachment: ZBuf = wbuf.into();

                Ok((Some(attachment.to_vec()), Some(cookie.to_vec())))
            }
            // The connecting zenoh peer does not want to do multilink
            None => {
                let guard = zasynclock!(self.state);
                if guard.authenticated.get(&cookie.zid).is_some() {
                    // The peer is already present but no multilink intereset is declared.
                    // Rejecting for inconsistent declaration.
                    bail!("No multilink supported on link: {}", link);
                }

                // No properties need to be included in the InitAck attachment
                Ok((None, None))
            }
        }
    }

    async fn handle_init_ack(
        &self,
        link: &AuthenticatedPeerLink,
        _zid: &ZenohId,
        _sn_bytes: SeqNumBytes,
        property: Option<Vec<u8>>,
    ) -> ZResult<Option<Vec<u8>>> {
        let pk = match property {
            Some(pk) => pk,
            None => return Ok(None),
        };

        let mut zbuf: ZBuf = pk.into();
        let init_ack_property = zbuf
            .read_init_ack_property_multilink()
            .ok_or_else(|| zerror!("Received invalid InitSyn on link: {}", link))?;

        let nonce = self.pri_key.decrypt(
            PaddingScheme::PKCS1v15Encrypt,
            init_ack_property
                .nonce_encrypted_with_alice_pubkey
                .as_slice(),
        )?;

        // Create the OpenSyn attachment
        let mut guard = zasynclock!(self.state);
        let nonce_encrypted_with_bob_pubkey = init_ack_property.bob_pubkey.encrypt(
            &mut guard.prng,
            PaddingScheme::PKCS1v15Encrypt,
            &nonce[..],
        )?;
        drop(guard);

        let open_syn_property = OpenSynProperty {
            nonce_encrypted_with_bob_pubkey,
        };

        // Encode the OpenSyn property
        let mut wbuf = WBuf::new(WBUF_SIZE, false);
        let res = wbuf.write_open_syn_property_multilink(&open_syn_property);
        if !res {
            bail!("Failed to serialize OpenSyn on link: {}", link);
        }

        let attachment: ZBuf = wbuf.into();

        Ok(Some(attachment.to_vec()))
    }

    async fn handle_open_syn(
        &self,
        link: &AuthenticatedPeerLink,
        cookie: &Cookie,
        property: (Option<Vec<u8>>, Option<Vec<u8>>),
    ) -> ZResult<Option<Vec<u8>>> {
        match property {
            (Some(att), Some(cke)) => {
                let mut zbuf: ZBuf = att.into();
                let open_syn_property = zbuf
                    .read_open_syn_property_multilink()
                    .ok_or_else(|| zerror!("Received invalid InitSyn on link: {}", link))?;

                let mut nonce_bytes: ZBuf = self
                    .pri_key
                    .decrypt(
                        PaddingScheme::PKCS1v15Encrypt,
                        open_syn_property.nonce_encrypted_with_bob_pubkey.as_slice(),
                    )?
                    .into();
                let nonce = nonce_bytes
                    .read_zint()
                    .ok_or_else(|| zerror!("Received invalid InitSyn on link: {}", link))?;

                if nonce != cookie.nonce {
                    bail!("Received invalid nonce on link: {}", link);
                }

                let mut zbuf: ZBuf = cke.into();
                let alice_pubkey = zbuf
                    .read_rsa_pub_key()
                    .ok_or_else(|| zerror!("Received invalid InitSyn on link: {}", link))?;

                let mut guard = zasynclock!(self.state);
                match guard.authenticated.get(&cookie.zid) {
                    Some(apk) => match apk {
                        Some(apk) => {
                            // Check if the public key is still correct
                            if apk != &alice_pubkey {
                                bail!("Invalid multilink pub key on link: {}", link);
                            }
                        }
                        None => {
                            // The peer did not previously express interest in multilink
                            bail!("Invalid multilink pub key on link: {}", link);
                        }
                    },
                    None => {
                        // Finally store the public key
                        guard.authenticated.insert(cookie.zid, Some(alice_pubkey));
                    }
                }
            }
            (None, None) => {
                // No multilink
                let mut guard = zasynclock!(self.state);
                if guard.authenticated.get(&cookie.zid).is_some() {
                    // The peer did not previously express interest in multilink
                    bail!("Invalid multilink pub key on link: {}", link);
                }
                // Finally store the public key
                guard.authenticated.insert(cookie.zid, None);
            }
            _ => {
                bail!("Received invalid nonce on link: {}", link);
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
        if let Some(zid) = link.zid.as_ref() {
            zasynclock!(self.state).authenticated.remove(zid);
        }
    }

    async fn handle_close(&self, zid: &ZenohId) {
        zasynclock!(self.state).authenticated.remove(zid);
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
