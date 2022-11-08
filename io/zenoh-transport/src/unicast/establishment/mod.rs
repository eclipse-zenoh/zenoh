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
pub(crate) mod accept;
pub mod authenticator;
pub(crate) mod open;

use super::super::TransportManager;
use super::protocol::core::{Property, WhatAmI, ZInt, ZenohId};
use super::protocol::io::{WBuf, ZBuf};
use super::protocol::proto::{Attachment, TransportMessage};
use super::{TransportConfigUnicast, TransportPeer, TransportUnicast};
use authenticator::AuthenticatedPeerLink;
use rand::Rng;
use std::ops::{Deref, DerefMut};
use std::time::Duration;
use zenoh_buffers::buffer::CopyBuffer;
use zenoh_buffers::reader::{HasReader, Reader};
use zenoh_buffers::SplitBuffer;
use zenoh_core::{bail, zerror};
use zenoh_core::{zasynclock, zasyncread, Result as ZResult};
use zenoh_crypto::{BlockCipher, PseudoRng};
use zenoh_link::{Link, LinkUnicast};
use zenoh_protocol::io::{WBufCodec, ZBufCodec};

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
            bail!("Property {} already exists", p.key)
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
        bail!("Can not create an attachment with zero properties")
    }

    let mut wbuf = WBuf::new(WBUF_SIZE, false);
    wbuf.write_properties(ps);
    let zbuf: ZBuf = wbuf.into();
    let attachment = Attachment::new(zbuf);
    Ok(attachment)
}

pub(super) fn properties_from_attachment(att: Attachment) -> ZResult<EstablishmentProperties> {
    let ps = att
        .buffer
        .reader()
        .read_properties()
        .ok_or_else(|| zerror!("Error while decoding attachment properties"))?;
    Ok(EstablishmentProperties(ps))
}

/*************************************/
/*             COOKIE                */
/*************************************/
pub struct Cookie {
    whatami: WhatAmI,
    zid: ZenohId,
    sn_resolution: ZInt,
    is_qos: bool,
    nonce: ZInt,
}
pub type CookieHash = Vec<u8>;

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
                    bail!("Invalid Cookie")
                }
            };
        }

        let mut wbuf = WBuf::new(64, false);

        zwrite!(wbuf.write_zint(self.whatami.into()));
        zwrite!(wbuf.write_zid(&self.zid));
        zwrite!(wbuf.write_zint(self.sn_resolution));
        zwrite!(wbuf.write_byte(u8::from(self.is_qos)).is_some());
        zwrite!(wbuf.write_zint(self.nonce));
        zwrite!(wbuf.write_properties(properties.as_slice()));

        let serialized = ZBuf::from(wbuf).contiguous().into_owned();
        let encrypted = cipher.encrypt(serialized, prng);
        Ok(encrypted)
    }

    pub(super) fn decrypt(
        bytes: Vec<u8>,
        cipher: &BlockCipher,
    ) -> ZResult<(Cookie, EstablishmentProperties)> {
        macro_rules! zread {
            ($op:expr) => {
                $op.ok_or_else(|| zerror!("Invalid Cookie"))?
            };
        }

        let decrypted = cipher.decrypt(bytes)?;

        let zbuf = ZBuf::from(decrypted);
        let mut reader = zbuf.reader();

        let whatami = WhatAmI::try_from(zread!(reader.read_zint()))
            .ok_or_else(|| zerror!("Invalid Cookie"))?;
        let zid = zread!(reader.read_zid());
        let sn_resolution = zread!(reader.read_zint());
        let is_qos = zread!(reader.read_byte()) == 1;
        let nonce = zread!(reader.read_zint());

        let mut ps = zread!(reader.read_properties());
        let mut properties = EstablishmentProperties::new();
        for p in ps.drain(..) {
            properties.insert(p)?;
        }

        let cookie = Cookie {
            whatami,
            zid,
            sn_resolution,
            is_qos,
            nonce,
        };
        Ok((cookie, properties))
    }
}

pub(super) async fn close_link(
    link: &LinkUnicast,
    manager: &TransportManager,
    auth_link: &AuthenticatedPeerLink,
    mut reason: Option<u8>,
) {
    if let Some(reason) = reason.take() {
        // Build the close message
        let peer_id = Some(manager.config.zid);
        let link_only = true;
        let attachment = None;
        let mut message = TransportMessage::make_close(peer_id, reason, link_only, attachment);
        // Send the close message on the link
        let _ = link.write_transport_message(&mut message).await;
    }

    // Close the link
    let _ = link.close().await;
    // Notify the authenticators
    for pa in zasyncread!(manager.state.unicast.peer_authenticator).iter() {
        pa.handle_link_err(auth_link).await;
    }
}

/*************************************/
/*            TRANSPORT              */
/*************************************/
pub(super) struct InputInit {
    pub(super) zid: ZenohId,
    pub(super) whatami: WhatAmI,
    pub(super) sn_resolution: ZInt,
    pub(super) is_shm: bool,
    pub(super) is_qos: bool,
}
async fn transport_init(
    manager: &TransportManager,
    input: self::InputInit,
) -> ZResult<TransportUnicast> {
    // Initialize the transport if it is new
    let initial_sn_tx = zasynclock!(manager.prng).gen_range(0..input.sn_resolution);

    let config = TransportConfigUnicast {
        peer: input.zid,
        whatami: input.whatami,
        sn_resolution: input.sn_resolution,
        is_shm: input.is_shm,
        is_qos: input.is_qos,
        initial_sn_tx,
    };

    manager.init_transport_unicast(config)
}

pub(super) struct InputFinalize {
    pub(super) transport: TransportUnicast,
    pub(super) lease: Duration,
}
// Finalize the transport, notify the callback and start the link tasks
pub(super) async fn transport_finalize(
    link: &LinkUnicast,
    manager: &TransportManager,
    input: self::InputFinalize,
) -> ZResult<()> {
    // Retrive the transport's transport
    let transport = input.transport.get_inner()?;

    // Start the TX loop
    let keep_alive = manager.config.unicast.lease / manager.config.unicast.keep_alive as u32;
    transport.start_tx(
        link,
        &manager.tx_executor,
        keep_alive,
        manager.config.batch_size,
    )?;

    // Assign a callback if the transport is new
    // Keep the lock to avoid concurrent new_transport and closing/closed notifications
    let a_guard = transport.get_alive().await;
    match transport.get_callback() {
        Some(callback) => {
            // Notify the transport handler there is a new link on this transport
            callback.new_link(Link::from(link));
        }
        None => {
            let peer = TransportPeer {
                zid: transport.get_zid(),
                whatami: transport.get_whatami(),
                is_qos: transport.is_qos(),
                is_shm: transport.is_shm(),
                links: vec![Link::from(link)],
            };
            // Notify the transport handler that there is a new transport and get back a callback
            // NOTE: the read loop of the link the open message was sent on remains blocked
            //       until new_unicast() returns. The read_loop in the various links
            //       waits for any eventual transport to associate to.
            let callback = manager
                .config
                .handler
                .new_unicast(peer, input.transport.clone())?;
            // Set the callback on the transport
            transport.set_callback(callback);
        }
    }
    drop(a_guard);

    // Start the RX loop
    transport.start_rx(link, input.lease)?;

    Ok(())
}
