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
use async_trait::async_trait;
use rand::{Rng, SeedableRng};
use std::{
    convert::TryInto,
    sync::{Arc, RwLock},
};
use zenoh_buffers::{
    reader::{DidntRead, HasReader, Reader},
    writer::{DidntWrite, HasWriter, Writer},
    ZSlice,
};
use zenoh_codec::{RCodec, WCodec, Zenoh060};
use zenoh_config::Config;
use zenoh_crypto::PseudoRng;
use zenoh_protocol::core::{ZInt, ZenohId};
use zenoh_result::{bail, zerror, ShmError, ZResult};
use zenoh_shm::{
    SharedMemoryBuf, SharedMemoryBufInfoSerialized, SharedMemoryManager, SharedMemoryReader,
};

const SHM_VERSION: ZInt = 0;
const SHM_NAME: &str = "shmauth";
// Let's use a ZInt as a challenge
const SHM_SIZE: usize = std::mem::size_of::<ZInt>();

/*************************************/
/*             InitSyn               */
/*************************************/
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |0 0 0|  ATTCH  |
/// +-+-+-+---------+
/// ~    version    ~
/// +---------------+
/// ~   ShmMemBuf   ~
/// +---------------+
struct InitSynProperty {
    version: ZInt,
    shm: ZSlice,
}

impl<W> WCodec<&InitSynProperty, &mut W> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &InitSynProperty) -> Self::Output {
        self.write(&mut *writer, x.version)?;
        self.write(&mut *writer, &x.shm)?;
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
        let bytes: Vec<u8> = self.read(&mut *reader)?;
        let shm_info: SharedMemoryBufInfoSerialized = bytes.into();
        let shm: ZSlice = shm_info.into();
        Ok(InitSynProperty { version, shm })
    }
}

/*************************************/
/*             InitAck               */
/*************************************/
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |0 0 0|  ATTCH  |
/// +-+-+-+---------+
/// ~   challenge   ~
/// +---------------+
/// ~   ShmMemBuf   ~
/// +---------------+
struct InitAckProperty {
    challenge: ZInt,
    shm: ZSlice,
}

impl<W> WCodec<&InitAckProperty, &mut W> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &InitAckProperty) -> Self::Output {
        self.write(&mut *writer, x.challenge)?;
        self.write(&mut *writer, &x.shm)?;
        Ok(())
    }
}

impl<R> RCodec<InitAckProperty, &mut R> for Zenoh060
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<InitAckProperty, Self::Error> {
        let challenge: ZInt = self.read(&mut *reader)?;
        let bytes: Vec<u8> = self.read(&mut *reader)?;
        let shm_info: SharedMemoryBufInfoSerialized = bytes.into();
        let shm: ZSlice = shm_info.into();
        Ok(InitAckProperty { challenge, shm })
    }
}

/*************************************/
/*             OpenSyn               */
/*************************************/
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |0 0 0|  ATTCH  |
/// +-+-+-+---------+
/// ~   challenge   ~
/// +---------------+
struct OpenSynProperty {
    challenge: ZInt,
}

impl<W> WCodec<&OpenSynProperty, &mut W> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &OpenSynProperty) -> Self::Output {
        self.write(&mut *writer, x.challenge)?;
        Ok(())
    }
}

impl<R> RCodec<OpenSynProperty, &mut R> for Zenoh060
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<OpenSynProperty, Self::Error> {
        let challenge: ZInt = self.read(&mut *reader)?;
        Ok(OpenSynProperty { challenge })
    }
}

/*************************************/
/*          Authenticator            */
/*************************************/
pub struct SharedMemoryAuthenticator {
    challenge: ZInt,
    // Rust guarantees that fields are dropped in the order of declaration.
    // Buffer needs to be dropped before the manager.
    buffer: SharedMemoryBuf,
    _manager: SharedMemoryManager,
    reader: Arc<RwLock<SharedMemoryReader>>,
}

impl SharedMemoryAuthenticator {
    pub fn make() -> ZResult<SharedMemoryAuthenticator> {
        let mut prng = PseudoRng::from_entropy();
        let challenge = prng.gen::<ZInt>();

        let mut _manager = SharedMemoryManager::make(format!("{SHM_NAME}.{challenge}"), SHM_SIZE)?;

        let mut buffer = _manager.alloc(SHM_SIZE).unwrap();
        let slice = unsafe { buffer.as_mut_slice() };
        slice[0..SHM_SIZE].copy_from_slice(&challenge.to_le_bytes());

        let shmauth = SharedMemoryAuthenticator {
            challenge,
            buffer,
            _manager,
            reader: Arc::new(RwLock::new(SharedMemoryReader::new())),
        };
        Ok(shmauth)
    }

    pub async fn from_config(config: &Config) -> ZResult<Option<SharedMemoryAuthenticator>> {
        if *config.transport().shared_memory().enabled() {
            let mut prng = PseudoRng::from_entropy();
            let challenge = prng.gen::<ZInt>();

            let mut _manager =
                SharedMemoryManager::make(format!("{SHM_NAME}.{challenge}"), SHM_SIZE)?;

            let mut buffer = _manager.alloc(SHM_SIZE)?;
            let slice = unsafe { buffer.as_mut_slice() };
            slice[0..SHM_SIZE].copy_from_slice(&challenge.to_le_bytes());

            let sma = SharedMemoryAuthenticator {
                challenge,
                buffer,
                _manager,
                reader: Arc::new(RwLock::new(SharedMemoryReader::new())),
            };
            Ok(Some(sma))
        } else {
            Ok(None)
        }
    }
}

unsafe impl Send for SharedMemoryAuthenticator {}
unsafe impl Sync for SharedMemoryAuthenticator {}

#[async_trait]
impl PeerAuthenticatorTrait for SharedMemoryAuthenticator {
    fn id(&self) -> PeerAuthenticatorId {
        PeerAuthenticatorId::Shm
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
            version: SHM_VERSION,
            shm: self.buffer.info.serialize().unwrap().into(),
        };
        let mut buff = vec![];
        let codec = Zenoh060::default();

        let mut writer = buff.writer();
        codec
            .write(&mut writer, &init_syn_property)
            .map_err(|_| zerror!("Error in encoding InitSyn for SHM on link: {}", link))?;

        Ok(Some(buff))
    }

    async fn handle_init_syn(
        &self,
        link: &AuthenticatedPeerLink,
        cookie: &Cookie,
        mut property: Option<Vec<u8>>,
    ) -> ZResult<(Option<Vec<u8>>, Option<Vec<u8>>)> {
        let buffer = match property.take() {
            Some(p) => p,
            None => {
                log::debug!("Peer {} did not express interest in SHM", cookie.zid);
                return Ok((None, None));
            }
        };

        let codec = Zenoh060::default();
        let mut reader = buffer.reader();

        let mut init_syn_property: InitSynProperty = codec
            .read(&mut reader)
            .map_err(|_| zerror!("Received InitSyn with invalid attachment on link: {}", link))?;

        if init_syn_property.version > SHM_VERSION {
            bail!("Rejected InitSyn with invalid attachment on link: {}", link)
        }

        // Try to read from the shared memory
        match crate::shm::map_zslice_to_shmbuf(&mut init_syn_property.shm, &self.reader) {
            Ok(res) => {
                if !res {
                    log::debug!("Peer {} can not operate over SHM: error", cookie.zid);
                    return Ok((None, None));
                }
            }
            Err(e) => {
                log::debug!("Peer {} can not operate over SHM: {}", cookie.zid, e);
                return Ok((None, None));
            }
        }

        log::debug!("Authenticating Shared Memory Access...");

        let xs = init_syn_property.shm;
        let bytes: [u8; SHM_SIZE] = match xs.as_slice().try_into() {
            Ok(bytes) => bytes,
            Err(e) => {
                log::debug!("Peer {} can not operate over SHM: {}", cookie.zid, e);
                return Ok((None, None));
            }
        };
        let challenge = ZInt::from_le_bytes(bytes);

        // Create the InitAck attachment
        let init_ack_property = InitAckProperty {
            challenge,
            shm: self.buffer.info.serialize().unwrap().into(),
        };
        // Encode the InitAck property
        let mut buffer = vec![];
        let mut writer = buffer.writer();
        codec
            .write(&mut writer, &init_ack_property)
            .map_err(|_| zerror!("Error in encoding InitSyn for SHM on link: {}", link))?;

        Ok((Some(buffer), None))
    }

    async fn handle_init_ack(
        &self,
        link: &AuthenticatedPeerLink,
        peer_id: &ZenohId,
        _sn_resolution: ZInt,
        mut property: Option<Vec<u8>>,
    ) -> ZResult<Option<Vec<u8>>> {
        let buffer = match property.take() {
            Some(p) => p,
            None => {
                log::debug!("Peer {} did not express interest in SHM", peer_id);
                return Ok(None);
            }
        };

        let codec = Zenoh060::default();
        let mut reader = buffer.reader();

        let mut init_ack_property: InitAckProperty = codec
            .read(&mut reader)
            .map_err(|_| zerror!("Received InitAck with invalid attachment on link: {}", link))?;

        // Try to read from the shared memory
        match crate::shm::map_zslice_to_shmbuf(&mut init_ack_property.shm, &self.reader) {
            Ok(res) => {
                if !res {
                    return Err(ShmError(zerror!("No SHM on link: {}", link)).into());
                }
            }
            Err(e) => return Err(ShmError(zerror!("No SHM on link {}: {}", link, e)).into()),
        }

        let bytes: [u8; SHM_SIZE] = init_ack_property.shm.as_slice().try_into().map_err(|e| {
            zerror!(
                "Received InitAck with invalid attachment on link {}: {}",
                link,
                e
            )
        })?;
        let challenge = ZInt::from_le_bytes(bytes);

        if init_ack_property.challenge == self.challenge {
            // Create the OpenSyn attachment
            let open_syn_property = OpenSynProperty { challenge };
            // Encode the OpenSyn property
            let mut buffer = vec![];
            let mut writer = buffer.writer();

            codec
                .write(&mut writer, &open_syn_property)
                .map_err(|_| zerror!("Error in encoding OpenSyn for SHM on link: {}", link))?;

            Ok(Some(buffer))
        } else {
            Err(ShmError(zerror!(
                "Received OpenSyn with invalid attachment on link: {}",
                link
            ))
            .into())
        }
    }

    async fn handle_open_syn(
        &self,
        link: &AuthenticatedPeerLink,
        _cookie: &Cookie,
        property: (Option<Vec<u8>>, Option<Vec<u8>>),
    ) -> ZResult<Option<Vec<u8>>> {
        let (mut attachment, _cookie) = property;
        let buffer = match attachment.take() {
            Some(p) => p,
            None => {
                return Err(ShmError(zerror!(
                    "Received OpenSyn with no SHM attachment on link: {}",
                    link
                ))
                .into());
            }
        };

        let codec = Zenoh060::default();
        let mut reader = buffer.reader();

        let open_syn_property: OpenSynProperty = codec
            .read(&mut reader)
            .map_err(|_| zerror!("Received OpenSyn with invalid attachment on link: {}", link))?;

        if open_syn_property.challenge == self.challenge {
            Ok(None)
        } else {
            Err(ShmError(zerror!(
                "Received OpenSyn with invalid attachment on link: {}",
                link
            ))
            .into())
        }
    }

    async fn handle_open_ack(
        &self,
        _link: &AuthenticatedPeerLink,
        _property: Option<Vec<u8>>,
    ) -> ZResult<Option<Vec<u8>>> {
        Ok(None)
    }

    async fn handle_link_err(&self, _link: &AuthenticatedPeerLink) {}

    async fn handle_close(&self, _peer_id: &ZenohId) {}
}

//noinspection ALL
impl From<Arc<SharedMemoryAuthenticator>> for PeerAuthenticator {
    fn from(v: Arc<SharedMemoryAuthenticator>) -> PeerAuthenticator {
        PeerAuthenticator(v)
    }
}

impl From<SharedMemoryAuthenticator> for PeerAuthenticator {
    fn from(v: SharedMemoryAuthenticator) -> PeerAuthenticator {
        Self::from(Arc::new(v))
    }
}
