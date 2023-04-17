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
use std::{
    convert::TryFrom,
    ops::{Deref, DerefMut},
};
use zenoh_buffers::{reader::HasReader, writer::HasWriter, ZBuf};
use zenoh_codec::{RCodec, WCodec, Zenoh060};
use zenoh_protocol::{
    common::Attachment,
    core::{Property, ZInt},
};
use zenoh_result::{bail, zerror, Error as ZError, ZResult};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EstablishmentProperties(Vec<Property>);

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
    pub(super) fn new() -> Self {
        EstablishmentProperties(vec![])
    }

    pub(super) fn insert(&mut self, p: Property) -> ZResult<()> {
        if self.0.iter().any(|x| x.key == p.key) {
            bail!("Property {} already exists", p.key)
        }
        self.0.push(p);
        Ok(())
    }

    pub(super) fn remove(&mut self, key: ZInt) -> Option<Property> {
        self.0
            .iter()
            .position(|x| x.key == key)
            .map(|i| self.0.remove(i))
    }
}

impl TryFrom<&EstablishmentProperties> for Attachment {
    type Error = ZError;

    fn try_from(eps: &EstablishmentProperties) -> Result<Self, Self::Error> {
        if eps.is_empty() {
            bail!("Can not create an attachment with zero properties")
        }

        let mut zbuf = ZBuf::default();
        let mut writer = zbuf.writer();
        let codec = Zenoh060::default();

        codec
            .write(&mut writer, eps.0.as_slice())
            .map_err(|_| zerror!(""))?;

        let attachment = Attachment::new(zbuf);
        Ok(attachment)
    }
}

impl TryFrom<Vec<Property>> for EstablishmentProperties {
    type Error = ZError;

    fn try_from(mut ps: Vec<Property>) -> Result<Self, Self::Error> {
        let mut eps = EstablishmentProperties::new();
        for p in ps.drain(..) {
            eps.insert(p)?;
        }

        Ok(eps)
    }
}

impl TryFrom<&Attachment> for EstablishmentProperties {
    type Error = ZError;

    fn try_from(att: &Attachment) -> Result<Self, Self::Error> {
        let mut reader = att.buffer.reader();
        let codec = Zenoh060::default();

        let ps: Vec<Property> = codec.read(&mut reader).map_err(|_| zerror!(""))?;
        EstablishmentProperties::try_from(ps)
    }
}

impl EstablishmentProperties {
    #[cfg(test)]
    pub fn rand() -> Self {
        use rand::Rng;

        const MIN: usize = 1;
        const MAX: usize = 8;

        let mut rng = rand::thread_rng();

        let mut eps = EstablishmentProperties::new();
        for _ in MIN..=MAX {
            loop {
                let key: ZInt = rng.gen();
                let mut value = vec![0u8; rng.gen_range(MIN..=MAX)];
                rng.fill(&mut value[..]);
                let p = Property { key, value };
                if eps.insert(p).is_ok() {
                    break;
                }
            }
        }

        eps
    }
}
