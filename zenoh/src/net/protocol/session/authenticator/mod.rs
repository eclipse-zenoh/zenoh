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
pub(super) mod attachment;
#[cfg(feature = "zero-copy")]
mod shm;
mod userpassword;

use super::core;
use super::io;

use super::core::{PeerId, Property, ZInt};
use super::io::{RBuf, WBuf};
use super::link::{Link, Locator, LocatorProperty};
use async_std::sync::Arc;
use async_trait::async_trait;
#[cfg(feature = "zero-copy")]
pub use shm::*;
use std::fmt;
use std::ops::Deref;
pub use userpassword::*;
use zenoh_util::core::ZResult;
use zenoh_util::properties::config::*;

/*************************************/
/*              LINK                 */
/*************************************/
pub struct LinkAuthenticator(Arc<dyn LinkAuthenticatorTrait + Send + Sync>);

impl LinkAuthenticator {
    pub(crate) async fn from_properties(
        _config: &ConfigProperties,
    ) -> ZResult<Vec<LinkAuthenticator>> {
        let las: Vec<LinkAuthenticator> = vec![];
        Ok(las)
    }
}

impl Deref for LinkAuthenticator {
    type Target = Arc<dyn LinkAuthenticatorTrait + Send + Sync>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/*************************************/
/*           DUMMY LINK              */
/*************************************/
#[async_trait]
pub trait LinkAuthenticatorTrait {
    async fn handle_new_link(
        &self,
        link: &Link,
        properties: Option<&LocatorProperty>,
    ) -> ZResult<Option<PeerId>>;

    /// Handle any error on a link. This callback is mainly used to clean-up any internal state
    /// of the authenticator in such a way no unnecessary data is left around
    ///
    /// # Arguments
    /// * `link` - The [`Link`][Link] generating the error
    ///
    async fn handle_link_err(&self, link: &Link);
}

pub struct DummyLinkAuthenticator;

impl DummyLinkAuthenticator {
    pub fn make() -> LinkAuthenticator {
        LinkAuthenticator(Arc::new(DummyLinkAuthenticator))
    }
}

#[async_trait]
impl LinkAuthenticatorTrait for DummyLinkAuthenticator {
    async fn handle_new_link(
        &self,
        _link: &Link,
        _properties: Option<&LocatorProperty>,
    ) -> ZResult<Option<PeerId>> {
        Ok(None)
    }

    async fn handle_link_err(&self, _link: &Link) {}
}

/*************************************/
/*              PEER                 */
/*************************************/
#[derive(Clone)]
pub struct PeerAuthenticator(Arc<dyn PeerAuthenticatorTrait + Send + Sync>);

impl PeerAuthenticator {
    pub(crate) async fn from_properties(
        config: &ConfigProperties,
    ) -> ZResult<Vec<PeerAuthenticator>> {
        let mut pas: Vec<PeerAuthenticator> = vec![];

        let mut res = UserPasswordAuthenticator::from_properties(config).await?;
        if let Some(pa) = res.take() {
            pas.push(pa.into());
        }

        #[cfg(feature = "zero-copy")]
        {
            let mut res = SharedMemoryAuthenticator::from_properties(config).await?;
            if let Some(pa) = res.take() {
                pas.push(pa.into());
            }
        }

        Ok(pas)
    }
}

impl Deref for PeerAuthenticator {
    type Target = Arc<dyn PeerAuthenticatorTrait + Send + Sync>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

// Authenticated peer link
#[derive(Debug)]
pub struct AuthenticatedPeerLink {
    pub src: Locator,
    pub dst: Locator,
    pub peer_id: Option<PeerId>,
    pub properties: Option<LocatorProperty>,
}

impl fmt::Display for AuthenticatedPeerLink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} => {}", self.src, self.dst)
    }
}

// Authenticated peer session
pub struct AuthenticatedPeerSession {
    pub is_local: bool,
}

impl AuthenticatedPeerSession {
    pub fn merge(self, other: Self) -> Self {
        Self {
            is_local: self.is_local || other.is_local,
        }
    }
}

impl Default for AuthenticatedPeerSession {
    fn default() -> Self {
        Self { is_local: false }
    }
}

pub struct PeerAuthenticatorOutput {
    pub properties: Vec<Property>,
    pub session: AuthenticatedPeerSession,
}

impl PeerAuthenticatorOutput {
    pub fn merge(mut self, mut other: Self) -> Self {
        self.properties.append(&mut other.properties);
        Self {
            properties: self.properties,
            session: self.session.merge(other.session),
        }
    }
}

impl Default for PeerAuthenticatorOutput {
    fn default() -> Self {
        Self {
            properties: vec![],
            session: AuthenticatedPeerSession::default(),
        }
    }
}

#[async_trait]
pub trait PeerAuthenticatorTrait {
    /// Return the attachment to be included in the InitSyn message.
    ///
    /// # Arguments
    /// * `link`        - The [`AuthenticatedPeerLink`][AuthenticatedPeerLink] the initial InitSyn message will be sent on
    ///
    /// * `peer_id`     - The [`PeerId`][PeerId] of the sender of the InitSyn, i.e., the peer
    ///                   initiating a new session.
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
    /// * `properties`      - The optional [`Property`][property] included in the InitSyn message
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
    /// * `properties`      - The optional [`Property`][property] included in the InitAck message
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
    /// * `properties`      - The optional [`Property`][property] included in the OpenSyn message
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
    /// * `properties`      - The optional [`Property`][property] included in the OpenAck message
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
    /// * `peerd_id` - The [`PeerId`][PeerId] of the session being closed.
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
