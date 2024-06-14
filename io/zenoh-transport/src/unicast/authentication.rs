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
use zenoh_link::{LinkAuthId, LinkAuthType};

#[cfg(feature = "auth_usrpwd")]
use super::establishment::ext::auth::UsrPwdId;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuthId {
    CertCommonName(String),
    Username(String),
    None,
}

impl From<LinkAuthId> for AuthId {
    fn from(lid: LinkAuthId) -> Self {
        match (lid.get_type(), lid.get_value()) {
            (LinkAuthType::Tls | LinkAuthType::Quic, Some(auth_value)) => {
                AuthId::CertCommonName(auth_value.clone())
            }
            _ => AuthId::None,
        }
    }
}

#[cfg(feature = "auth_usrpwd")]
impl From<UsrPwdId> for AuthId {
    fn from(user_password_id: UsrPwdId) -> Self {
        match user_password_id.0 {
            Some(username) => {
                // Convert username from Vec<u8> to String
                match std::str::from_utf8(&username) {
                    Ok(name) => AuthId::Username(name.to_owned()),
                    Err(e) => {
                        tracing::error!("Error in extracting username {}", e);
                        AuthId::None
                    }
                }
            }
            None => AuthId::None,
        }
    }
}
