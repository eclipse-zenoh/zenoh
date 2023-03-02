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
use crate::{
    establishment::{AcceptFsm, OpenFsm},
    unicast::{shm::Challenge, shm::SharedMemoryUnicast},
};
use async_trait::async_trait;
use std::convert::TryInto;
use zenoh_buffers::{
    reader::{DidntRead, HasReader, Reader},
    writer::{DidntWrite, HasWriter, Writer},
};
use zenoh_codec::{RCodec, WCodec, Zenoh080};
use zenoh_core::zasyncwrite;
use zenoh_protocol::{
    core::ZInt,
    transport::{init, open},
};
use zenoh_result::{zerror, Error as ZError};
use zenoh_shm::SharedMemoryBufInfo;

/*************************************/
/*             InitSyn               */
/*************************************/
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// ~ ShmMemBufInfo ~
/// +---------------+
pub(crate) struct InitSyn {
    pub(crate) alice_info: SharedMemoryBufInfo,
}

// Codec
impl<W> WCodec<&InitSyn, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &InitSyn) -> Self::Output {
        self.write(&mut *writer, &x.alice_info)?;
        Ok(())
    }
}

impl<R> RCodec<InitSyn, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<InitSyn, Self::Error> {
        let alice_info: SharedMemoryBufInfo = self.read(&mut *reader)?;
        Ok(InitSyn { alice_info })
    }
}

/*************************************/
/*             InitAck               */
/*************************************/
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// ~   challenge   ~
/// +---------------+
/// ~ ShmMemBufInfo ~
/// +---------------+
struct InitAck {
    alice_challenge: ZInt,
    bob_info: SharedMemoryBufInfo,
}

impl<W> WCodec<&InitAck, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &InitAck) -> Self::Output {
        self.write(&mut *writer, x.alice_challenge)?;
        self.write(&mut *writer, &x.bob_info)?;
        Ok(())
    }
}

impl<R> RCodec<InitAck, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<InitAck, Self::Error> {
        let alice_challenge: ZInt = self.read(&mut *reader)?;
        let bob_info: SharedMemoryBufInfo = self.read(&mut *reader)?;
        Ok(InitAck {
            alice_challenge,
            bob_info,
        })
    }
}

/*************************************/
/*             OpenSyn               */
/*************************************/
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// ~   challenge   ~
/// +---------------+

/*************************************/
/*             OpenAck               */
/*************************************/
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// ~      ack      ~
/// +---------------+

// Extension Fsm State
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct State {
    is_shm: bool,
}

impl State {
    pub(crate) const fn new(is_shm: bool) -> Self {
        Self { is_shm }
    }

    pub(crate) const fn is_shm(&self) -> bool {
        self.is_shm
    }
}

// Codec
impl<W> WCodec<&State, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &State) -> Self::Output {
        let is_shm = u8::from(x.is_shm);
        self.write(&mut *writer, is_shm)?;
        Ok(())
    }
}

impl<R> RCodec<State, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<State, Self::Error> {
        let is_shm: u8 = self.read(&mut *reader)?;
        let is_shm = is_shm == 1;
        Ok(State { is_shm })
    }
}

// Extension Fsm
pub(crate) struct Shm<'a> {
    inner: &'a SharedMemoryUnicast,
}

impl<'a> Shm<'a> {
    pub(crate) const fn new(inner: &'a SharedMemoryUnicast) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl<'a> OpenFsm<'a> for Shm<'a> {
    type Error = ZError;

    type InitSynIn = &'a State;
    type InitSynOut = Option<init::ext::Shm>;
    async fn send_init_syn(
        &'a self,
        state: Self::InitSynIn,
    ) -> Result<Self::InitSynOut, Self::Error> {
        const S: &str = "Shm extension - Send InitSyn.";

        if !state.is_shm() {
            return Ok(None);
        }

        let init_syn = InitSyn {
            alice_info: self.inner.challenge.info.clone(),
        };

        let codec = Zenoh080::new();
        let mut buff = vec![];
        let mut writer = buff.writer();
        codec
            .write(&mut writer, &init_syn)
            .map_err(|_| zerror!("{} Encoding error", S))?;

        Ok(Some(init::ext::Shm::new(buff.into())))
    }

    type InitAckIn = (&'a mut State, Option<init::ext::Shm>);
    type InitAckOut = Challenge;
    async fn recv_init_ack(
        &'a self,
        input: Self::InitAckIn,
    ) -> Result<Self::InitAckOut, Self::Error> {
        const S: &str = "Shm extension - Recv InitAck.";

        let (state, mut ext) = input;
        if !state.is_shm() {
            return Ok(0);
        }

        let Some(mut ext) = ext.take() else {
            state.is_shm = false;
            return Ok(0);
        };

        // Decode the extension
        let codec = Zenoh080::new();
        let mut reader = ext.value.reader();
        let Ok(init_ack): Result<InitAck, _> = codec.read(&mut reader) else {
            log::trace!("{} Decoding error.", S);
            state.is_shm = false;
            return Ok(0);
        };

        // Alice challenge as seen by Alice
        let bytes: [u8; std::mem::size_of::<Challenge>()] = self
            .inner
            .challenge
            .as_slice()
            .try_into()
            .map_err(|e| zerror!("{}", e))?;
        let challenge = ZInt::from_le_bytes(bytes);

        // Verify that Bob has correctly read Alice challenge
        if challenge != init_ack.alice_challenge {
            log::trace!(
                "{} Challenge mismatch: {} != {}.",
                S,
                init_ack.alice_challenge,
                challenge
            );
            state.is_shm = false;
            return Ok(0);
        }

        // Read Bob's SharedMemoryBuf
        let shm_buff = match zasyncwrite!(self.inner.reader).read_shmbuf(&init_ack.bob_info) {
            Ok(buff) => buff,
            Err(e) => {
                log::trace!("{} {}", S, e);
                state.is_shm = false;
                return Ok(0);
            }
        };

        // Bob challenge as seen by Alice
        let bytes: [u8; std::mem::size_of::<Challenge>()] = match shm_buff.as_slice().try_into() {
            Ok(bytes) => bytes,
            Err(_) => {
                log::trace!("{} Failed to read remote Shm.", S);
                state.is_shm = false;
                return Ok(0);
            }
        };
        let bob_challenge = ZInt::from_le_bytes(bytes);

        Ok(bob_challenge)
    }

    type OpenSynIn = (&'a State, Self::InitAckOut);
    type OpenSynOut = Option<open::ext::Shm>;
    async fn send_open_syn(
        &'a self,
        input: Self::OpenSynIn,
    ) -> Result<Self::OpenSynOut, Self::Error> {
        const S: &str = "Shm extension - Send OpenSyn.";

        let (state, bob_challenge) = input;
        if !state.is_shm() {
            return Ok(None);
        }

        Ok(Some(open::ext::Shm::new(bob_challenge)))
    }

    type OpenAckIn = (&'a mut State, Option<open::ext::Shm>);
    type OpenAckOut = ();
    async fn recv_open_ack(
        &'a self,
        input: Self::OpenAckIn,
    ) -> Result<Self::OpenAckOut, Self::Error> {
        const S: &str = "Shm extension - Recv OpenAck.";

        let (state, mut ext) = input;
        if !state.is_shm() {
            return Ok(());
        }

        let Some(ext) = ext.take() else {
            state.is_shm = false;
            return Ok(());
        };

        if ext.value != 1 {
            log::trace!("{} Invalid value.", S);
            state.is_shm = false;
            return Ok(());
        }

        state.is_shm = true;
        Ok(())
    }
}

/*************************************/
/*            ACCEPT                 */
/*************************************/
#[async_trait]
impl<'a> AcceptFsm<'a> for Shm<'a> {
    type Error = ZError;

    type InitSynIn = (&'a mut State, Option<init::ext::Shm>);
    type InitSynOut = Challenge;
    async fn recv_init_syn(
        &'a self,
        input: Self::InitSynIn,
    ) -> Result<Self::InitSynOut, Self::Error> {
        const S: &str = "Shm extension - Recv InitSyn.";

        let (state, mut ext) = input;
        if !state.is_shm() {
            return Ok(0);
        }

        let Some(mut ext) = ext.take() else {
            state.is_shm = false;
            return Ok(0);
        };

        // Decode the extension
        let codec = Zenoh080::new();
        let mut reader = ext.value.reader();
        let Ok(init_syn): Result<InitSyn, _> = codec.read(&mut reader) else {
            log::trace!("{} Decoding error.", S);
            state.is_shm = false;
            return Ok(0);
        };

        // Read Alice's SharedMemoryBuf
        let shm_buff = match zasyncwrite!(self.inner.reader).read_shmbuf(&init_syn.alice_info) {
            Ok(buff) => buff,
            Err(e) => {
                log::trace!("{} {}", S, e);
                state.is_shm = false;
                return Ok(0);
            }
        };

        // Alice challenge as seen by Bob
        let bytes: [u8; std::mem::size_of::<Challenge>()] = match shm_buff.as_slice().try_into() {
            Ok(bytes) => bytes,
            Err(_) => {
                log::trace!("{} Failed to read remote Shm.", S);
                state.is_shm = false;
                return Ok(0);
            }
        };
        let alice_challenge = ZInt::from_le_bytes(bytes);

        Ok(alice_challenge)
    }

    type InitAckIn = (&'a State, Self::InitSynOut);
    type InitAckOut = Option<init::ext::Shm>;
    async fn send_init_ack(
        &'a self,
        input: Self::InitAckIn,
    ) -> Result<Self::InitAckOut, Self::Error> {
        const S: &str = "Shm extension - Send InitAck.";

        let (state, alice_challenge) = input;
        if !state.is_shm() {
            return Ok(None);
        }

        let init_syn = InitAck {
            alice_challenge,
            bob_info: self.inner.challenge.info.clone(),
        };

        let codec = Zenoh080::new();
        let mut buff = vec![];
        let mut writer = buff.writer();
        codec
            .write(&mut writer, &init_syn)
            .map_err(|_| zerror!("{} Encoding error", S))?;

        Ok(Some(init::ext::Shm::new(buff.into())))
    }

    type OpenSynIn = (&'a mut State, Option<open::ext::Shm>);
    type OpenSynOut = ();
    async fn recv_open_syn(
        &'a self,
        input: Self::OpenSynIn,
    ) -> Result<Self::OpenSynOut, Self::Error> {
        const S: &str = "Shm extension - Recv OpenSyn.";

        let (state, mut ext) = input;
        if !state.is_shm() {
            return Ok(());
        }

        let Some(ext) = ext.take() else {
            state.is_shm = false;
            return Ok(());
        };

        // Bob challenge as seen by Bob
        let bytes: [u8; std::mem::size_of::<Challenge>()] = self
            .inner
            .challenge
            .as_slice()
            .try_into()
            .map_err(|e| zerror!("{}", e))?;
        let challenge = ZInt::from_le_bytes(bytes);

        // Verify that Alice has correctly read Bob challenge
        let bob_challnge = ext.value;
        if challenge != bob_challnge {
            log::trace!(
                "{} Challenge mismatch: {} != {}.",
                S,
                bob_challnge,
                challenge
            );
            state.is_shm = false;
            return Ok(());
        }

        Ok(())
    }

    type OpenAckIn = &'a mut State;
    type OpenAckOut = Option<open::ext::Shm>;
    async fn send_open_ack(
        &'a self,
        state: Self::OpenAckIn,
    ) -> Result<Self::OpenAckOut, Self::Error> {
        const S: &str = "Shm extension - Recv OpenSyn.";

        if !state.is_shm() {
            return Ok(None);
        }

        state.is_shm = true;
        Ok(Some(open::ext::Shm::new(1)))
    }
}

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
