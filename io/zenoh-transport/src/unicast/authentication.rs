//
// Copyright (c) 2024 ZettaScale Technology
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
use zenoh_link::LinkAuthId;
use zenoh_protocol::core::ZenohIdProto;

#[cfg(feature = "auth_usrpwd")]
use super::establishment::ext::auth::UsrPwdId;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransportAuthId {
    username: Option<String>,
    zid: ZenohIdProto,
    link_auth_ids: Vec<LinkAuthId>,
}

impl TransportAuthId {
    pub(crate) fn new(zid: ZenohIdProto) -> Self {
        Self {
            username: None,
            zid,
            link_auth_ids: vec![],
        }
    }

    #[cfg(feature = "auth_usrpwd")]
    pub(crate) fn set_username(&mut self, user_pwd_id: &UsrPwdId) {
        self.username = if let Some(username) = &user_pwd_id.0 {
            // Convert username from Vec<u8> to String
            match std::str::from_utf8(username) {
                Ok(name) => Some(name.to_owned()),
                Err(e) => {
                    tracing::error!("Error in extracting username {}", e);
                    None
                }
            }
        } else {
            None
        }
    }

    pub(crate) fn push_link_auth_id(&mut self, link_auth_id: LinkAuthId) {
        self.link_auth_ids.push(link_auth_id);
    }

    pub fn username(&self) -> Option<&String> {
        self.username.as_ref()
    }

    pub fn link_auth_ids(&self) -> &Vec<LinkAuthId> {
        &self.link_auth_ids
    }

    pub fn zid(&self) -> &ZenohIdProto {
        &self.zid
    }
}
