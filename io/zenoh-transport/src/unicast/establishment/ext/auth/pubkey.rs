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
use std::{collections::HashSet, fmt, ops::Deref, path::Path};

use async_trait::async_trait;
use rand::Rng;
use rsa::{
    pkcs1::{DecodeRsaPrivateKey, DecodeRsaPublicKey},
    traits::PublicKeyParts,
    BigUint, Pkcs1v15Encrypt, RsaPrivateKey, RsaPublicKey,
};
use tokio::sync::{Mutex, RwLock};
use zenoh_buffers::{
    reader::{DidntRead, HasReader, Reader},
    writer::{DidntWrite, HasWriter, Writer},
};
use zenoh_codec::{RCodec, WCodec, Zenoh080};
use zenoh_config::PubKeyConf;
use zenoh_core::{bail, zasynclock, zasyncread, zerror, Error as ZError, Result as ZResult};
use zenoh_crypto::PseudoRng;
use zenoh_protocol::common::{ZExtUnit, ZExtZBuf};

use crate::unicast::establishment::{ext::auth::id, AcceptFsm, OpenFsm};

mod ext {
    use zenoh_protocol::{zextunit, zextzbuf};

    use super::{id::PUBKEY, ZExtUnit, ZExtZBuf};

    pub(super) type InitSyn = zextzbuf!(PUBKEY, false);
    pub(super) type InitAck = zextzbuf!(PUBKEY, false);
    pub(super) type OpenSyn = zextzbuf!(PUBKEY, false);
    pub(super) type OpenAck = zextunit!(PUBKEY, false);
}

// Authenticator
#[derive(Debug)]
pub struct AuthPubKey {
    lookup: Option<HashSet<ZPublicKey>>,
    pub_key: ZPublicKey,
    pri_key: ZPrivateKey,
}

impl AuthPubKey {
    pub fn new(pub_key: ZPublicKey, pri_key: ZPrivateKey) -> Self {
        Self {
            lookup: Some(HashSet::new()),
            pub_key,
            pri_key,
        }
    }

    pub(crate) fn disable_lookup(&mut self) {
        self.lookup = None;
    }

    pub async fn add_pubkey(&mut self, pub_key: ZPublicKey) -> ZResult<()> {
        if let Some(lookup) = self.lookup.as_mut() {
            lookup.insert(pub_key);
        }
        Ok(())
    }

    pub async fn del_pubkey(&mut self, pub_key: &ZPublicKey) -> ZResult<()> {
        if let Some(lookup) = self.lookup.as_mut() {
            lookup.remove(pub_key);
        }
        Ok(())
    }

    pub fn from_config(config: &PubKeyConf) -> ZResult<Option<Self>> {
        const S: &str = "PubKey extension - From config.";

        // First, check if PEM keys are provided
        match (config.public_key_pem(), config.private_key_pem()) {
            (Some(public), Some(private)) => {
                let pub_key = RsaPublicKey::from_pkcs1_pem(public)
                    .map_err(|e| zerror!("{} Rsa Public Key: {}.", S, e))?;
                let pri_key = RsaPrivateKey::from_pkcs1_pem(private)
                    .map_err(|e| zerror!("{} Rsa Private Key: {}.", S, e))?;
                return Ok(Some(Self::new(pub_key.into(), pri_key.into())));
            }
            (Some(_), None) => {
                bail!("{S} Missing Rsa Private Key: PEM.")
            }
            (None, Some(_)) => {
                bail!("{S} Missing Rsa Public Key: PEM.")
            }
            (None, None) => {}
        }

        // Second, check if PEM files are provided
        match (config.public_key_file(), config.private_key_file()) {
            (Some(public), Some(private)) => {
                let path = Path::new(public);
                let pub_key = RsaPublicKey::read_pkcs1_pem_file(path)
                    .map_err(|e| zerror!("{} Rsa Public Key: {}.", S, e))?;
                let path = Path::new(private);
                let pri_key = RsaPrivateKey::read_pkcs1_pem_file(path)
                    .map_err(|e| zerror!("{} Rsa Private Key: {}.", S, e))?;
                return Ok(Some(Self::new(pub_key.into(), pri_key.into())));
            }
            (Some(_), None) => {
                bail!("{S} Missing Rsa Private Key: file.")
            }
            (None, Some(_)) => {
                bail!("{S} Missing Rsa Public Key: file.")
            }
            (None, None) => {}
        }

        // @TODO: populate lookup file

        Ok(None)
    }
}

#[repr(transparent)]
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct ZPublicKey(RsaPublicKey);

impl Deref for ZPublicKey {
    type Target = RsaPublicKey;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl fmt::Debug for ZPublicKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for b in self.0.n().to_bytes_le() {
            write!(f, "{b:02x}")?;
        }
        for b in self.0.e().to_bytes_le() {
            write!(f, "{b:02x}")?;
        }
        Ok(())
    }
}

impl From<RsaPublicKey> for ZPublicKey {
    fn from(x: RsaPublicKey) -> Self {
        Self(x)
    }
}

#[repr(transparent)]
#[derive(Clone, PartialEq, Eq)]
pub struct ZPrivateKey(RsaPrivateKey);

impl Deref for ZPrivateKey {
    type Target = RsaPrivateKey;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl fmt::Debug for ZPrivateKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "***",)
    }
}

impl From<RsaPrivateKey> for ZPrivateKey {
    fn from(x: RsaPrivateKey) -> Self {
        Self(x)
    }
}

impl<W> WCodec<&ZPublicKey, &mut W> for Zenoh080
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

impl<R> RCodec<ZPublicKey, &mut R> for Zenoh080
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
/// ```text
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// ~  public key   ~
/// +---------------+
///
/// ZExtZBuf
/// ```
pub(crate) struct InitSyn {
    pub(crate) alice_pubkey: ZPublicKey,
}

impl<W> WCodec<&InitSyn, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &InitSyn) -> Self::Output {
        self.write(&mut *writer, &x.alice_pubkey)?;
        Ok(())
    }
}

impl<R> RCodec<InitSyn, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<InitSyn, Self::Error> {
        let alice_pubkey: ZPublicKey = self.read(&mut *reader)?;
        Ok(InitSyn { alice_pubkey })
    }
}

/*************************************/
/*             InitAck               */
/*************************************/
/// ```text
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// ~  public key   ~
/// +---------------+
/// ~ ciphered nonce~
/// +---------------+
///
/// ZExtZBuf
/// ```
pub(crate) struct InitAck {
    pub(crate) bob_pubkey: ZPublicKey,
    pub(crate) nonce_encrypted_with_alice_pubkey: Vec<u8>,
}

impl<W> WCodec<&InitAck, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &InitAck) -> Self::Output {
        self.write(&mut *writer, &x.bob_pubkey)?;
        self.write(&mut *writer, x.nonce_encrypted_with_alice_pubkey.as_slice())?;
        Ok(())
    }
}

impl<R> RCodec<InitAck, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<InitAck, Self::Error> {
        let bob_pubkey: ZPublicKey = self.read(&mut *reader)?;
        let nonce_encrypted_with_alice_pubkey: Vec<u8> = self.read(&mut *reader)?;
        Ok(InitAck {
            bob_pubkey,
            nonce_encrypted_with_alice_pubkey,
        })
    }
}

/*************************************/
/*             OpenSyn               */
/*************************************/
/// ```text
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// ~ ciphered nonce~
/// +---------------+
///
/// ZExtZBuf
/// ```
pub(crate) struct OpenSyn {
    pub(crate) nonce_encrypted_with_bob_pubkey: Vec<u8>,
}

impl<W> WCodec<&OpenSyn, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &OpenSyn) -> Self::Output {
        self.write(&mut *writer, x.nonce_encrypted_with_bob_pubkey.as_slice())?;
        Ok(())
    }
}

impl<R> RCodec<OpenSyn, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<OpenSyn, Self::Error> {
        let nonce_encrypted_with_bob_pubkey: Vec<u8> = self.read(&mut *reader)?;
        Ok(OpenSyn {
            nonce_encrypted_with_bob_pubkey,
        })
    }
}

pub(crate) struct AuthPubKeyFsm<'a> {
    inner: &'a RwLock<AuthPubKey>,
    prng: &'a Mutex<PseudoRng>,
}

impl<'a> AuthPubKeyFsm<'a> {
    pub(crate) const fn new(inner: &'a RwLock<AuthPubKey>, prng: &'a Mutex<PseudoRng>) -> Self {
        Self { inner, prng }
    }
}

/*************************************/
/*              OPEN                 */
/*************************************/
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct StateOpen {
    nonce: Vec<u8>,
}

impl StateOpen {
    pub(crate) const fn new() -> Self {
        Self { nonce: vec![] }
    }
}

#[async_trait]
impl<'a> OpenFsm for &'a AuthPubKeyFsm<'a> {
    type Error = ZError;

    type SendInitSynIn = &'a StateOpen;
    type SendInitSynOut = Option<ext::InitSyn>;
    async fn send_init_syn(
        self,
        _input: Self::SendInitSynIn,
    ) -> Result<Self::SendInitSynOut, Self::Error> {
        const S: &str = "PubKey extension - Send InitSyn.";
        tracing::trace!("{S}");

        let init_syn = InitSyn {
            alice_pubkey: zasyncread!(self.inner).pub_key.clone(),
        };

        let codec = Zenoh080::new();
        let mut buff = vec![];
        let mut writer = buff.writer();
        codec
            .write(&mut writer, &init_syn)
            .map_err(|_| zerror!("{S} Encoding error."))?;

        Ok(Some(ZExtZBuf::new(buff.into())))
    }

    type RecvInitAckIn = (&'a mut StateOpen, Option<ext::InitAck>);
    type RecvInitAckOut = ();
    async fn recv_init_ack(
        self,
        input: Self::RecvInitAckIn,
    ) -> Result<Self::RecvInitAckOut, Self::Error> {
        const S: &str = "PubKey extension - Recv InitAck.";
        tracing::trace!("{S}");

        let (state, mut ext) = input;

        let ext = ext
            .take()
            .ok_or_else(|| zerror!("{S} Missing PubKey extension."))?;

        let codec = Zenoh080::new();
        let mut reader = ext.value.reader();
        let init_ack: InitAck = codec
            .read(&mut reader)
            .map_err(|_| zerror!("{S} Decoding error."))?;

        let r_inner = zasyncread!(self.inner);
        if let Some(lookup) = r_inner.lookup.as_ref() {
            if !lookup.is_empty() && !lookup.contains(&init_ack.bob_pubkey) {
                bail!("{S} Unauthorized PubKey.");
            }
        }

        let mut prng = zasynclock!(self.prng);
        let nonce = r_inner
            .pri_key
            .decrypt_blinded(
                &mut *prng,
                Pkcs1v15Encrypt,
                init_ack.nonce_encrypted_with_alice_pubkey.as_slice(),
            )
            .map_err(|_| zerror!("{S} Decryption error."))?;
        drop(r_inner);

        state.nonce = init_ack
            .bob_pubkey
            .encrypt(&mut *prng, Pkcs1v15Encrypt, nonce.as_slice())?;

        Ok(())
    }

    type SendOpenSynIn = &'a StateOpen;
    type SendOpenSynOut = Option<ext::OpenSyn>;
    async fn send_open_syn(
        self,
        state: Self::SendOpenSynIn,
    ) -> Result<Self::SendOpenSynOut, Self::Error> {
        const S: &str = "PubKey extension - Send OpenSyn.";
        tracing::trace!("{S}");

        let open_syn = OpenSyn {
            nonce_encrypted_with_bob_pubkey: state.nonce.clone(),
        };

        let codec = Zenoh080::new();
        let mut buff = vec![];
        let mut writer = buff.writer();
        codec
            .write(&mut writer, &open_syn)
            .map_err(|_| zerror!("{S} Encoding error."))?;

        Ok(Some(ZExtZBuf::new(buff.into())))
    }

    type RecvOpenAckIn = (&'a mut StateOpen, Option<ext::OpenAck>);
    type RecvOpenAckOut = ();
    async fn recv_open_ack(
        self,
        input: Self::RecvOpenAckIn,
    ) -> Result<Self::RecvOpenAckOut, Self::Error> {
        const S: &str = "PubKey extension - Recv OpenAck.";

        let (_, ext) = input;
        if ext.is_none() {
            bail!("{S} Expected extension.");
        }

        Ok(())
    }
}

/*************************************/
/*            ACCEPT                 */
/*************************************/

#[derive(Debug)]
pub(crate) struct StateAccept {
    nonce: Vec<u8>,
    challenge: u64,
}

impl StateAccept {
    pub(crate) const fn new() -> Self {
        Self {
            nonce: vec![],
            challenge: 0,
        }
    }

    #[cfg(all(test, feature = "test"))]
    pub(crate) fn rand() -> Self {
        let mut rng = rand::thread_rng();
        let mut nonce = vec![0u8; rng.gen_range(0..=64)];
        rng.fill(&mut nonce[..]);
        Self {
            nonce,
            challenge: rng.gen(),
        }
    }
}

// Codec
impl<W> WCodec<&StateAccept, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &StateAccept) -> Self::Output {
        self.write(&mut *writer, x.challenge)
    }
}

impl<R> RCodec<StateAccept, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<StateAccept, Self::Error> {
        let challenge: u64 = self.read(&mut *reader)?;
        Ok(StateAccept {
            nonce: vec![],
            challenge,
        })
    }
}

impl PartialEq for StateAccept {
    fn eq(&self, other: &Self) -> bool {
        self.challenge == other.challenge
    }
}

#[async_trait]
impl<'a> AcceptFsm for &'a AuthPubKeyFsm<'a> {
    type Error = ZError;

    type RecvInitSynIn = (&'a mut StateAccept, Option<ext::InitSyn>);
    type RecvInitSynOut = ();
    async fn recv_init_syn(
        self,
        input: Self::RecvInitSynIn,
    ) -> Result<Self::RecvInitSynOut, Self::Error> {
        const S: &str = "PubKey extension - Recv InitSyn.";
        tracing::trace!("{S}");

        let (state, mut ext) = input;

        let ext = ext
            .take()
            .ok_or_else(|| zerror!("{S} Missing PubKey extension."))?;

        let codec = Zenoh080::new();
        let mut reader = ext.value.reader();
        let init_syn: InitSyn = codec
            .read(&mut reader)
            .map_err(|_| zerror!("{S} Decoding error."))?;

        let r_inner = zasyncread!(self.inner);
        if let Some(lookup) = r_inner.lookup.as_ref() {
            if !lookup.contains(&init_syn.alice_pubkey) {
                bail!("{S} Unauthorized PubKey.");
            }
        }

        let mut prng = zasynclock!(self.prng);
        state.challenge = prng.gen();
        state.nonce = init_syn
            .alice_pubkey
            .encrypt(&mut *prng, Pkcs1v15Encrypt, &state.challenge.to_le_bytes())
            .map_err(|_| zerror!("{S} Encoding error."))?;

        Ok(())
    }

    type SendInitAckIn = &'a StateAccept;
    type SendInitAckOut = Option<ext::InitAck>;
    async fn send_init_ack(
        self,
        state: Self::SendInitAckIn,
    ) -> Result<Self::SendInitAckOut, Self::Error> {
        const S: &str = "PubKey extension - Send InitAck.";
        tracing::trace!("{S}");

        let init_ack = InitAck {
            bob_pubkey: zasyncread!(self.inner).pub_key.clone(),
            nonce_encrypted_with_alice_pubkey: state.nonce.clone(),
        };

        let codec = Zenoh080::new();
        let mut buff = vec![];
        let mut writer = buff.writer();
        codec
            .write(&mut writer, &init_ack)
            .map_err(|_| zerror!("{S} Encoding error."))?;

        Ok(Some(ZExtZBuf::new(buff.into())))
    }

    type RecvOpenSynIn = (&'a mut StateAccept, Option<ext::OpenSyn>);
    type RecvOpenSynOut = ();
    async fn recv_open_syn(
        self,
        input: Self::RecvOpenSynIn,
    ) -> Result<Self::RecvOpenSynOut, Self::Error> {
        const S: &str = "PubKey extension - Recv OpenSyn.";
        tracing::trace!("{S}");

        let (state, mut ext) = input;

        let ext = ext
            .take()
            .ok_or_else(|| zerror!("{S} Missing PubKey extension."))?;

        let codec = Zenoh080::new();
        let mut reader = ext.value.reader();
        let open_syn: OpenSyn = codec
            .read(&mut reader)
            .map_err(|_| zerror!("{S} Decoding error."))?;

        let mut prng = zasynclock!(self.prng);
        let nonce = zasyncread!(self.inner)
            .pri_key
            .decrypt_blinded(
                &mut *prng,
                Pkcs1v15Encrypt,
                open_syn.nonce_encrypted_with_bob_pubkey.as_slice(),
            )
            .map_err(|_| zerror!("{S} Decryption error."))?;

        if nonce.as_slice() != state.challenge.to_le_bytes() {
            println!("{:02x?}\n{:02x?}", nonce, state.nonce);
            bail!("{S} Invalid nonce.");
        }

        Ok(())
    }

    type SendOpenAckIn = &'a StateAccept;
    type SendOpenAckOut = Option<ext::OpenAck>;
    async fn send_open_ack(
        self,
        _input: Self::SendOpenAckIn,
    ) -> Result<Self::SendOpenAckOut, Self::Error> {
        const S: &str = "PubKey extension - Send OpenAck.";
        tracing::trace!("{S}");

        Ok(Some(ZExtUnit::new()))
    }
}
