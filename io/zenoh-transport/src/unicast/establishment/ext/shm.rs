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
use crate::unicast::{establishment::Cookie, shm::SharedMemoryUnicast};
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
use zenoh_codec::{RCodec, WCodec, Zenoh080};
use zenoh_config::Config;
use zenoh_crypto::PseudoRng;
use zenoh_protocol::{
    core::{ZInt, ZenohId},
    transport::{init, open},
};
use zenoh_result::{bail, zerror, ShmError, ZResult};
use zenoh_shm::{
    SharedMemoryBuf, SharedMemoryBufInfo, SharedMemoryBufInfoSerialized, SharedMemoryManager,
    SharedMemoryReader,
};

pub(crate) struct Shm<'a> {
    inner: &'a SharedMemoryUnicast,
}
// /*************************************/
// /*             InitSyn               */
// /*************************************/
// ///  7 6 5 4 3 2 1 0
// /// +-+-+-+-+-+-+-+-+
// /// |X 1 0|   EXT   |
// /// +-+-+-+---------+
// /// |    version    |
// /// +---------------+
// /// ~ ShmMemBufInfo ~
// /// +---------------+
// pub(crate) struct InitSyn {
//     pub(crate) version: u8,
//     pub(crate) info: ZSlice,
// }

// // Codec
// impl<W> WCodec<&InitSyn, &mut W> for Zenoh080
// where
//     W: Writer,
// {
//     type Output = Result<(), DidntWrite>;

//     fn write(self, writer: &mut W, x: &InitSyn) -> Self::Output {
//         self.write(&mut *writer, x.version)?;
//         self.write(&mut *writer, &x.info)?;
//         Ok(())
//     }
// }

// impl<R> RCodec<InitSyn, &mut R> for Zenoh080
// where
//     R: Reader,
// {
//     type Error = DidntRead;

//     fn read(self, reader: &mut R) -> Result<InitSyn, Self::Error> {
//         let version: ZInt = self.read(&mut *reader)?;
//         let info: ZSlice = self.read(&mut *reader)?;
//         Ok(InitSyn { version, info })
//     }
// }

// Open
// pub(crate) struct InitSynOpenState {
//     buffer: SharedMemoryBuf,
// }

// impl InitSyn {
//     pub(crate) fn send(shm: &SharedMemoryUnicast) -> ZResult<(init::ext::Shm, InitSynOpenState)> {
//         let init_syn_property = InitSyn {
//             version: shm::VERSION,
//             info: shm
//                 .buffer
//                 .info
//                 .serialize()
//                 .map_err(|e| zerror!("{e}"))?
//                 .into(),
//         };
//         let mut buff = vec![];
//         let codec = Zenoh080::new();

//         let mut writer = buff.writer();
//         codec
//             .write(&mut writer, &init_syn_property)
//             .map_err(|_| zerror!("Error in encoding InitSyn SHM extension"))?;

//         Ok(Some(buff))
//     }
// }

// pub(crate) struct InitSynAcceptInput {
//     link: &AuthenticatedLink,
//     cookie: &Cookie,
//     property: Option<Vec<u8>>,
// }

// Accept
// pub(crate) struct InitSynAcceptState {}

// impl InitSyn {
//     pub(crate) fn accept(
//         ext: &init::ext::Shm,
//         reader: &SharedMemoryReader,
//     ) -> ZResult<InitSynAcceptState> {
//         let codec = Zenoh080::new();
//         let mut reader = ext.reader();

//         let mut init_syn_ext: InitSyn = codec
//             .read(&mut reader)
//             .map_err(|_| zerror!("Error in decoding InitSyn SHM extension"))?;

//         if init_syn_ext.version != SHM_VERSION {
//             bail!(
//                 "Incompatible InitSyn SHM extension version. Expected: {}. Received: {}.",
//                 SHM_VERSION,
//                 init_syn_ext.version
//             );
//         }

//         // Try to read from the shared memory
//         match crate::shm::map_zslice_to_shmbuf(&mut init_syn_ext.shm, reader) {
//             Ok(false) => {
//                 log::debug!("Zenoh node can not operate over SHM: not an SHM buffer");
//                 return Ok((None, None));
//             }
//             Err(e) => {
//                 log::debug!("Zenoh node can not operate over SHM: {}", e);
//                 return Ok((None, None));
//             }
//             Ok(true) => {
//                 // Payload is SHM: continue.
//             }
//         }

//         log::trace!("Verifying SHM extension");

//         let xs = init_syn_ext.shm;
//         let bytes: [u8; SHM_SIZE] = match xs.as_slice().try_into() {
//             Ok(bytes) => bytes,
//             Err(e) => {
//                 log::debug!("Zenoh node can not operate over SHM: {}", e);
//                 return Ok((None, None));
//             }
//         };
//         let challenge = ZInt::from_le_bytes(bytes);

//         // Create the InitAck attachment
//         let init_ack_property = InitAckExt {
//             challenge,
//             shm: buffer.info.serialize()?.into(),
//         };
//         // Encode the InitAck property
//         let mut buffer = vec![];
//         let mut writer = buffer.writer();
//         codec
//             .write(&mut writer, &init_ack_property)
//             .map_err(|_| zerror!("Error in encoding InitSyn for SHM on link: {}", link))?;

//         Ok((Some(buffer), None))
//     }
// }

/*************************************/
/*             InitAck               */
/*************************************/
// ///  7 6 5 4 3 2 1 0
// /// +-+-+-+-+-+-+-+-+
// /// |0 0 0|  ATTCH  |
// /// +-+-+-+---------+
// /// ~   challenge   ~
// /// +---------------+
// /// ~ ShmMemBufInfo ~
// /// +---------------+
// struct InitAck {
//     challenge: ZInt,
//     info: ZSlice,
// }

// impl<W> WCodec<&InitAck, &mut W> for Zenoh080
// where
//     W: Writer,
// {
//     type Output = Result<(), DidntWrite>;

//     fn write(self, writer: &mut W, x: &InitAck) -> Self::Output {
//         self.write(&mut *writer, x.challenge)?;
//         self.write(&mut *writer, &x.shm)?;
//         Ok(())
//     }
// }

// impl<R> RCodec<InitAck, &mut R> for Zenoh080
// where
//     R: Reader,
// {
//     type Error = DidntRead;

//     fn read(self, reader: &mut R) -> Result<InitAck, Self::Error> {
//         let challenge: ZInt = self.read(&mut *reader)?;
//         let info: ZSlice = self.read(&mut *reader)?;
//         Ok(InitAck { challenge, info })
//     }
// }

// // Open
// pub(crate) struct InitAckOpenState {
//     buffer: SharedMemoryBuf,
// }

// impl InitAck {
//     pub(crate) fn recv(
//         ext: &init::ext::Shm,
//         shm: &SharedMemoryUnicast,
//     ) -> ZResult<(init::ext::Shm, InitAckOpenState)> {
// let info = SharedMemoryBufInfo::deserialize(ext.value.as_slice())?;

// // Try to read from the shared memory
// let mut zslice = ext.value.clone();
// match crate::shm::map_zslice_to_shmbuf(&mut zslice, reader) {
//     Ok(false) => {
//         log::debug!("Zenoh node can not operate over SHM: not an SHM buffer");
//         return Ok((None, None));
//     }
//     Err(e) => {
//         log::debug!("Zenoh node can not operate over SHM: {}", e);
//         return Ok((None, None));
//     }
//     Ok(true) => {
//         // Payload is SHM: continue.
//     }
// }

// match crate::shm::map_zslice_to_shmbuf(&mut ext, &shm.reader) {
//     Ok(res) => {
//         if !res {
//             return Err(ShmError(zerror!("No SHM on link: {}", link)).into());
//         }
//     }
//     Err(e) => return Err(ShmError(zerror!("No SHM on link {}: {}", link, e)).into()),
// }

// let bytes: [u8; std::mem::size_of::<shm::Nonce>()] =
//     init_ack_property.shm.as_slice().try_into().map_err(|e| {
//         zerror!(
//             "Received InitAck with invalid attachment on link {}: {}",
//             link,
//             e
//         )
//     })?;
// let challenge = ZInt::from_le_bytes(bytes);

// let challenge = ();
// let init_syn_property = InitAck {
//     version: shm::VERSION,
//     info: shm
//         .buffer
//         .info
//         .serialize()
//         .map_err(|e| zerror!("{e}"))?
//         .into(),
// };
// let mut buff = vec![];
// let codec = Zenoh080::new();

// let mut writer = buff.writer();
// codec
//     .write(&mut writer, &init_syn_property)
//     .map_err(|_| zerror!("Error in encoding InitSyn SHM extension"))?;

// Ok(Some(buff))
//         panic!()
//     }
// }

// /*************************************/
// /*             OpenSyn               */
// /*************************************/
// ///  7 6 5 4 3 2 1 0
// /// +-+-+-+-+-+-+-+-+
// /// |0 0 0|  ATTCH  |
// /// +-+-+-+---------+
// /// ~   challenge   ~
// /// +---------------+
// struct OpenSynExt {
//     challenge: ZInt,
// }

// impl<W> WCodec<&OpenSynExt, &mut W> for Zenoh080
// where
//     W: Writer,
// {
//     type Output = Result<(), DidntWrite>;

//     fn write(self, writer: &mut W, x: &OpenSynExt) -> Self::Output {
//         self.write(&mut *writer, x.challenge)?;
//         Ok(())
//     }
// }

// impl<R> RCodec<OpenSynExt, &mut R> for Zenoh080
// where
//     R: Reader,
// {
//     type Error = DidntRead;

//     fn read(self, reader: &mut R) -> Result<OpenSynExt, Self::Error> {
//         let challenge: ZInt = self.read(&mut *reader)?;
//         Ok(OpenSynExt { challenge })
//     }
// }

// /*************************************/
// /*          Authenticator            */
// /*************************************/
// #[async_trait]
// impl TransportAuthenticatorTrait for SharedMemoryAuthenticator {
//     fn id(&self) -> ZNodeAuthenticatorId {
//         ZNodeAuthenticatorId::Shm
//     }

//     async fn close(&self) {
//         // No cleanup needed
//     }

//     async fn get_init_syn_properties(
//         &self,
//         link: &AuthenticatedLink,
//         _node_id: &ZenohId,
//     ) -> ZResult<Option<Vec<u8>>> {
//         let init_syn_property = InitSyn {
//             version: SHM_VERSION,
//             shm: self.buffer.info.serialize().unwrap().into(),
//         };
//         let mut buff = vec![];
//         let codec = Zenoh080::new();

//         let mut writer = buff.writer();
//         codec
//             .write(&mut writer, &init_syn_property)
//             .map_err(|_| zerror!("Error in encoding InitSyn for SHM on link: {}", link))?;

//         Ok(Some(buff))
//     }

//     async fn handle_init_syn(
//         &self,
//         link: &AuthenticatedLink,
//         cookie: &Cookie,
//         mut property: Option<Vec<u8>>,
//     ) -> ZResult<(Option<Vec<u8>>, Option<Vec<u8>>)> {
//         let buffer = match property.take() {
//             Some(p) => p,
//             None => {
//                 log::debug!("Peer {} did not express interest in SHM", cookie.zid);
//                 return Ok((None, None));
//             }
//         };

//         let codec = Zenoh080::new();
//         let mut reader = buffer.reader();

//         let mut init_syn_property: InitSyn = codec
//             .read(&mut reader)
//             .map_err(|_| zerror!("Received InitSyn with invalid attachment on link: {}", link))?;

//         if init_syn_property.version > SHM_VERSION {
//             bail!("Rejected InitSyn with invalid attachment on link: {}", link)
//         }

//         // Try to read from the shared memory
//         match crate::shm::map_zslice_to_shmbuf(&mut init_syn_property.shm, &self.reader) {
//             Ok(res) => {
//                 if !res {
//                     log::debug!("Peer {} can not operate over SHM: error", cookie.zid);
//                     return Ok((None, None));
//                 }
//             }
//             Err(e) => {
//                 log::debug!("Peer {} can not operate over SHM: {}", cookie.zid, e);
//                 return Ok((None, None));
//             }
//         }

//         log::debug!("Authenticating Shared Memory Access...");

//         let xs = init_syn_property.shm;
//         let bytes: [u8; SHM_SIZE] = match xs.as_slice().try_into() {
//             Ok(bytes) => bytes,
//             Err(e) => {
//                 log::debug!("Peer {} can not operate over SHM: {}", cookie.zid, e);
//                 return Ok((None, None));
//             }
//         };
//         let challenge = ZInt::from_le_bytes(bytes);

//         // Create the InitAck attachment
//         let init_ack_property = InitAckExt {
//             challenge,
//             shm: self.buffer.info.serialize().unwrap().into(),
//         };
//         // Encode the InitAck property
//         let mut buffer = vec![];
//         let mut writer = buffer.writer();
//         codec
//             .write(&mut writer, &init_ack_property)
//             .map_err(|_| zerror!("Error in encoding InitSyn for SHM on link: {}", link))?;

//         Ok((Some(buffer), None))
//     }

//     async fn handle_init_ack(
//         &self,
//         link: &AuthenticatedLink,
//         node_id: &ZenohId,
//         _sn_resolution: ZInt,
//         mut property: Option<Vec<u8>>,
//     ) -> ZResult<Option<Vec<u8>>> {
//         let buffer = match property.take() {
//             Some(p) => p,
//             None => {
//                 log::debug!("Peer {} did not express interest in SHM", node_id);
//                 return Ok(None);
//             }
//         };

//         let codec = Zenoh080::new();
//         let mut reader = buffer.reader();

//         let mut init_ack_property: InitAckExt = codec
//             .read(&mut reader)
//             .map_err(|_| zerror!("Received InitAck with invalid attachment on link: {}", link))?;

//         // Try to read from the shared memory
//         match crate::shm::map_zslice_to_shmbuf(&mut init_ack_property.shm, &self.reader) {
//             Ok(res) => {
//                 if !res {
//                     return Err(ShmError(zerror!("No SHM on link: {}", link)).into());
//                 }
//             }
//             Err(e) => return Err(ShmError(zerror!("No SHM on link {}: {}", link, e)).into()),
//         }

//         let bytes: [u8; SHM_SIZE] = init_ack_property.shm.as_slice().try_into().map_err(|e| {
//             zerror!(
//                 "Received InitAck with invalid attachment on link {}: {}",
//                 link,
//                 e
//             )
//         })?;
//         let challenge = ZInt::from_le_bytes(bytes);

//         if init_ack_property.challenge == self.challenge {
//             // Create the OpenSyn attachment
//             let open_syn_property = OpenSynExt { challenge };
//             // Encode the OpenSyn property
//             let mut buffer = vec![];
//             let mut writer = buffer.writer();

//             codec
//                 .write(&mut writer, &open_syn_property)
//                 .map_err(|_| zerror!("Error in encoding OpenSyn for SHM on link: {}", link))?;

//             Ok(Some(buffer))
//         } else {
//             Err(ShmError(zerror!(
//                 "Received OpenSyn with invalid attachment on link: {}",
//                 link
//             ))
//             .into())
//         }
//     }

//     async fn handle_open_syn(
//         &self,
//         link: &AuthenticatedLink,
//         _cookie: &Cookie,
//         property: (Option<Vec<u8>>, Option<Vec<u8>>),
//     ) -> ZResult<Option<Vec<u8>>> {
//         let (mut attachment, _cookie) = property;
//         let buffer = match attachment.take() {
//             Some(p) => p,
//             None => {
//                 return Err(ShmError(zerror!(
//                     "Received OpenSyn with no SHM attachment on link: {}",
//                     link
//                 ))
//                 .into());
//             }
//         };

//         let codec = Zenoh080::new();
//         let mut reader = buffer.reader();

//         let open_syn_property: OpenSynExt = codec
//             .read(&mut reader)
//             .map_err(|_| zerror!("Received OpenSyn with invalid attachment on link: {}", link))?;

//         if open_syn_property.challenge == self.challenge {
//             Ok(None)
//         } else {
//             Err(ShmError(zerror!(
//                 "Received OpenSyn with invalid attachment on link: {}",
//                 link
//             ))
//             .into())
//         }
//     }

//     async fn handle_open_ack(
//         &self,
//         _link: &AuthenticatedLink,
//         _property: Option<Vec<u8>>,
//     ) -> ZResult<Option<Vec<u8>>> {
//         Ok(None)
//     }

//     async fn handle_link_err(&self, _link: &AuthenticatedLink) {}

//     async fn handle_close(&self, _node_id: &ZenohId) {}
// }
