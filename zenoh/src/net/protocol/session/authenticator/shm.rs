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
use super::core::{PeerId, Property, ZInt};
use super::io::{RBuf, SharedMemoryBuf, SharedMemoryManager, WBuf};
use super::{
    attachment, AuthenticatedPeerLink, PeerAuthenticator, PeerAuthenticatorOutput,
    PeerAuthenticatorTrait,
};
use async_std::sync::{Arc, Mutex};
use async_trait::async_trait;
use rand::{Rng, SeedableRng};
use std::convert::TryInto;
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::crypto::PseudoRng;
use zenoh_util::properties::config::*;
use zenoh_util::{zasynclock, zcheck};

const WBUF_SIZE: usize = 64;
const SHM_VERSION: ZInt = 0;
const SHM_NAME: &str = "shmauth";
// Let's use a ZInt as a challenge
const SHM_SIZE: usize = std::mem::size_of::<ZInt>();

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
/// ~   ShmMemBuf   ~
/// +---------------+
struct InitSynProperty {
    version: ZInt,
    shm: RBuf,
}

impl WBuf {
    fn write_init_syn_property_shm(&mut self, init_syn_property: &InitSynProperty) -> bool {
        zcheck!(self.write_zint(init_syn_property.version));
        self.write_rbuf(&init_syn_property.shm)
    }
}

impl RBuf {
    fn read_init_syn_property_shm(&mut self) -> Option<InitSynProperty> {
        let version = self.read_zint()?;
        let shm = self.read_rbuf()?;
        Some(InitSynProperty { version, shm })
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
    shm: RBuf,
}

impl WBuf {
    fn write_init_ack_property_shm(&mut self, init_ack_property: &InitAckProperty) -> bool {
        zcheck!(self.write_zint(init_ack_property.challenge));
        self.write_rbuf(&init_ack_property.shm)
    }
}

impl RBuf {
    fn read_init_ack_property_shm(&mut self) -> Option<InitAckProperty> {
        let challenge = self.read_zint()?;
        let shm = self.read_rbuf()?;
        Some(InitAckProperty { challenge, shm })
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

impl WBuf {
    fn write_open_syn_property_shm(&mut self, open_syn_property: &OpenSynProperty) -> bool {
        self.write_zint(open_syn_property.challenge)
    }
}

impl RBuf {
    fn read_open_syn_property_shm(&mut self) -> Option<OpenSynProperty> {
        let challenge = self.read_zint()?;
        Some(OpenSynProperty { challenge })
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
    manager: Mutex<SharedMemoryManager>,
}

impl SharedMemoryAuthenticator {
    pub fn new() -> SharedMemoryAuthenticator {
        let mut prng = PseudoRng::from_entropy();
        let challenge = prng.gen::<ZInt>();

        let mut manager =
            SharedMemoryManager::new(format!("{}.{}", SHM_NAME, challenge), SHM_SIZE).unwrap();

        let mut buffer = manager.alloc(SHM_SIZE).unwrap();
        let slice = unsafe { buffer.as_mut_slice() };
        slice[0..SHM_SIZE].copy_from_slice(&challenge.to_le_bytes());

        SharedMemoryAuthenticator {
            challenge,
            buffer,
            manager: Mutex::new(manager),
        }
    }

    pub async fn from_properties(
        config: &ConfigProperties,
    ) -> ZResult<Option<SharedMemoryAuthenticator>> {
        let zero_copy = config.get_or(&ZN_ZERO_COPY_KEY, ZN_ZERO_COPY_DEFAULT);
        if zero_copy == ZN_TRUE {
            let mut prng = PseudoRng::from_entropy();
            let challenge = prng.gen::<ZInt>();

            let mut manager =
                SharedMemoryManager::new(format!("{}.{}", SHM_NAME, challenge), SHM_SIZE).map_err(
                    |e| {
                        zerror2!(ZErrorKind::Other {
                            descr: e.to_string()
                        })
                    },
                )?;

            let mut buffer = manager.alloc(SHM_SIZE).unwrap();
            let slice = unsafe { buffer.as_mut_slice() };
            slice[0..SHM_SIZE].copy_from_slice(&challenge.to_le_bytes());

            let sma = SharedMemoryAuthenticator {
                challenge,
                buffer,
                manager: Mutex::new(manager),
            };
            return Ok(Some(sma));
        }
        Ok(None)
    }
}

impl Default for SharedMemoryAuthenticator {
    fn default() -> Self {
        Self::new()
    }
}

unsafe impl Send for SharedMemoryAuthenticator {}
unsafe impl Sync for SharedMemoryAuthenticator {}

#[async_trait]
impl PeerAuthenticatorTrait for SharedMemoryAuthenticator {
    async fn get_init_syn_properties(
        &self,
        _link: &AuthenticatedPeerLink,
        _peer_id: &PeerId,
    ) -> ZResult<PeerAuthenticatorOutput> {
        let init_syn_property = InitSynProperty {
            version: SHM_VERSION,
            shm: self.buffer.clone().into(),
        };
        let mut wbuf = WBuf::new(WBUF_SIZE, false);
        wbuf.write_init_syn_property_shm(&init_syn_property);
        let rbuf: RBuf = wbuf.into();

        let prop = Property {
            key: attachment::authorization::SHM,
            value: rbuf.to_vec(),
        };
        let mut res = PeerAuthenticatorOutput::default();
        res.properties.push(prop);
        Ok(res)
    }

    async fn handle_init_syn(
        &self,
        link: &AuthenticatedPeerLink,
        peer_id: &PeerId,
        _sn_resolution: ZInt,
        properties: &[Property],
    ) -> ZResult<PeerAuthenticatorOutput> {
        log::debug!("Authenticator::handle_init_syn(...)");
        let res = properties
            .iter()
            .find(|p| p.key == attachment::authorization::SHM);
        let mut rbuf: RBuf = match res {
            Some(p) => p.value.clone().into(),
            None => {
                log::debug!("Peer {} did not express interest in shared memory", peer_id);
                return Ok(PeerAuthenticatorOutput::default());
            }
        };
        let init_syn_property = match rbuf.read_init_syn_property_shm() {
            Some(isa) => isa,
            None => {
                return zerror!(ZErrorKind::InvalidMessage {
                    descr: format!("Received InitSyn with invalid attachment on link: {}", link),
                });
            }
        };

        if init_syn_property.version > SHM_VERSION {
            return zerror!(ZErrorKind::InvalidMessage {
                descr: format!("Rejected InitSyn with invalid attachment on link: {}", link),
            });
        }

        // Try to read from the shared memory
        let mut manager = zasynclock!(self.manager);
        let sbuf = match init_syn_property.shm.into_shm(&mut manager) {
            Ok(sbuf) => sbuf,
            Err(_) => {
                log::debug!("Peer {} can not operate over shared memory", peer_id);
                return Ok(PeerAuthenticatorOutput::default());
            }
        };

        log::debug!("Authenticating Shared Memory Access...");

        let xs = sbuf.as_slice();
        log::debug!("Extracted Slice creating array... printing content:");
        log::debug!("Slice: {:?}", xs);

        let bytes: [u8; SHM_SIZE] = match xs.try_into() {
            Ok(bytes) => bytes,
            Err(e) => {
                log::debug!("Peer {} can not operate over shared memory: {}", peer_id, e);
                return Ok(PeerAuthenticatorOutput::default());
            }
        };
        let challenge = ZInt::from_le_bytes(bytes);

        // Create the InitAck attachment
        let init_ack_property = InitAckProperty {
            challenge,
            shm: self.buffer.clone().into(),
        };
        // Encode the InitAck property
        let mut wbuf = WBuf::new(WBUF_SIZE, false);
        wbuf.write_init_ack_property_shm(&init_ack_property);
        let rbuf: RBuf = wbuf.into();

        let prop = Property {
            key: attachment::authorization::SHM,
            value: rbuf.to_vec(),
        };
        let mut res = PeerAuthenticatorOutput::default();
        res.properties.push(prop);
        Ok(res)
    }

    async fn handle_init_ack(
        &self,
        link: &AuthenticatedPeerLink,
        peer_id: &PeerId,
        _sn_resolution: ZInt,
        properties: &[Property],
    ) -> ZResult<PeerAuthenticatorOutput> {
        let res = properties
            .iter()
            .find(|p| p.key == attachment::authorization::SHM);
        let mut rbuf: RBuf = match res {
            Some(p) => p.value.clone().into(),
            None => {
                log::debug!("Peer {} did not express interest in shared memory", peer_id);
                return Ok(PeerAuthenticatorOutput::default());
            }
        };
        let init_ack_property = match rbuf.read_init_ack_property_shm() {
            Some(iaa) => iaa,
            None => {
                return zerror!(ZErrorKind::InvalidMessage {
                    descr: format!("Received InitAck with invalid attachment on link: {}", link),
                });
            }
        };

        // Try to read from the shared memory
        let mut manager = zasynclock!(self.manager);
        let sbuf = match init_ack_property.shm.into_shm(&mut manager) {
            Ok(sbuf) => sbuf,
            Err(_) => {
                log::debug!("Peer {} can not operate over shared memory", peer_id);
                return Ok(PeerAuthenticatorOutput::default());
            }
        };

        let bytes: [u8; SHM_SIZE] = match sbuf.as_slice().try_into() {
            Ok(bytes) => bytes,
            Err(e) => {
                log::debug!("Peer {} can not operate over shared memory: {}", peer_id, e);
                return Ok(PeerAuthenticatorOutput::default());
            }
        };
        let challenge = ZInt::from_le_bytes(bytes);

        // Create the OpenSyn attachment
        let open_syn_property = OpenSynProperty { challenge };
        // Encode the OpenSyn property
        let mut wbuf = WBuf::new(WBUF_SIZE, false);
        wbuf.write_open_syn_property_shm(&open_syn_property);
        let rbuf: RBuf = wbuf.into();

        let prop = Property {
            key: attachment::authorization::SHM,
            value: rbuf.to_vec(),
        };

        let mut res = PeerAuthenticatorOutput::default();
        res.properties.push(prop);
        if init_ack_property.challenge == self.challenge {
            res.session.is_local = true;
        }
        Ok(res)
    }

    async fn handle_open_syn(
        &self,
        link: &AuthenticatedPeerLink,
        properties: &[Property],
    ) -> ZResult<PeerAuthenticatorOutput> {
        let res = properties
            .iter()
            .find(|p| p.key == attachment::authorization::SHM);
        let mut rbuf: RBuf = match res {
            Some(p) => p.value.clone().into(),
            None => {
                log::debug!("Received OpenSyn with no SHM attachment on link: {}", link);
                return Ok(PeerAuthenticatorOutput::default());
            }
        };
        let open_syn_property = match rbuf.read_open_syn_property_shm() {
            Some(isa) => isa,
            None => {
                return zerror!(ZErrorKind::InvalidMessage {
                    descr: format!("Received OpenSyn with invalid attachment on link: {}", link),
                });
            }
        };

        let mut res = PeerAuthenticatorOutput::default();
        if open_syn_property.challenge == self.challenge {
            res.session.is_local = true;
        }
        Ok(res)
    }

    async fn handle_open_ack(
        &self,
        _link: &AuthenticatedPeerLink,
        _properties: &[Property],
    ) -> ZResult<PeerAuthenticatorOutput> {
        Ok(PeerAuthenticatorOutput::default())
    }

    async fn handle_link_err(&self, _link: &AuthenticatedPeerLink) {}

    async fn handle_close(&self, _peer_id: &PeerId) {}
}

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
