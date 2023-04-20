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
use crate::unicast::establishment::Cookie;
use async_std::sync::Mutex;
use async_trait::async_trait;
use rand::SeedableRng;
use rsa::{
    pkcs1::{DecodeRsaPrivateKey, DecodeRsaPublicKey},
    BigUint, Pkcs1v15Encrypt, PublicKey, PublicKeyParts, RsaPrivateKey, RsaPublicKey,
};
use std::collections::HashMap;
use std::ops::Deref;
use std::path::Path;
use std::sync::Arc;
use zenoh_buffers::{
    reader::{DidntRead, HasReader, Reader},
    writer::{DidntWrite, HasWriter, Writer},
};
use zenoh_cfg_properties::config::ZN_AUTH_RSA_KEY_SIZE_DEFAULT;
use zenoh_codec::{RCodec, WCodec, Zenoh060};
use zenoh_config::Config;
use zenoh_core::{zasynclock, zparse};
use zenoh_crypto::PseudoRng;
use zenoh_protocol::core::{ZInt, ZenohId};
use zenoh_result::{bail, zerror, ZResult};

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
#[repr(transparent)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZPublicKey(RsaPublicKey);

impl Deref for ZPublicKey {
    type Target = RsaPublicKey;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<RsaPublicKey> for ZPublicKey {
    fn from(x: RsaPublicKey) -> Self {
        Self(x)
    }
}

#[repr(transparent)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZPrivateKey(RsaPrivateKey);

impl Deref for ZPrivateKey {
    type Target = RsaPrivateKey;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<RsaPrivateKey> for ZPrivateKey {
    fn from(x: RsaPrivateKey) -> Self {
        Self(x)
    }
}

impl<W> WCodec<&ZPublicKey, &mut W> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &ZPublicKey) -> Self::Output {
        self.write(&mut *writer, x.n().to_bytes_le().as_slice())?;
        self.write(&mut *writer, x.e().to_bytes_le().as_slice())?;
        Ok(())
    }
}

impl<R> RCodec<ZPublicKey, &mut R> for Zenoh060
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<ZPublicKey, Self::Error> {
        let n: Vec<u8> = self.read(&mut *reader)?;
        let n = BigUint::from_bytes_le(n.as_slice());
        let e: Vec<u8> = self.read(&mut *reader)?;
        let e = BigUint::from_bytes_le(e.as_slice());
        let rsa = RsaPublicKey::new(n, e).map_err(|_| DidntRead)?;

        Ok(ZPublicKey(rsa))
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
    alice_pubkey: ZPublicKey,
}

impl<W> WCodec<&InitSynProperty, &mut W> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &InitSynProperty) -> Self::Output {
        self.write(&mut *writer, x.version)?;
        self.write(&mut *writer, &x.alice_pubkey)?;
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
        let alice_pubkey: ZPublicKey = self.read(&mut *reader)?;
        Ok(InitSynProperty {
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
    bob_pubkey: ZPublicKey,
    nonce_encrypted_with_alice_pubkey: Vec<u8>,
}

impl<W> WCodec<&InitAckProperty, &mut W> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &InitAckProperty) -> Self::Output {
        self.write(&mut *writer, &x.bob_pubkey)?;
        self.write(&mut *writer, x.nonce_encrypted_with_alice_pubkey.as_slice())?;
        Ok(())
    }
}

impl<R> RCodec<InitAckProperty, &mut R> for Zenoh060
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<InitAckProperty, Self::Error> {
        let bob_pubkey: ZPublicKey = self.read(&mut *reader)?;
        let nonce_encrypted_with_alice_pubkey: Vec<u8> = self.read(&mut *reader)?;
        Ok(InitAckProperty {
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

impl<W> WCodec<&OpenSynProperty, &mut W> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &OpenSynProperty) -> Self::Output {
        self.write(&mut *writer, x.nonce_encrypted_with_bob_pubkey.as_slice())?;
        Ok(())
    }
}

impl<R> RCodec<OpenSynProperty, &mut R> for Zenoh060
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<OpenSynProperty, Self::Error> {
        let nonce_encrypted_with_bob_pubkey: Vec<u8> = self.read(&mut *reader)?;
        Ok(OpenSynProperty {
            nonce_encrypted_with_bob_pubkey,
        })
    }
}

/*************************************/
/*          Authenticator            */
/*************************************/
struct InnerState {
    prng: PseudoRng,
    known_keys: Option<Vec<ZPublicKey>>,
    authenticated: HashMap<ZenohId, Option<ZPublicKey>>,
}

pub struct PubKeyAuthenticator {
    pub_key: ZPublicKey,
    pri_key: ZPrivateKey,
    state: Mutex<InnerState>,
}

impl PubKeyAuthenticator {
    pub fn new<T, U>(pub_key: T, pri_key: U) -> PubKeyAuthenticator
    where
        T: Into<ZPublicKey>,
        U: Into<ZPrivateKey>,
    {
        PubKeyAuthenticator {
            pub_key: pub_key.into(),
            pri_key: pri_key.into(),
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
            pub_key: pub_key.into(),
            pri_key: pri_key.into(),
            state: Mutex::new(InnerState {
                prng,
                known_keys: None,
                authenticated: HashMap::new(),
            }),
        };
        Ok(pka)
    }

    pub async fn add_key(&self, key: ZPublicKey) -> ZResult<()> {
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

    pub async fn del_key(&self, key: &ZPublicKey) -> ZResult<()> {
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
        _peer_id: &ZenohId,
    ) -> ZResult<Option<Vec<u8>>> {
        let init_syn_property = InitSynProperty {
            version: MULTILINK_VERSION,
            alice_pubkey: self.pub_key.clone(),
        };

        let mut wbuf = vec![];
        let codec = Zenoh060::default();
        let mut writer = wbuf.writer();
        codec
            .write(&mut writer, &init_syn_property)
            .map_err(|_| zerror!("Error in encoding InitSyn for PubKey on link: {}", link))?;

        Ok(Some(wbuf))
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
                let mut reader = pk.reader();
                let codec = Zenoh060::default();

                let init_syn_property: InitSynProperty = codec.read(&mut reader).map_err(|_| {
                    zerror!(
                        "Received InitSyn with invalid PubKey attachment on link: {}",
                        link
                    )
                })?;

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
                                    bail!("Invalid multilink PubKey on link: {}", link);
                                }
                            }
                            None => {
                                // The peer is already present but no previous multilink intereset
                                // was declared. Rejecting for inconsistent declaration.
                                bail!("Unexpected multilink PubKey on link: {}", link);
                            }
                        }
                    }
                    None => {
                        // It's the first time we see this peer, check if it is authorized it
                        if let Some(kk) = guard.known_keys.as_ref() {
                            if !kk.iter().any(|x| x == &init_syn_property.alice_pubkey) {
                                // The peer is already present but no previous multilink intereset
                                // was declared. Rejecting for inconsistent declaration.
                                bail!("Unauthorized multilink PubKey on link: {}", link);
                            }
                        }

                        guard
                            .authenticated
                            .insert(cookie.zid, Some(init_syn_property.alice_pubkey.clone()));
                    }
                }

                // Create the InitAck attachment
                let codec = Zenoh060::default();

                let mut wbuf = vec![];
                let mut writer = wbuf.writer();
                codec.write(&mut writer, cookie.nonce).map_err(|_| {
                    zerror!("Error in encoding InitAck for PubKey on link: {}", link)
                })?;

                let nonce_bytes = wbuf;
                let nonce_encrypted_with_alice_pubkey = init_syn_property.alice_pubkey.encrypt(
                    &mut guard.prng,
                    Pkcs1v15Encrypt,
                    nonce_bytes.as_slice(),
                )?;

                let init_ack_property = InitAckProperty {
                    bob_pubkey: self.pub_key.clone(),
                    nonce_encrypted_with_alice_pubkey,
                };

                // Store the public key in the cookie
                let mut wbuf = vec![];
                let mut writer = wbuf.writer();
                codec
                    .write(&mut writer, &init_syn_property.alice_pubkey)
                    .map_err(|_| {
                        zerror!("Error in encoding InitAck for PubKey on link: {}", link)
                    })?;
                let cookie = wbuf;

                // Encode the InitAck property
                let mut wbuf = vec![];
                let mut writer = wbuf.writer();
                codec.write(&mut writer, &init_ack_property).map_err(|_| {
                    zerror!("Error in encoding InitAck for PubKey on link: {}", link)
                })?;
                let attachment = wbuf;

                Ok((Some(attachment), Some(cookie)))
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
        _peer_id: &ZenohId,
        _sn_resolution: ZInt,
        property: Option<Vec<u8>>,
    ) -> ZResult<Option<Vec<u8>>> {
        let pk = match property {
            Some(pk) => pk,
            None => return Ok(None),
        };

        let codec = Zenoh060::default();

        let mut reader = pk.reader();
        let init_ack_property: InitAckProperty = codec.read(&mut reader).map_err(|_| {
            zerror!(
                "Received InitAck with invalid PubKey attachment on link: {}",
                link
            )
        })?;
        let nonce = self.pri_key.decrypt(
            Pkcs1v15Encrypt,
            init_ack_property
                .nonce_encrypted_with_alice_pubkey
                .as_slice(),
        )?;

        // Create the OpenSyn attachment
        let mut guard = zasynclock!(self.state);
        let nonce_encrypted_with_bob_pubkey =
            init_ack_property
                .bob_pubkey
                .encrypt(&mut guard.prng, Pkcs1v15Encrypt, &nonce[..])?;
        drop(guard);

        let open_syn_property = OpenSynProperty {
            nonce_encrypted_with_bob_pubkey,
        };

        // Encode the OpenSyn property
        let mut wbuf = vec![];
        let mut writer = wbuf.writer();
        codec
            .write(&mut writer, &open_syn_property)
            .map_err(|_| zerror!("Error in encoding OpenSyn for PubKey on link: {}", link))?;
        let attachment = wbuf;

        Ok(Some(attachment))
    }

    async fn handle_open_syn(
        &self,
        link: &AuthenticatedPeerLink,
        cookie: &Cookie,
        property: (Option<Vec<u8>>, Option<Vec<u8>>),
    ) -> ZResult<Option<Vec<u8>>> {
        match property {
            (Some(att), Some(cke)) => {
                let codec = Zenoh060::default();

                let mut reader = att.reader();
                let open_syn_property: OpenSynProperty = codec.read(&mut reader).map_err(|_| {
                    zerror!(
                        "Received OpenSyn with invalid PubKey attachment on link: {}",
                        link
                    )
                })?;

                let nonce_bytes = self.pri_key.decrypt(
                    Pkcs1v15Encrypt,
                    open_syn_property.nonce_encrypted_with_bob_pubkey.as_slice(),
                )?;
                let mut reader = nonce_bytes.reader();
                let nonce: ZInt = codec.read(&mut reader).map_err(|_| {
                    zerror!(
                        "Received OpenSyn with invalid PubKey attachment on link: {}",
                        link
                    )
                })?;

                if nonce != cookie.nonce {
                    bail!("Received invalid nonce on link: {}", link);
                }

                let mut reader = cke.reader();
                let alice_pubkey: ZPublicKey = codec.read(&mut reader).map_err(|_| {
                    zerror!(
                        "Received OpenSyn with invalid PubKey attachment on link: {}",
                        link
                    )
                })?;

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
        if let Some(zid) = link.peer_id.as_ref() {
            zasynclock!(self.state).authenticated.remove(zid);
        }
    }

    async fn handle_close(&self, peer_id: &ZenohId) {
        zasynclock!(self.state).authenticated.remove(peer_id);
    }
}

//noinspection ALL
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
