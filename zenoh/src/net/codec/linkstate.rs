//
// Copyright (c) 2023 ZettaScale Technology
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
use core::convert::TryFrom;

use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
};
use zenoh_codec::{RCodec, WCodec, Zenoh080};
use zenoh_protocol::{
    common::imsg,
    core::{Locator, WhatAmI, ZenohIdProto},
};

use super::Zenoh080Routing;
use crate::net::protocol::{
    linkstate,
    linkstate::{LinkState, LinkStateList},
};

// LinkState
impl<W> WCodec<&LinkState, &mut W> for Zenoh080Routing
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &LinkState) -> Self::Output {
        let codec = Zenoh080::new();
        // Options
        let mut options = 0;
        if x.zid.is_some() {
            options |= linkstate::PID;
        }
        if x.whatami.is_some() {
            options |= linkstate::WAI;
        }
        if x.locators.is_some() {
            options |= linkstate::LOC;
        }
        if x.link_weights.is_some() {
            options |= linkstate::WGT;
        }
        codec.write(&mut *writer, options)?;

        // Body
        codec.write(&mut *writer, x.psid)?;
        codec.write(&mut *writer, x.sn)?;
        if let Some(zid) = x.zid.as_ref() {
            codec.write(&mut *writer, zid)?;
        }
        if let Some(wai) = x.whatami {
            let wai: u8 = wai.into();
            codec.write(&mut *writer, wai)?;
        }
        if let Some(locators) = x.locators.as_ref() {
            codec.write(&mut *writer, locators.as_slice())?;
        }
        codec.write(&mut *writer, x.links.len())?;
        for l in x.links.iter() {
            codec.write(&mut *writer, *l)?;
        }
        if let Some(link_weights) = x.link_weights.as_ref() {
            // do not write len since it is the same as that of links
            for w in link_weights.iter() {
                codec.write(&mut *writer, w)?;
            }
        }

        Ok(())
    }
}

impl<R> RCodec<LinkState, &mut R> for Zenoh080Routing
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<LinkState, Self::Error> {
        let codec = Zenoh080::new();
        let options: u64 = codec.read(&mut *reader)?;
        let psid: u64 = codec.read(&mut *reader)?;
        let sn: u64 = codec.read(&mut *reader)?;
        let zid = if imsg::has_option(options, linkstate::PID) {
            let zid: ZenohIdProto = codec.read(&mut *reader)?;
            Some(zid)
        } else {
            None
        };
        let whatami = if imsg::has_option(options, linkstate::WAI) {
            let wai: u8 = codec.read(&mut *reader)?;
            Some(WhatAmI::try_from(wai).map_err(|_| DidntRead)?)
        } else {
            None
        };
        let locators = if imsg::has_option(options, linkstate::LOC) {
            let locs: Vec<Locator> = codec.read(&mut *reader)?;
            Some(locs)
        } else {
            None
        };
        let links_len: usize = codec.read(&mut *reader)?;
        let mut links: Vec<u64> = Vec::with_capacity(links_len);
        for _ in 0..links_len {
            let l: u64 = codec.read(&mut *reader)?;
            links.push(l);
        }

        let link_weights = if imsg::has_option(options, linkstate::WGT) {
            // number of weights is the same as number of links
            let mut weights: Vec<u16> = Vec::with_capacity(links_len);
            for _ in 0..links_len {
                let w: u16 = codec.read(&mut *reader)?;
                weights.push(w);
            }
            Some(weights)
        } else {
            None
        };

        Ok(LinkState {
            psid,
            sn,
            zid,
            whatami,
            locators,
            links,
            link_weights,
        })
    }
}

// LinkStateList
impl<W> WCodec<&LinkStateList, &mut W> for Zenoh080Routing
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &LinkStateList) -> Self::Output {
        let codec = Zenoh080::new();

        codec.write(&mut *writer, x.link_states.len())?;
        for ls in x.link_states.iter() {
            self.write(&mut *writer, ls)?;
        }

        Ok(())
    }
}

impl<R> RCodec<LinkStateList, &mut R> for Zenoh080Routing
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<LinkStateList, Self::Error> {
        let codec = Zenoh080::new();

        let len: usize = codec.read(&mut *reader)?;
        let mut link_states = Vec::with_capacity(len);
        for _ in 0..len {
            let ls: LinkState = self.read(&mut *reader)?;
            link_states.push(ls);
        }

        Ok(LinkStateList { link_states })
    }
}
