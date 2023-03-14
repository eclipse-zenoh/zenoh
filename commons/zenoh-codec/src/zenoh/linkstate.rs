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
use crate::{RCodec, WCodec, Zenoh080, Zenoh080Header};
use alloc::vec::Vec;
use core::convert::TryFrom;
use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
};
use zenoh_protocol::{
    common::imsg,
    core::{Locator, WhatAmI, ZenohId},
    zenoh::{zmsg, LinkState, LinkStateList},
};

// LinkState
impl<W> WCodec<&LinkState, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &LinkState) -> Self::Output {
        // Options
        let mut options = 0;
        if x.zid.is_some() {
            options |= zmsg::link_state::PID;
        }
        if x.whatami.is_some() {
            options |= zmsg::link_state::WAI;
        }
        if x.locators.is_some() {
            options |= zmsg::link_state::LOC;
        }
        self.write(&mut *writer, options)?;

        // Body
        self.write(&mut *writer, &x.psid)?;
        self.write(&mut *writer, x.sn)?;
        if let Some(zid) = x.zid.as_ref() {
            self.write(&mut *writer, zid)?;
        }
        if let Some(wai) = x.whatami {
            let wai: u8 = wai.into();
            self.write(&mut *writer, wai)?;
        }
        if let Some(locators) = x.locators.as_ref() {
            self.write(&mut *writer, locators.as_slice())?;
        }
        self.write(&mut *writer, x.links.len())?;
        for l in x.links.iter() {
            self.write(&mut *writer, *l)?;
        }

        Ok(())
    }
}

impl<R> RCodec<LinkState, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<LinkState, Self::Error> {
        let options: u64 = self.read(&mut *reader)?;
        let psid: u64 = self.read(&mut *reader)?;
        let sn: u64 = self.read(&mut *reader)?;
        let zid = if imsg::has_option(options, zmsg::link_state::PID) {
            let zid: ZenohId = self.read(&mut *reader)?;
            Some(zid)
        } else {
            None
        };
        let whatami = if imsg::has_option(options, zmsg::link_state::WAI) {
            let wai: u8 = self.read(&mut *reader)?;
            Some(WhatAmI::try_from(wai).map_err(|_| DidntRead)?)
        } else {
            None
        };
        let locators = if imsg::has_option(options, zmsg::link_state::LOC) {
            let locs: Vec<Locator> = self.read(&mut *reader)?;
            Some(locs)
        } else {
            None
        };
        let len: usize = self.read(&mut *reader)?;
        let mut links: Vec<u64> = Vec::with_capacity(len);
        for _ in 0..len {
            let l: u64 = self.read(&mut *reader)?;
            links.push(l);
        }

        Ok(LinkState {
            psid,
            sn,
            zid,
            whatami,
            locators,
            links,
        })
    }
}

// LinkStateList
impl<W> WCodec<&LinkStateList, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &LinkStateList) -> Self::Output {
        // Header
        let header = zmsg::id::LINK_STATE_LIST;
        self.write(&mut *writer, header)?;

        // Body
        self.write(&mut *writer, x.link_states.len())?;
        for ls in x.link_states.iter() {
            self.write(&mut *writer, ls)?;
        }

        Ok(())
    }
}

impl<R> RCodec<LinkStateList, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<LinkStateList, Self::Error> {
        let header: u8 = self.read(&mut *reader)?;
        let codec = Zenoh080Header::new(header);

        codec.read(reader)
    }
}

impl<R> RCodec<LinkStateList, &mut R> for Zenoh080Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<LinkStateList, Self::Error> {
        if imsg::mid(self.header) != zmsg::id::LINK_STATE_LIST {
            return Err(DidntRead);
        }

        let len: usize = self.codec.read(&mut *reader)?;
        let mut link_states = Vec::with_capacity(len);
        for _ in 0..len {
            let ls: LinkState = self.codec.read(&mut *reader)?;
            link_states.push(ls);
        }

        Ok(LinkStateList { link_states })
    }
}
