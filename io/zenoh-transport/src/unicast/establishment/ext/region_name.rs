//
// Copyright (c) 2026 ZettaScale Technology
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

use zenoh_buffers::{
    reader::Reader,
    writer::{DidntWrite, Writer},
    ZBuf,
};
use zenoh_codec::{RCodec, WCodec, Zenoh080};
use zenoh_protocol::transport::init::ext;
use zenoh_result::ZResult;

use crate::{
    unicast::establishment::{AcceptFsm, OpenFsm},
    RegionName,
};

pub(crate) struct RegionNameFsm {
    region_name: Option<RegionName>,
}

impl RegionNameFsm {
    pub(crate) fn new(region_name: Option<RegionName>) -> Self {
        Self { region_name }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct State {
    other_region_name: Option<RegionName>,
}

impl State {
    fn new() -> Self {
        Self {
            other_region_name: None,
        }
    }
}

impl State {
    #[cfg(test)]
    pub(crate) fn rand() -> Self {
        use rand::Rng;

        Self {
            other_region_name: rand::thread_rng().gen_bool(0.5).then(RegionName::rand),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct StateOpen(State);

impl StateOpen {
    pub(crate) fn new() -> Self {
        Self(State::new())
    }

    pub(crate) fn other_region_name(&self) -> Option<RegionName> {
        self.0.other_region_name.clone()
    }
}

#[async_trait::async_trait]
impl<'a> OpenFsm for &'a RegionNameFsm {
    type Error = zenoh_result::Error;

    type SendInitSynIn = &'a StateOpen;
    type SendInitSynOut = Option<ext::RegionName>;

    async fn send_init_syn(
        self,
        _state: Self::SendInitSynIn,
    ) -> Result<Self::SendInitSynOut, Self::Error> {
        Ok(self.region_name.clone().map(name_to_ext))
    }

    type RecvInitAckIn = (&'a mut StateOpen, Option<ext::RegionName>);
    type RecvInitAckOut = ();

    async fn recv_init_ack(
        self,
        (state, ext): Self::RecvInitAckIn,
    ) -> Result<Self::RecvInitAckOut, Self::Error> {
        state.0.other_region_name = ext.map(ext_to_name).transpose()?;
        Ok(())
    }

    type SendOpenSynIn = &'a StateOpen;
    type SendOpenSynOut = Option<RegionName>;

    async fn send_open_syn(
        self,
        state: Self::SendOpenSynIn,
    ) -> Result<Self::SendOpenSynOut, Self::Error> {
        Ok(state.0.other_region_name.clone())
    }

    type RecvOpenAckIn = ();
    type RecvOpenAckOut = ();

    async fn recv_open_ack(
        self,
        _input: Self::RecvOpenAckIn,
    ) -> Result<Self::RecvOpenAckOut, Self::Error> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct StateAccept(State);

impl StateAccept {
    pub(crate) fn new() -> Self {
        Self(State::new())
    }

    pub(crate) fn other_region_name(&self) -> Option<RegionName> {
        self.0.other_region_name.clone()
    }

    #[cfg(test)]
    pub(crate) fn rand() -> Self {
        Self(State::rand())
    }
}

impl<W> WCodec<&StateAccept, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &StateAccept) -> Self::Output {
        if let Some(region_name) = &x.0.other_region_name {
            self.write(writer, region_name.as_str())
        } else {
            self.write(writer, "")
        }
    }
}

impl<R> RCodec<StateAccept, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = zenoh_result::Error;

    fn read(self, reader: &mut R) -> Result<StateAccept, Self::Error> {
        let s: String = self
            .read(&mut *reader)
            .map_err(|_| "failed to read region name string")?;

        let other_region_name = if s.is_empty() {
            // NOTE: region names are non-empty
            None
        } else {
            Some(RegionName::try_from(s)?)
        };

        Ok(StateAccept(State { other_region_name }))
    }
}

#[async_trait::async_trait]
impl<'a> AcceptFsm for &'a RegionNameFsm {
    type Error = zenoh_result::Error;

    type RecvInitSynIn = (&'a mut StateAccept, Option<ext::RegionName>);
    type RecvInitSynOut = ();

    async fn recv_init_syn(
        self,
        (state, ext): Self::RecvInitSynIn,
    ) -> Result<Self::RecvInitSynOut, Self::Error> {
        state.0.other_region_name = ext.map(ext_to_name).transpose()?;
        Ok(())
    }

    type SendInitAckIn = &'a StateAccept;
    type SendInitAckOut = Option<ext::RegionName>;

    async fn send_init_ack(
        self,
        _state: Self::SendInitAckIn,
    ) -> Result<Self::SendInitAckOut, Self::Error> {
        Ok(self.region_name.clone().map(name_to_ext))
    }

    type RecvOpenSynIn = ();
    type RecvOpenSynOut = ();
    async fn recv_open_syn(
        self,
        _input: Self::RecvOpenSynIn,
    ) -> Result<Self::RecvOpenSynOut, Self::Error> {
        Ok(())
    }

    type SendOpenAckIn = &'a StateAccept;
    type SendOpenAckOut = Option<RegionName>;
    async fn send_open_ack(
        self,
        state: Self::SendOpenAckIn,
    ) -> Result<Self::SendOpenAckOut, Self::Error> {
        Ok(state.0.other_region_name.clone())
    }
}

fn name_to_ext(name: RegionName) -> ext::RegionName {
    ext::RegionName::new(ZBuf::from(name.into_string().into_bytes()))
}

fn ext_to_name(ext: ext::RegionName) -> ZResult<RegionName> {
    let s = String::from_utf8(ext.value.to_zslice().to_vec())?;
    let name = RegionName::try_from(s)?;
    Ok(name)
}
