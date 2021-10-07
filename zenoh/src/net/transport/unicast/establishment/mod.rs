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
pub(crate) mod accept;
pub mod authenticator;
pub(crate) mod open;

use super::super::TransportManager;
use super::protocol::core::{PeerId, Property, WhatAmI, ZInt};
use super::protocol::io::{WBuf, ZBuf};
use super::protocol::proto::{Attachment, TransportMessage};
use super::{TransportConfigUnicast, TransportUnicast};
use crate::net::link::LinkUnicast;
use authenticator::AuthenticatedPeerLink;
use std::ops::{Deref, DerefMut};
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::crypto::{BlockCipher, PseudoRng};
use zenoh_util::{zerror, zerror2};

const WBUF_SIZE: usize = 64;

/*************************************/
/*            PROPERTIES             */
/*************************************/
pub(crate) struct EstablishmentProperties(Vec<Property>);

impl Deref for EstablishmentProperties {
    type Target = Vec<Property>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for EstablishmentProperties {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl EstablishmentProperties {
    pub(crate) fn new() -> Self {
        EstablishmentProperties(vec![])
    }

    pub(crate) fn insert(&mut self, p: Property) -> ZResult<()> {
        if self.0.iter().any(|x| x.key == p.key) {
            let e = format!("Property {} already exists", p.key);
            return zerror!(ZErrorKind::Other { descr: e });
        }
        self.0.push(p);
        Ok(())
    }

    pub(crate) fn remove(&mut self, key: ZInt) -> Option<Property> {
        self.0
            .iter()
            .position(|x| x.key == key)
            .map(|i| self.0.remove(i))
    }
}

pub(super) fn attachment_from_properties(ps: &EstablishmentProperties) -> ZResult<Attachment> {
    if ps.is_empty() {
        let e = "Can not create an attachment with zero properties".to_string();
        return zerror!(ZErrorKind::Other { descr: e });
    }

    let mut wbuf = WBuf::new(WBUF_SIZE, false);
    wbuf.write_properties(ps);
    let zbuf: ZBuf = wbuf.into();
    let attachment = Attachment::new(zbuf);
    Ok(attachment)
}

pub(super) fn properties_from_attachment(mut att: Attachment) -> ZResult<EstablishmentProperties> {
    let ps = att.buffer.read_properties().ok_or_else(|| {
        let e = "Error while decoding attachment properties".to_string();
        zerror2!(ZErrorKind::Other { descr: e })
    })?;
    Ok(EstablishmentProperties(ps))
}

/*************************************/
/*             COOKIE                */
/*************************************/
pub struct Cookie {
    whatami: WhatAmI,
    pid: PeerId,
    sn_resolution: ZInt,
    is_qos: bool,
    nonce: ZInt,
}

impl Cookie {
    pub(super) fn encrypt(
        self,
        cipher: &BlockCipher,
        prng: &mut PseudoRng,
        properties: EstablishmentProperties,
    ) -> ZResult<Vec<u8>> {
        macro_rules! zwrite {
            ($op:expr) => {
                if !$op {
                    let e = "Invalid Cookie".to_string();
                    return zerror!(ZErrorKind::InvalidMessage { descr: e });
                }
            };
        }

        let mut wbuf = WBuf::new(64, false);

        zwrite!(wbuf.write_zint(self.whatami.into()));
        zwrite!(wbuf.write_peerid(&self.pid));
        zwrite!(wbuf.write_zint(self.sn_resolution));
        zwrite!(wbuf.write(if self.is_qos { 1 } else { 0 }));
        zwrite!(wbuf.write_zint(self.nonce));
        zwrite!(wbuf.write_properties(properties.as_slice()));

        let serialized = ZBuf::from(wbuf).to_vec();
        let encrypted = cipher.encrypt(serialized, prng);
        Ok(encrypted)
    }

    pub(super) fn decrypt(
        bytes: Vec<u8>,
        cipher: &BlockCipher,
    ) -> ZResult<(Cookie, EstablishmentProperties)> {
        macro_rules! zread {
            ($op:expr) => {
                $op.ok_or_else(|| {
                    let e = "Invalid Cookie".to_string();
                    zerror2!(ZErrorKind::InvalidMessage { descr: e })
                })?
            };
        }

        let decrypted = cipher.decrypt(bytes)?;

        let mut zbuf = ZBuf::from(decrypted);

        let whatami = WhatAmI::try_from(zread!(zbuf.read_zint())).ok_or_else(|| {
            zerror2!(ZErrorKind::InvalidMessage {
                descr: "Invalid Cookie".to_string()
            })
        })?;
        let pid = zread!(zbuf.read_peerid());
        let sn_resolution = zread!(zbuf.read_zint());
        let is_qos = zread!(zbuf.read()) == 1;
        let nonce = zread!(zbuf.read_zint());

        let mut ps = zread!(zbuf.read_properties());
        let mut properties = EstablishmentProperties::new();
        for p in ps.drain(..) {
            properties.insert(p)?;
        }

        let cookie = Cookie {
            whatami,
            pid,
            sn_resolution,
            is_qos,
            nonce,
        };
        Ok((cookie, properties))
    }
}

pub(super) async fn close_link(
    manager: &TransportManager,
    link: &LinkUnicast,
    auth_link: &AuthenticatedPeerLink,
    mut reason: Option<u8>,
) {
    if let Some(reason) = reason.take() {
        // Build the close message
        let peer_id = Some(manager.config.pid);
        let link_only = true;
        let attachment = None;
        let mut message = TransportMessage::make_close(peer_id, reason, link_only, attachment);
        // Send the close message on the link
        let _ = link.write_transport_message(&mut message).await;
    }

    // Close the link
    let _ = link.close().await;
    // Notify the authenticators
    for pa in manager.config.unicast.peer_authenticator.iter() {
        pa.handle_link_err(auth_link).await;
    }
}
