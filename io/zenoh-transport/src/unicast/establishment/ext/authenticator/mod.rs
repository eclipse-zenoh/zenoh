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
#[cfg(feature = "auth_pubkey")]
mod pubkey;
// #[cfg(feature = "auth_usrpwd")]
// mod userpassword;

// use crate::unicast::establishment::Cookie;
// use async_trait::async_trait;
#[cfg(feature = "auth_pubkey")]
pub use pubkey::*;
use zenoh_buffers::reader::Reader;
use zenoh_buffers::writer::Writer;
// use std::collections::HashSet;
// use std::fmt;
// use std::hash::{Hash, Hasher};
// use std::ops::Deref;
// use std::sync::Arc;
// #[cfg(feature = "auth_usrpwd")]
// pub use userpassword::*;
// use zenoh_config::Config;
// use zenoh_link::{Link, Locator};
// use zenoh_protocol::core::{ZInt, ZenohId};
// use zenoh_result::ZResult;

use crate::establishment::{AcceptFsm, OpenFsm};
use async_trait::async_trait;
use std::collections::HashMap;
use zenoh_codec::{RCodec, WCodec};
use zenoh_config::ZenohId;
use zenoh_protocol::common::ZExtUnknown;
use zenoh_protocol::core::ZInt;
use zenoh_result::{ZError, ZResult};

struct Auth<'a> {
    inner: &'a HashMap<&'static str, Box<dyn TransportAuthenticatorTrait>>,
}

pub type TransportAuthenticator<'a> = Box<dyn TransportAuthenticatorTrait>;
// impl<'a>

/*************************************/
/*              PEER                 */
/*************************************/
// #[derive(PartialEq, Eq, Hash)]
// #[repr(u8)]
// pub enum ZNodeAuthenticatorId {
//     Reserved = 0,
//     Shm = 1,
//     UserPassword = 2,
//     PublicKey = 3,
// }

// impl From<ZNodeAuthenticatorId> for ZInt {
//     fn from(pa: ZNodeAuthenticatorId) -> ZInt {
//         pa as ZInt
//     }
// }

// #[derive(Clone)]
// pub struct TransportAuthenticator(Arc<dyn TransportAuthenticatorTrait>);

// impl TransportAuthenticator {
//     pub async fn from_config(_config: &Config) -> ZResult<HashSet<TransportAuthenticator>> {
//         #[allow(unused_mut)]
//         let mut pas = HashSet::new();

//         #[cfg(feature = "auth_pubkey")]
//         {
//             let mut res = PubKeyAuthenticator::from_config(_config).await?;
//             if let Some(pa) = res.take() {
//                 pas.insert(pa.into());
//             }
//         }

//         #[cfg(feature = "auth_usrpwd")]
//         {
//             let mut res = UserPasswordAuthenticator::from_config(_config).await?;
//             if let Some(pa) = res.take() {
//                 pas.insert(pa.into());
//             }
//         }

//         Ok(pas)
//     }
// }

/*************************************/
/*              OPEN                 */
/*************************************/
// InitSyn
pub struct SendInitSynIn<'a> {
    state: &'a mut [u8],
    zid: &'a ZenohId,
}

pub struct SendInitSynOut {
    ext: Option<ZExtUnknown>,
}

// InitAck
pub struct RecvInitAckIn<'a> {
    state: &'a mut [u8],
    zid: &'a ZenohId,
    ext: Option<ZExtUnknown>,
}

pub struct RecvInitAckOut {}

// OpenSyn
pub struct SendOpenSynIn<'a> {
    state: &'a mut [u8],
    zid: &'a ZenohId,
}

pub struct SendOpenSynOut {
    ext: Option<ZExtUnknown>,
}

// OpenAck
pub struct RecvOpenAckIn<'a> {
    state: &'a mut [u8],
    zid: &'a ZenohId,
}

pub struct RecvOpenAckOut {}

#[async_trait]
pub trait TransportAuthenticatorTrait<'a>:
    OpenFsm<
        '_,
        SendInitSynIn = SendInitSynIn<'a>,
        SendInitSynOut = SendInitSynOut,
        RecvInitAckIn = RecvInitAckIn<'a>,
        RecvInitAckOut = RecvInitAckOut,
        SendOpenSynIn = SendOpenSynIn<'a>,
        SendOpenSynOut = SendOpenSynOut,
        RecvOpenAckIn = RecvOpenAckIn<'a>,
        RecvOpenAckOut = RecvOpenAckOut,
        Error = ZError,
    > + AcceptFsm<
        'a,
        RecvInitSynIn = (&'a mut [u8], &'a ZenohId, Option<ZExtUnknown>),
        RecvInitSynOut = (),
        SendInitAckIn = &'a mut [u8],
        SendInitAckOut = Option<ZExtUnknown>,
        RecvOpenSynIn = (&'a mut [u8], Option<ZExtUnknown>),
        RecvOpenSynOut = (),
        SendOpenAckIn = &'a mut [u8],
        SendOpenAckOut = Option<ZExtUnknown>,
        Error = ZError,
    > + Send
    + Sync
{
    fn id(&self) -> &'static str;

    async fn from_config(config: ()) -> ZResult<Self>
    where
        Self: Sized;

    async fn init(&self) -> ZResult<Vec<u8>>;

    // Close the authenticator
    async fn close(&self);
}

#[async_trait]
pub trait TransportAuthenticatorState<'a, W, R>: WCodec<&'a Self, W> + RCodec<Self, R>
where
    Self: 'a + Sized,
    W: Writer,
    R: Reader,
{
}

// #[async_trait]
// pub trait TransportAuthenticatorTrait: Send + Sync {
//     // Return the ID of this authenticator.
//     fn id(&self) -> ZNodeAuthenticatorId;

// Close the authenticator
// async fn close(&self);

// Return the attachment to be included in the InitSyn message.
//
// # Arguments
// * `link`        - The [`AuthenticatedPeerLink`][AuthenticatedPeerLink] the initial InitSyn message will be sent on
//
// * `node_id`     - The [`ZenohId`][ZenohId] of the sender of the InitSyn, i.e., the peer
//                   initiating a new transport.
//
// async fn get_init_syn_properties(
//     &self,
//     link: &AuthenticatedLink,
//     node_id: &ZenohId,
// ) -> ZResult<Option<Vec<u8>>>;

// Return the attachment to be included in the InitAck message to be sent
// in response of the authenticated InitSyn.
//
// # Arguments
// * `link`            - The [`AuthenticatedPeerLink`][AuthenticatedPeerLink] the InitSyn message was received on
//
// * `cookie`          - The Cookie containing the internal state
//
// * `property`        - The optional `Property` included in the InitSyn message
//
// async fn handle_init_syn(
//     &self,
//     link: &AuthenticatedLink,
//     cookie: &Cookie,
//     property: Option<Vec<u8>>,
// ) -> ZResult<(Option<Vec<u8>>, Option<Vec<u8>>)>; // (Attachment, Cookie)

// Return the attachment to be included in the OpenSyn message to be sent
// in response of the authenticated InitAck.
//
// # Arguments
// * `link` - The [`AuthenticatedPeerLink`][AuthenticatedPeerLink] the InitSyn message was received on
//
// * `node_id` - The [`ZenohId`][ZenohId] of the sender of the InitAck message
//
// * `sn_resolution`   - The sn_resolution negotiated by the sender of the InitAck message
//
// * `properties`      - The optional `Property` included in the InitAck message
//
// async fn handle_init_ack(
//     &self,
//     link: &AuthenticatedLink,
//     node_id: &ZenohId,
//     sn_resolution: ZInt,
//     property: Option<Vec<u8>>,
// ) -> ZResult<Option<Vec<u8>>>;

// Return the attachment to be included in the OpenAck message to be sent
// in response of the authenticated OpenSyn.
//
// # Arguments
// * `link` - The [`AuthenticatedPeerLink`][AuthenticatedPeerLink] the OpenSyn message was received on
//
// * `properties`      - The optional `Property` included in the OpenSyn message
//
// * `cookie`          - The optional `Property` included in the OpenSyn message
//
// async fn handle_open_syn(
//     &self,
//     link: &AuthenticatedLink,
//     cookie: &Cookie,
//     property: (Option<Vec<u8>>, Option<Vec<u8>>), // (Attachment, Cookie)
// ) -> ZResult<Option<Vec<u8>>>;

// Auhtenticate the OpenAck. No message is sent back in response to an OpenAck
//
// # Arguments
// * `link` - The [`AuthenticatedPeerLink`][AuthenticatedPeerLink] the OpenAck message was received on
//
// * `properties`      - The optional `Property` included in the OpenAck message
//
// async fn handle_open_ack(
//     &self,
//     link: &AuthenticatedLink,
//     property: Option<Vec<u8>>,
// ) -> ZResult<Option<Vec<u8>>>;

// Handle any error on a link. This callback is mainly used to clean-up any internal state
// of the authenticator in such a way no unnecessary data is left around
//
// # Arguments
// * `link` - The [`AuthenticatedPeerLink`][AuthenticatedPeerLink] generating the error
//
// async fn handle_link_err(&self, link: &AuthenticatedLink);

// Handle any error on a link. This callback is mainly used to clean-up any internal state
// of the authenticator in such a way no unnecessary data is left around
//
// # Arguments
// * `peerd_id` - The [`ZenohId`][ZenohId] of the transport being closed.
//
// async fn handle_close(&self, node_id: &ZenohId);
// }
