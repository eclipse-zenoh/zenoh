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
#[cfg(feature = "zero-copy")]
mod shm;
mod userpassword;

#[cfg(feature = "zero-copy")]
use super::protocol;
use super::protocol::core::{PeerId, Property, ZInt};
use super::protocol::io::{WBuf, ZBuf};
use crate::net::link::{Link, Locator};
use async_std::sync::Arc;
use async_trait::async_trait;
#[cfg(feature = "zero-copy")]
pub use shm::*;
use std::collections::HashSet;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::ops::Deref;
pub use userpassword::*;
use zenoh_util::core::ZResult;
use zenoh_util::properties::config::*;

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
        _config: &ConfigProperties,
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
    fn id(&self) -> LinkAuthenticatorId;

    async fn handle_new_link(&self, link: &Link) -> ZResult<Option<PeerId>>;

    /// Handle any error on a link. This callback is mainly used to clean-up any internal state
    /// of the authenticator in such a way no unnecessary data is left around
    ///
    /// # Arguments
    /// * `link` - The [`Link`][Link] generating the error
    ///
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

    async fn handle_new_link(&self, _link: &Link) -> ZResult<Option<PeerId>> {
        Ok(None)
    }

    async fn handle_link_err(&self, _link: &Link) {}
}

/*************************************/
/*              PEER                 */
/*************************************/
#[derive(PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum PeerAuthenticatorId {
    Reserved = 0,
    UserPassword = 1,
    Shm = 2,
}

#[derive(Clone)]
pub struct PeerAuthenticator(Arc<dyn PeerAuthenticatorTrait>);

impl PeerAuthenticator {
    pub(crate) async fn from_config(
        config: &ConfigProperties,
    ) -> ZResult<HashSet<PeerAuthenticator>> {
        let mut pas = HashSet::new();

        let mut res = UserPasswordAuthenticator::from_config(config).await?;
        if let Some(pa) = res.take() {
            pas.insert(pa.into());
        }

        #[cfg(feature = "zero-copy")]
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
    pub peer_id: Option<PeerId>,
}

impl fmt::Display for AuthenticatedPeerLink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} => {}", self.src, self.dst)
    }
}

// Authenticated peer transport
pub struct AuthenticatedPeerTransport {
    pub is_shm: bool,
}

impl AuthenticatedPeerTransport {
    pub fn merge(self, other: Self) -> Self {
        Self {
            is_shm: self.is_shm || other.is_shm,
        }
    }
}

impl Default for AuthenticatedPeerTransport {
    fn default() -> Self {
        Self { is_shm: false }
    }
}

pub struct PeerAuthenticatorOutput {
    pub properties: Vec<Property>,
    pub transport: AuthenticatedPeerTransport,
}

impl PeerAuthenticatorOutput {
    pub fn merge(mut self, mut other: Self) -> Self {
        self.properties.append(&mut other.properties);
        Self {
            properties: self.properties,
            transport: self.transport.merge(other.transport),
        }
    }
}

impl Default for PeerAuthenticatorOutput {
    fn default() -> Self {
        Self {
            properties: vec![],
            transport: AuthenticatedPeerTransport::default(),
        }
    }
}

#[async_trait]
pub trait PeerAuthenticatorTrait: Send + Sync {
    fn id(&self) -> PeerAuthenticatorId;

    /// Return the attachment to be included in the InitSyn message.
    ///
    /// # Arguments
    /// * `link`        - The [`AuthenticatedPeerLink`][AuthenticatedPeerLink] the initial InitSyn message will be sent on
    ///
    /// * `peer_id`     - The [`PeerId`][PeerId] of the sender of the InitSyn, i.e., the peer
    ///                   initiating a new transport.
    ///
    async fn get_init_syn_properties(
        &self,
        link: &AuthenticatedPeerLink,
        peer_id: &PeerId,
    ) -> ZResult<PeerAuthenticatorOutput>;

    /// Return the attachment to be included in the InitAck message to be sent
    /// in response of the authenticated InitSyn.
    ///
    /// # Arguments
    /// * `link`            - The [`AuthenticatedPeerLink`][AuthenticatedPeerLink] the InitSyn message was received on
    ///
    /// * `peer_id`         - The [`PeerId`][PeerId] of the sender of the InitSyn message
    ///
    /// * `sn_resolution`   - The sn_resolution negotiated by the sender of the InitSyn message
    ///
    /// * `properties`      - The optional [`Property`][Property] included in the InitSyn message
    ///
    async fn handle_init_syn(
        &self,
        link: &AuthenticatedPeerLink,
        peer_id: &PeerId,
        sn_resolution: ZInt,
        properties: &[Property],
    ) -> ZResult<PeerAuthenticatorOutput>;

    /// Return the attachment to be included in the OpenSyn message to be sent
    /// in response of the authenticated InitAck.
    ///
    /// # Arguments
    /// * `link` - The [`AuthenticatedPeerLink`][AuthenticatedPeerLink] the InitSyn message was received on
    ///
    /// * `peer_id` - The [`PeerId`][PeerId] of the sender of the InitAck message
    ///
    /// * `sn_resolution`   - The sn_resolution negotiated by the sender of the InitAck message
    ///
    /// * `properties`      - The optional [`Property`][Property] included in the InitAck message
    ///
    async fn handle_init_ack(
        &self,
        link: &AuthenticatedPeerLink,
        peer_id: &PeerId,
        sn_resolution: ZInt,
        properties: &[Property],
    ) -> ZResult<PeerAuthenticatorOutput>;

    /// Return the attachment to be included in the OpenAck message to be sent
    /// in response of the authenticated OpenSyn.
    ///
    /// # Arguments
    /// * `link` - The [`AuthenticatedPeerLink`][AuthenticatedPeerLink] the OpenSyn message was received on
    ///
    /// * `properties`      - The optional [`Property`][Property] included in the OpenSyn message
    ///
    async fn handle_open_syn(
        &self,
        link: &AuthenticatedPeerLink,
        properties: &[Property],
    ) -> ZResult<PeerAuthenticatorOutput>;

    /// Auhtenticate the OpenAck. No message is sent back in response to an OpenAck
    ///
    /// # Arguments
    /// * `link` - The [`AuthenticatedPeerLink`][AuthenticatedPeerLink] the OpenAck message was received on
    ///
    /// * `properties`      - The optional [`Property`][Property] included in the OpenAck message
    ///
    async fn handle_open_ack(
        &self,
        link: &AuthenticatedPeerLink,
        properties: &[Property],
    ) -> ZResult<PeerAuthenticatorOutput>;

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
    /// * `peerd_id` - The [`PeerId`][PeerId] of the transport being closed.
    ///
    async fn handle_close(&self, peer_id: &PeerId);
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

    async fn get_init_syn_properties(
        &self,
        _link: &AuthenticatedPeerLink,
        _peer_id: &PeerId,
    ) -> ZResult<PeerAuthenticatorOutput> {
        Ok(PeerAuthenticatorOutput::default())
    }

    async fn handle_init_syn(
        &self,
        _link: &AuthenticatedPeerLink,
        _peer_id: &PeerId,
        _sn_resolution: ZInt,
        _properties: &[Property],
    ) -> ZResult<PeerAuthenticatorOutput> {
        Ok(PeerAuthenticatorOutput::default())
    }

    async fn handle_init_ack(
        &self,
        _link: &AuthenticatedPeerLink,
        _peer_id: &PeerId,
        _sn_resolution: ZInt,
        _properties: &[Property],
    ) -> ZResult<PeerAuthenticatorOutput> {
        Ok(PeerAuthenticatorOutput::default())
    }

    async fn handle_open_syn(
        &self,
        _link: &AuthenticatedPeerLink,
        _properties: &[Property],
    ) -> ZResult<PeerAuthenticatorOutput> {
        Ok(PeerAuthenticatorOutput::default())
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
