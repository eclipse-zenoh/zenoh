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
#[cfg(feature = "auth_pubkey")]
mod pubkey;
#[cfg(feature = "shared-memory")]
mod shm;
#[cfg(feature = "auth_usrpwd")]
mod userpassword;

use crate::config::Config;
use crate::net::link::{Link, Locator};
use crate::net::protocol::core::SeqNumBytes;
use crate::net::protocol::core::{ZInt, ZenohId};
#[cfg(feature = "auth_usrpwd")]
use crate::net::protocol::io::{WBuf, ZBuf};
use crate::net::transport::unicast::establishment::Cookie;
use async_std::sync::Arc;
use async_trait::async_trait;
#[cfg(feature = "auth_pubkey")]
pub use pubkey::*;
#[cfg(feature = "shared-memory")]
pub use shm::*;
use std::collections::HashSet;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::ops::Deref;
#[cfg(feature = "auth_usrpwd")]
pub use userpassword::*;
use zenoh_util::core::Result as ZResult;

/*************************************/
/*              LINK                 */
/*************************************/
#[derive(PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum LinkAuthenticatorId {
    Reserved = 0,
}

pub struct LinkAuthenticator(Arc<dyn LinkUnicastAuthenticatorTrait + Send + Sync>);

impl LinkAuthenticator {
    pub(crate) async fn from_config(
        _config: &crate::config::Config,
    ) -> ZResult<HashSet<LinkAuthenticator>> {
        Ok(HashSet::new())
    }
}

impl Deref for LinkAuthenticator {
    type Target = Arc<dyn LinkUnicastAuthenticatorTrait + Send + Sync>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Eq for LinkAuthenticator {}

impl PartialEq for LinkAuthenticator {
    fn eq(&self, other: &Self) -> bool {
        self.id() == other.id()
    }
}

impl Hash for LinkAuthenticator {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id().hash(state);
    }
}

/*************************************/
/*           DUMMY LINK              */
/*************************************/
#[async_trait]
pub trait LinkUnicastAuthenticatorTrait {
    /// Return the ID of this authenticator.
    fn id(&self) -> LinkAuthenticatorId;

    /// Close the authenticator
    async fn close(&self);

    /// Handle new links
    async fn handle_new_link(&self, link: &Link) -> ZResult<Option<ZenohId>>;

    /// Handle any error on a link. This callback is mainly used to clean-up any internal state
    /// of the authenticator in such a way no unnecessary data is left around.
    async fn handle_link_err(&self, link: &Link);
}

pub struct DummyLinkUnicastAuthenticator;

impl DummyLinkUnicastAuthenticator {
    pub fn make() -> LinkAuthenticator {
        LinkAuthenticator(Arc::new(DummyLinkUnicastAuthenticator))
    }
}

#[async_trait]
impl LinkUnicastAuthenticatorTrait for DummyLinkUnicastAuthenticator {
    fn id(&self) -> LinkAuthenticatorId {
        LinkAuthenticatorId::Reserved
    }

    async fn close(&self) {}

    async fn handle_new_link(&self, _link: &Link) -> ZResult<Option<ZenohId>> {
        Ok(None)
    }

    async fn handle_link_err(&self, _link: &Link) {}
}

/*************************************/
/*              PEER                 */
/*************************************/
#[derive(PartialEq, Eq, Hash)]
pub enum PeerAuthenticatorId {
    Reserved = 0,
    Shm = 1,
    UserPassword = 2,
    PublicKey = 3,
}

impl PeerAuthenticatorId {
    pub const fn id(self) -> ZInt {
        self as ZInt
    }
}

impl From<PeerAuthenticatorId> for ZInt {
    fn from(pa: PeerAuthenticatorId) -> ZInt {
        pa.id()
    }
}

#[derive(Clone)]
pub struct PeerAuthenticator(Arc<dyn PeerAuthenticatorTrait>);

impl PeerAuthenticator {
    pub(crate) async fn from_config(config: &Config) -> ZResult<HashSet<PeerAuthenticator>> {
        let mut pas = HashSet::new();

        #[cfg(feature = "auth_pubkey")]
        {
            let mut res = PubKeyAuthenticator::from_config(config).await?;
            if let Some(pa) = res.take() {
                pas.insert(pa.into());
            }
        }

        #[cfg(feature = "auth_usrpwd")]
        {
            let mut res = UserPasswordAuthenticator::from_config(config).await?;
            if let Some(pa) = res.take() {
                pas.insert(pa.into());
            }
        }

        #[cfg(feature = "shared-memory")]
        {
            let mut res = SharedMemoryAuthenticator::from_config(config).await?;
            if let Some(pa) = res.take() {
                pas.insert(pa.into());
            }
        }

        Ok(pas)
    }
}

impl Deref for PeerAuthenticator {
    type Target = Arc<dyn PeerAuthenticatorTrait>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Eq for PeerAuthenticator {}

impl PartialEq for PeerAuthenticator {
    fn eq(&self, other: &Self) -> bool {
        self.id() == other.id()
    }
}

impl Hash for PeerAuthenticator {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id().hash(state);
    }
}

// Authenticated peer link
#[derive(Debug)]
pub struct AuthenticatedPeerLink {
    pub src: Locator,
    pub dst: Locator,
    pub zid: Option<ZenohId>,
}

impl fmt::Display for AuthenticatedPeerLink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} => {}", self.src, self.dst)
    }
}

#[async_trait]
pub trait PeerAuthenticatorTrait: Send + Sync {
    /// Return the ID of this authenticator.
    fn id(&self) -> PeerAuthenticatorId;

    /// Close the authenticator
    async fn close(&self);

    /// Return the attachment to be included in the InitSyn message.
    ///
    /// # Arguments
    /// * `link`        - The [`AuthenticatedPeerLink`][AuthenticatedPeerLink] the initial InitSyn message will be sent on
    ///
    /// * `zid`     - The [`ZenohId`][ZenohId] of the sender of the InitSyn, i.e., the peer
    ///                   initiating a new transport.
    ///
    async fn get_init_syn_properties(
        &self,
        link: &AuthenticatedPeerLink,
        zid: &ZenohId,
    ) -> ZResult<Option<Vec<u8>>>;

    /// Return the attachment to be included in the InitAck message to be sent
    /// in response of the authenticated InitSyn.
    ///
    /// # Arguments
    /// * `link`            - The [`AuthenticatedPeerLink`][AuthenticatedPeerLink] the InitSyn message was received on
    ///
    /// * `cookie`          - The Cookie containing the internal state
    ///
    /// * `property`        - The optional `Property` included in the InitSyn message
    ///
    async fn handle_init_syn(
        &self,
        link: &AuthenticatedPeerLink,
        cookie: &Cookie,
        property: Option<Vec<u8>>,
    ) -> ZResult<(Option<Vec<u8>>, Option<Vec<u8>>)>; // (Attachment, Cookie)

    /// Return the attachment to be included in the OpenSyn message to be sent
    /// in response of the authenticated InitAck.
    ///
    /// # Arguments
    /// * `link`        - The [`AuthenticatedPeerLink`][AuthenticatedPeerLink] the InitSyn message was received on
    ///
    /// * `zid`         - The [`ZenohId`][ZenohId] of the sender of the InitAck message
    ///
    /// * `sn_bytes`    - The sn_bytes negotiated by the sender of the InitAck message
    ///
    /// * `properties`  - The optional `Property` included in the InitAck message
    ///
    async fn handle_init_ack(
        &self,
        link: &AuthenticatedPeerLink,
        zid: &ZenohId,
        sn_bytes: SeqNumBytes,
        property: Option<Vec<u8>>,
    ) -> ZResult<Option<Vec<u8>>>;

    /// Return the attachment to be included in the OpenAck message to be sent
    /// in response of the authenticated OpenSyn.
    ///
    /// # Arguments
    /// * `link` - The [`AuthenticatedPeerLink`][AuthenticatedPeerLink] the OpenSyn message was received on
    ///
    /// * `properties`      - The optional `Property` included in the OpenSyn message
    ///
    /// * `cookie`          - The optional `Property` included in the OpenSyn message
    ///
    async fn handle_open_syn(
        &self,
        link: &AuthenticatedPeerLink,
        cookie: &Cookie,
        property: (Option<Vec<u8>>, Option<Vec<u8>>), // (Attachment, Cookie)
    ) -> ZResult<Option<Vec<u8>>>;

    /// Auhtenticate the OpenAck. No message is sent back in response to an OpenAck
    ///
    /// # Arguments
    /// * `link` - The [`AuthenticatedPeerLink`][AuthenticatedPeerLink] the OpenAck message was received on
    ///
    /// * `properties`      - The optional `Property` included in the OpenAck message
    ///
    async fn handle_open_ack(
        &self,
        link: &AuthenticatedPeerLink,
        property: Option<Vec<u8>>,
    ) -> ZResult<Option<Vec<u8>>>;

    /// Handle any error on a link. This callback is mainly used to clean-up any internal state
    /// of the authenticator in such a way no unnecessary data is left around
    ///
    /// # Arguments
    /// * `link` - The [`AuthenticatedPeerLink`][AuthenticatedPeerLink] generating the error
    ///
    async fn handle_link_err(&self, link: &AuthenticatedPeerLink);

    /// Handle any error on a link. This callback is mainly used to clean-up any internal state
    /// of the authenticator in such a way no unnecessary data is left around
    ///
    /// # Arguments
    /// * `peerd_id` - The [`ZenohId`][ZenohId] of the transport being closed.
    ///
    async fn handle_close(&self, zid: &ZenohId);
}

/*************************************/
/*           DUMMY PEER              */
/*************************************/
pub struct DummyPeerAuthenticator;

impl DummyPeerAuthenticator {
    pub fn make() -> PeerAuthenticator {
        PeerAuthenticator(Arc::new(DummyPeerAuthenticator))
    }
}

#[async_trait]
impl PeerAuthenticatorTrait for DummyPeerAuthenticator {
    fn id(&self) -> PeerAuthenticatorId {
        PeerAuthenticatorId::Reserved
    }

    async fn close(&self) {}

    async fn get_init_syn_properties(
        &self,
        _link: &AuthenticatedPeerLink,
        _zid: &ZenohId,
    ) -> ZResult<Option<Vec<u8>>> {
        Ok(None)
    }

    async fn handle_init_syn(
        &self,
        _link: &AuthenticatedPeerLink,
        _cookie: &Cookie,
        _property: Option<Vec<u8>>,
    ) -> ZResult<(Option<Vec<u8>>, Option<Vec<u8>>)> {
        Ok((None, None))
    }

    async fn handle_init_ack(
        &self,
        _link: &AuthenticatedPeerLink,
        _zid: &ZenohId,
        _sn_bytes: SeqNumBytes,
        _property: Option<Vec<u8>>,
    ) -> ZResult<Option<Vec<u8>>> {
        Ok(None)
    }

    async fn handle_open_syn(
        &self,
        _link: &AuthenticatedPeerLink,
        _cookie: &Cookie,
        _property: (Option<Vec<u8>>, Option<Vec<u8>>),
    ) -> ZResult<Option<Vec<u8>>> {
        Ok(None)
    }

    async fn handle_open_ack(
        &self,
        _link: &AuthenticatedPeerLink,
        _property: Option<Vec<u8>>,
    ) -> ZResult<Option<Vec<u8>>> {
        Ok(None)
    }

    async fn handle_link_err(&self, _link: &AuthenticatedPeerLink) {}

    async fn handle_close(&self, _zid: &ZenohId) {}
}
