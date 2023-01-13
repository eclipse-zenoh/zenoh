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

use crate::prelude::*;
use crate::SessionRef;
use crate::Undeclarable;
use std::future::Ready;
use std::sync::Arc;
use zenoh_core::AsyncResolve;
use zenoh_core::Resolvable;
use zenoh_core::Result as ZResult;
use zenoh_core::SyncResolve;

pub(crate) static PREFIX_LIVELINESS: &str = crate::net::routing::PREFIX_LIVELINESS;

lazy_static::lazy_static!(
    pub(crate) static ref KE_PREFIX_LIVELINESS: &'static keyexpr = unsafe { keyexpr::from_str_unchecked(PREFIX_LIVELINESS) };
);

#[derive(Debug)]
pub struct LivelinessTokenBuilder<'a, 'b> {
    pub(crate) session: SessionRef<'a>,
    pub(crate) key_expr: ZResult<KeyExpr<'b>>,
}

impl<'a> Resolvable for LivelinessTokenBuilder<'a, '_> {
    type To = ZResult<LivelinessToken<'a>>;
}

impl SyncResolve for LivelinessTokenBuilder<'_, '_> {
    #[inline]
    fn res_sync(self) -> <Self as Resolvable>::To {
        let session = self.session;
        let key_expr = self.key_expr?.into_owned();
        session
            .declare_liveliness_inner(&key_expr)
            .map(|tok_state| LivelinessToken {
                session,
                state: tok_state,
                alive: true,
            })
    }
}

impl AsyncResolve for LivelinessTokenBuilder<'_, '_> {
    type Future = Ready<Self::To>;

    fn res_async(self) -> Self::Future {
        std::future::ready(self.res_sync())
    }
}

#[derive(Debug)]
pub(crate) struct LivelinessTokenState {
    pub(crate) id: Id,
    pub(crate) key_expr: KeyExpr<'static>,
}

#[derive(Debug)]
pub struct LivelinessToken<'a> {
    pub(crate) session: SessionRef<'a>,
    pub(crate) state: Arc<LivelinessTokenState>,
    pub(crate) alive: bool,
}
pub struct LivelinessTokenUndeclaration<'a> {
    token: LivelinessToken<'a>,
}

impl Resolvable for LivelinessTokenUndeclaration<'_> {
    type To = ZResult<()>;
}

impl SyncResolve for LivelinessTokenUndeclaration<'_> {
    fn res_sync(mut self) -> <Self as Resolvable>::To {
        self.token.alive = false;
        self.token.session.undeclare_liveliness(self.token.state.id)
    }
}

impl<'a> AsyncResolve for LivelinessTokenUndeclaration<'a> {
    type Future = Ready<Self::To>;

    fn res_async(self) -> Self::Future {
        std::future::ready(self.res_sync())
    }
}

impl<'a> LivelinessToken<'a> {
    #[inline]
    pub fn undeclare(self) -> impl Resolve<ZResult<()>> + 'a {
        Undeclarable::undeclare_inner(self, ())
    }
}

impl<'a> Undeclarable<(), LivelinessTokenUndeclaration<'a>> for LivelinessToken<'a> {
    fn undeclare_inner(self, _: ()) -> LivelinessTokenUndeclaration<'a> {
        LivelinessTokenUndeclaration { token: self }
    }
}

impl Drop for LivelinessToken<'_> {
    fn drop(&mut self) {
        if self.alive {
            let _ = self.session.undeclare_liveliness(self.state.id);
        }
    }
}
