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

//! Liveliness primitives.

#[zenoh_core::unstable]
use {
    crate::prelude::*, crate::SessionRef, crate::Undeclarable, std::future::Ready, std::sync::Arc,
    zenoh_core::AsyncResolve, zenoh_core::Resolvable, zenoh_core::Result as ZResult,
    zenoh_core::SyncResolve,
};

#[zenoh_core::unstable]
pub(crate) static PREFIX_LIVELINESS: &str = crate::net::routing::PREFIX_LIVELINESS;

#[zenoh_core::unstable]
lazy_static::lazy_static!(
    pub(crate) static ref KE_PREFIX_LIVELINESS: &'static keyexpr = unsafe { keyexpr::from_str_unchecked(PREFIX_LIVELINESS) };
);

/// A builder for initializing a [`LivelinessToken`](LivelinessToken).
///
/// # Examples
/// ```
/// # async_std::task::block_on(async {
/// use zenoh::prelude::r#async::*;
///
/// let session = zenoh::open(config::peer()).res().await.unwrap();
/// let liveliness = session
///     .declare_liveliness("key/expression")
///     .res()
///     .await
///     .unwrap();
/// # })
/// ```
#[zenoh_core::unstable]
#[derive(Debug)]
pub struct LivelinessTokenBuilder<'a, 'b> {
    pub(crate) session: SessionRef<'a>,
    pub(crate) key_expr: ZResult<KeyExpr<'b>>,
}

#[zenoh_core::unstable]
impl<'a> Resolvable for LivelinessTokenBuilder<'a, '_> {
    type To = ZResult<LivelinessToken<'a>>;
}

#[zenoh_core::unstable]
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

#[zenoh_core::unstable]
impl AsyncResolve for LivelinessTokenBuilder<'_, '_> {
    type Future = Ready<Self::To>;

    fn res_async(self) -> Self::Future {
        std::future::ready(self.res_sync())
    }
}

#[zenoh_core::unstable]
#[derive(Debug)]
pub(crate) struct LivelinessTokenState {
    pub(crate) id: Id,
    pub(crate) key_expr: KeyExpr<'static>,
}

/// A token which liveliness is tied to the Zenoh [`Session`](Session)
/// and can be monitored by remote applications.
///
/// A LivelinessToken with key `key/expression` can be queried or subscribed
/// to on key `@/liveliness/key/expression`.
///
/// A declared liveliness token will be seen as alive by any other Zenoh
/// application in the system that monitors it while the liveliness token
/// is not undeclared or dropped, while the Zenoh application that declared
/// it is alive (didn't stop or crashed) and while the Zenoh application
/// that declared the token has Zenoh connectivity with the Zenoh application
/// that monitors it.
///
/// LivelinessTokens are automatically undeclared when dropped.
///
/// # Examples
/// ```no_run
/// # async_std::task::block_on(async {
/// use zenoh::prelude::r#async::*;
///
/// let session = zenoh::open(config::peer()).res().await.unwrap();
/// let liveliness = session.declare_liveliness("key/expression")
///     .res()
///     .await
///     .unwrap();
/// # })
/// ```
#[zenoh_core::unstable]
#[derive(Debug)]
pub struct LivelinessToken<'a> {
    pub(crate) session: SessionRef<'a>,
    pub(crate) state: Arc<LivelinessTokenState>,
    pub(crate) alive: bool,
}

/// A [`Resolvable`] returned when undeclaring a [`LivelinessToken`](LivelinessToken).
///
/// # Examples
/// ```
/// # async_std::task::block_on(async {
/// use zenoh::prelude::r#async::*;
///
/// let session = zenoh::open(config::peer()).res().await.unwrap();
/// let liveliness = session.declare_liveliness("key/expression")
///     .res()
///     .await
///     .unwrap();
///
/// liveliness.undeclare().res().await.unwrap();
/// # })
/// ```
#[zenoh_core::unstable]
pub struct LivelinessTokenUndeclaration<'a> {
    token: LivelinessToken<'a>,
}

#[zenoh_core::unstable]
impl Resolvable for LivelinessTokenUndeclaration<'_> {
    type To = ZResult<()>;
}

#[zenoh_core::unstable]
impl SyncResolve for LivelinessTokenUndeclaration<'_> {
    fn res_sync(mut self) -> <Self as Resolvable>::To {
        self.token.alive = false;
        self.token.session.undeclare_liveliness(self.token.state.id)
    }
}

#[zenoh_core::unstable]
impl<'a> AsyncResolve for LivelinessTokenUndeclaration<'a> {
    type Future = Ready<Self::To>;

    fn res_async(self) -> Self::Future {
        std::future::ready(self.res_sync())
    }
}

#[zenoh_core::unstable]
impl<'a> LivelinessToken<'a> {
    /// Undeclare a [`LivelinessToken`](LivelinessToken).
    ///
    /// LivelinessTokens are automatically closed when dropped,
    /// but you may want to use this function to handle errors or
    /// undeclare the LivelinessToken asynchronously.
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::r#async::*;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap();
    /// let liveliness = session.declare_liveliness("key/expression")
    ///     .res()
    ///     .await
    ///     .unwrap();
    ///
    /// liveliness.undeclare().res().await.unwrap();
    /// # })
    /// ```
    #[inline]
    pub fn undeclare(self) -> impl Resolve<ZResult<()>> + 'a {
        Undeclarable::undeclare_inner(self, ())
    }
}

#[zenoh_core::unstable]
impl<'a> Undeclarable<(), LivelinessTokenUndeclaration<'a>> for LivelinessToken<'a> {
    fn undeclare_inner(self, _: ()) -> LivelinessTokenUndeclaration<'a> {
        LivelinessTokenUndeclaration { token: self }
    }
}

#[zenoh_core::unstable]
impl Drop for LivelinessToken<'_> {
    fn drop(&mut self) {
        if self.alive {
            let _ = self.session.undeclare_liveliness(self.state.id);
        }
    }
}
