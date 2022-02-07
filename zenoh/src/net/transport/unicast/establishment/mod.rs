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
use super::protocol::core::{SeqNumBytes, WhatAmI, ZInt, ZenohId};
use super::protocol::io::{WBuf, ZBuf};
use super::{TransportConfigUnicast, TransportPeer, TransportUnicast};
use crate::net::link::{Link, LinkUnicast};
use crate::net::protocol::message::{Close, CloseReason, WireProperties};
use authenticator::AuthenticatedPeerLink;
use rand::Rng;
use std::convert::TryFrom;
use std::time::Duration;
use zenoh_util::core::Result as ZResult;
use zenoh_util::crypto::{BlockCipher, PseudoRng};
use zenoh_util::{bail, zerror};

/*************************************/
/*             COOKIE                */
/*************************************/
pub struct Cookie {
    whatami: WhatAmI,
    zid: ZenohId,
    sn_bytes: SeqNumBytes,
    is_qos: bool,
    nonce: ZInt,
}
pub type CookieHash = Vec<u8>;

impl Cookie {
    pub(super) fn encrypt(
        self,
        cipher: &BlockCipher,
        prng: &mut PseudoRng,
        properties: WireProperties,
    ) -> ZResult<Vec<u8>> {
        macro_rules! zwrite {
            ($op:expr) => {
                if !$op {
                    bail!("Invalid Cookie")
                }
            };
        }

        let mut wbuf = WBuf::new(64, false);

        zwrite!(wbuf.write(self.whatami.into()));
        zwrite!(wbuf.write_zenohid(&self.zid));
        zwrite!(wbuf.write(self.sn_bytes.value()));
        zwrite!(wbuf.write(if self.is_qos { 1 } else { 0 }));
        zwrite!(wbuf.write_zint(self.nonce));
        zwrite!(wbuf.write_wire_properties(&properties));

        let serialized = ZBuf::from(wbuf).to_vec();
        let encrypted = cipher.encrypt(serialized, prng);
        Ok(encrypted)
    }

    pub(super) fn decrypt(
        bytes: Vec<u8>,
        cipher: &BlockCipher,
    ) -> ZResult<(Cookie, WireProperties)> {
        macro_rules! zread {
            ($op:expr) => {
                $op.ok_or_else(|| zerror!("Invalid Cookie"))?
            };
        }

        let decrypted = cipher.decrypt(bytes)?;

        let mut zbuf = ZBuf::from(decrypted);

        let whatami =
            WhatAmI::try_from(zread!(zbuf.read())).map_err(|_| zerror!("Invalid Cookie"))?;
        let zid = zread!(zbuf.read_zenohid());
        let sn_bytes =
            SeqNumBytes::try_from(zread!(zbuf.read())).map_err(|_| zerror!("Invalid Cookie"))?;
        let is_qos = zread!(zbuf.read()) == 1;
        let nonce = zread!(zbuf.read_zint());

        let properties = zread!(zbuf.read_wire_properties());

        let cookie = Cookie {
            whatami,
            zid,
            sn_bytes,
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
    mut reason: Option<CloseReason>,
) {
    if let Some(reason) = reason.take() {
        // Build the close message
        let mut message = Close::new(reason);
        // Send the close message on the link
        let _ = link.send(&mut message).await;
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
    pub(super) sn_bytes: SeqNumBytes,
    pub(super) is_shm: bool,
    pub(super) is_qos: bool,
}
async fn transport_init(
    manager: &TransportManager,
    input: self::InputInit,
) -> ZResult<TransportUnicast> {
    // Initialize the transport if it is new
    let sn_resolution = input.sn_bytes.resolution();
    let initial_sn_tx = zasynclock!(manager.prng).gen_range(0..sn_resolution);

    let config = TransportConfigUnicast {
        peer: input.zid,
        whatami: input.whatami,
        sn_bytes: input.sn_bytes,
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
    let _ = transport.start_tx(
        link,
        manager.config.unicast.keep_alive,
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
    let _ = transport.start_rx(link, input.lease)?;

    Ok(())
}
