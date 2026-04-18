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
use zenoh_result::{zerror, ZResult};

#[cfg(feature = "auth_usrpwd")]
use super::establishment::ext::auth::UsrPwdId;

#[cfg(feature = "auth_usrpwd")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TransportUsrPwdPrincipal {
    Unknown,
    Known(Vec<u8>),
}

#[cfg(feature = "auth_usrpwd")]
impl TransportUsrPwdPrincipal {
    pub(crate) fn from_auth_id(auth_id: UsrPwdId) -> Self {
        if let Some(username) = auth_id.0 {
            Self::Known(username)
        } else {
            Self::Unknown
        }
    }
}

#[cfg(feature = "auth_usrpwd")]
pub(crate) fn plan_usrpwd_principal_update(
    existing: &TransportUsrPwdPrincipal,
    incoming: &TransportUsrPwdPrincipal,
) -> ZResult<bool> {
    match (existing, incoming) {
        (TransportUsrPwdPrincipal::Unknown, TransportUsrPwdPrincipal::Unknown)
        | (TransportUsrPwdPrincipal::Known(_), TransportUsrPwdPrincipal::Unknown) => Ok(false),
        (TransportUsrPwdPrincipal::Unknown, TransportUsrPwdPrincipal::Known(_)) => Ok(true),
        (TransportUsrPwdPrincipal::Known(a), TransportUsrPwdPrincipal::Known(b)) if a == b => {
            Ok(false)
        }
        _ => Err(zerror!(
            "Invalid authenticated principal: {:?}. Expected: {:?}.",
            incoming,
            existing
        )
        .into()),
    }
}

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
    pub(crate) fn set_username(&mut self, principal: &TransportUsrPwdPrincipal) {
        self.username = if let TransportUsrPwdPrincipal::Known(username) = principal {
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
