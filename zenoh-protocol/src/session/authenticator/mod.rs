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
pub(super) mod properties;
mod userpassword;

use crate::core::{PeerId, Property, ZInt};
use crate::link::{Link, LinkProperties, Locator};
use async_std::sync::Arc;
use async_trait::async_trait;
use std::fmt;
pub use userpassword::*;
use zenoh_util::core::ZResult;

/*************************************/
/*              LINK                 */
/*************************************/
pub type LinkAuthenticator = Arc<dyn LinkAuthenticatorTrait + Send + Sync>;

#[async_trait]
pub trait LinkAuthenticatorTrait {
    async fn handle_new_link(
        &self,
        link: &Link,
        properties: Option<LinkProperties>,
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
        Arc::new(DummyLinkAuthenticator)
    }
}

#[async_trait]
impl LinkAuthenticatorTrait for DummyLinkAuthenticator {
    async fn handle_new_link(
        &self,
        _link: &Link,
        _properties: Option<LinkProperties>,
    ) -> ZResult<Option<PeerId>> {
        Ok(None)
    }

    async fn handle_link_err(&self, _link: &Link) {}
}

/*************************************/
/*              PEER                 */
/*************************************/
pub type PeerAuthenticator = Arc<dyn PeerAuthenticatorTrait + Send + Sync>;

#[derive(Debug, Clone)]
pub struct AuthenticatedPeerLink {
    pub src: Locator,
    pub dst: Locator,
    pub peer_id: Option<PeerId>,
    pub properties: Option<LinkProperties>,
}

impl fmt::Display for AuthenticatedPeerLink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} => {}", self.src, self.dst)
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
    ) -> ZResult<Vec<Property>>;

    /// Return the attachment to be included in the InitAck message to be sent
    /// in response of the authenticated InitSyn.
    ///
    /// # Arguments
    /// * `link`        - The [`AuthenticatedPeerLink`][AuthenticatedPeerLink] the InitSyn message was received on
    ///
    /// * `peer_id`     - The [`PeerId`][PeerId] of the sender of the InitSyn message
    ///
    /// * `attachment`  - The optional [`Attachment`][Attachment] included in the InitSyn message
    ///
    async fn handle_init_syn(
        &self,
        link: &AuthenticatedPeerLink,
        peer_id: &PeerId,
        sn_resolution: ZInt,
        properties: &[Property],
    ) -> ZResult<Vec<Property>>;

    /// Return the attachment to be included in the OpenSyn message to be sent
    /// in response of the authenticated InitAck.
    ///
    /// # Arguments
    /// * `link` - The [`AuthenticatedPeerLink`][AuthenticatedPeerLink] the InitSyn message was received on
    ///
    /// * `peer_id` - The [`PeerId`][PeerId] of the sender of the InitAck message
    ///
    /// * `attachment`  - The optional [`Attachment`][Attachment] included in the InitAck message
    ///
    async fn handle_init_ack(
        &self,
        link: &AuthenticatedPeerLink,
        peer_id: &PeerId,
        sn_resolution: ZInt,
        properties: &[Property],
    ) -> ZResult<Vec<Property>>;

    /// Return the attachment to be included in the OpenAck message to be sent
    /// in response of the authenticated OpenSyn.
    ///
    /// # Arguments
    /// * `link` - The [`AuthenticatedPeerLink`][AuthenticatedPeerLink] the OpenSyn message was received on
    ///
    /// * `peer_id` - The [`PeerId`][PeerId] of the sender of the OpenSyn message
    ///
    /// * `attachment`  - The optional [`Attachment`][Attachment] included in the OpenSyn message
    ///
    async fn handle_open_syn(
        &self,
        link: &AuthenticatedPeerLink,
        properties: &[Property],
    ) -> ZResult<Vec<Property>>;

    /// Auhtenticate the OpenAck. No message is sent back in response to an OpenAck
    ///
    /// # Arguments
    /// * `link` - The [`AuthenticatedPeerLink`][AuthenticatedPeerLink] the OpenAck message was received on
    ///
    /// * `peer_id` - The [`PeerId`][PeerId] of the sender of the OpenAck message
    ///
    /// * `attachment`  - The optional [`Attachment`][Attachment] included in the OpenAck message
    ///
    async fn handle_open_ack(
        &self,
        link: &AuthenticatedPeerLink,
        properties: &[Property],
    ) -> ZResult<()>;

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

pub struct DummyPeerAuthenticator;

impl DummyPeerAuthenticator {
    pub fn make() -> PeerAuthenticator {
        Arc::new(DummyPeerAuthenticator)
    }
}

#[async_trait]
impl PeerAuthenticatorTrait for DummyPeerAuthenticator {
    async fn get_init_syn_properties(
        &self,
        _link: &AuthenticatedPeerLink,
        _peer_id: &PeerId,
    ) -> ZResult<Vec<Property>> {
        Ok(vec![])
    }

    async fn handle_init_syn(
        &self,
        _link: &AuthenticatedPeerLink,
        _peer_id: &PeerId,
        _sn_resolution: ZInt,
        _properties: &[Property],
    ) -> ZResult<Vec<Property>> {
        Ok(vec![])
    }

    async fn handle_init_ack(
        &self,
        _link: &AuthenticatedPeerLink,
        _peer_id: &PeerId,
        _sn_resolution: ZInt,
        _properties: &[Property],
    ) -> ZResult<Vec<Property>> {
        Ok(vec![])
    }

    async fn handle_open_syn(
        &self,
        _link: &AuthenticatedPeerLink,
        _properties: &[Property],
    ) -> ZResult<Vec<Property>> {
        Ok(vec![])
    }

    async fn handle_open_ack(
        &self,
        _link: &AuthenticatedPeerLink,
        _properties: &[Property],
    ) -> ZResult<()> {
        Ok(())
    }

    async fn handle_link_err(&self, _link: &AuthenticatedPeerLink) {}

    async fn handle_close(&self, _peer_id: &PeerId) {}
}
