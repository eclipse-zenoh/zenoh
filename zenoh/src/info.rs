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

//! Tools to access information about the current zenoh [`Session`](crate::Session).

use crate::utils::ClosureResolve;
use crate::SessionRef;
use zenoh_config::{WhatAmI, ZenohId};
use zenoh_core::{AsyncResolve, Resolvable, SyncResolve};

/// A builder retuned by [`SessionInfo::zid()`](SessionInfo::zid) that allows
/// to access the [`ZenohId`] of the current zenoh [`Session`](crate::Session).
///
/// # Examples
/// ```
/// # async_std::task::block_on(async {
/// use zenoh::prelude::*;
/// use r#async::AsyncResolve;
///
/// let session = zenoh::open(config::peer()).res().await.unwrap();
/// let zid = session.info().zid().res().await;
/// # })
/// ```
pub struct ZidBuilder<'a> {
    pub(crate) session: SessionRef<'a>,
}
impl<'a> Resolvable for ZidBuilder<'a> {
    type Output = ZenohId;
}
impl<'a> SyncResolve for ZidBuilder<'a> {
    #[inline]
    fn res_sync(self) -> Self::Output {
        self.session.runtime.zid
    }
}
impl<'a> AsyncResolve for ZidBuilder<'a> {
    type Future = futures::future::Ready<Self::Output>;
    fn res_async(self) -> Self::Future {
        futures::future::ready(self.res_sync())
    }
}

macro_rules! closure_builder {
    ($(#[$attr:meta])* $name:ident) => {
        $(#[$attr])*
        pub struct $name<'a, T>(ClosureResolve<Box<dyn FnOnce() -> T + 'a>>);
        impl<'a, T> Resolvable for $name<'a, T> {
            type Output = T;
        }
        impl<'a, T> SyncResolve for $name<'a, T>
        where
            T: Send,
        {
            #[inline]
            fn res_sync(self) -> Self::Output {
                self.0.res_sync()
            }
        }
        impl<'a, T> AsyncResolve for $name<'a, T>
        where
            T: Send,
        {
            type Future = futures::future::Ready<Self::Output>;
            fn res_async(self) -> Self::Future {
                self.0.res_async()
            }
        }
    };
}

closure_builder! {
    /// A builder retuned by [`SessionInfo::routers_zid()`](SessionInfo::routers_zid) that allows
    /// to access the [`ZenohId`] of the zenoh routers this process is currently connected to
    /// or the [`ZenohId`] of the current router if this code is run from a router (plugin).
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::*;
    /// use r#async::AsyncResolve;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap();
    /// let mut routers_zid = session.info().routers_zid().res().await;
    /// while let Some(router_zid) = routers_zid.next() {}
    /// # })
    /// ```
    RoutersZidBuilder
}

closure_builder! {
    /// A builder retuned by [`SessionInfo::peers_zid()`](SessionInfo::peers_zid) that allows
    /// to access the [`ZenohId`] of the zenoh peers this process is currently connected to.
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::*;
    /// use r#async::AsyncResolve;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap();
    /// let zid = session.info().zid().res().await;
    /// let mut peers_zid = session.info().peers_zid().res().await;
    /// while let Some(peer_zid) = peers_zid.next() {}
    /// # })
    /// ```
    PeersZidBuilder
}

/// Struct returned by [`Session::info()`](crate::Session::info) which allows
/// to access informations about the current zenoh [`Session`](crate::Session).
///
/// # Examples
/// ```
/// # async_std::task::block_on(async {
/// use zenoh::prelude::*;
/// use r#async::AsyncResolve;
///
/// let session = zenoh::open(config::peer()).res().await.unwrap();
/// let info = session.info();
/// let zid = info.zid().res().await;
/// # })
/// ```
pub struct SessionInfo<'a> {
    pub(crate) session: SessionRef<'a>,
}

impl SessionInfo<'_> {
    /// Return the [`ZenohId`] of the current zenoh [`Session`](crate::Session).
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::*;
    /// use r#async::AsyncResolve;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap();
    /// let zid = session.info().zid().res().await;
    /// # })
    /// ```
    pub fn zid(&self) -> ZidBuilder {
        ZidBuilder {
            session: self.session.clone(),
        }
    }

    /// Return the [`ZenohId`] of the zenoh routers this process is currently connected to
    /// or the [`ZenohId`] of the current router if this code is run from a router (plugin).
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::*;
    /// use r#async::AsyncResolve;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap();
    /// let mut routers_zid = session.info().routers_zid().res().await;
    /// while let Some(router_zid) = routers_zid.next() {}
    /// # })
    /// ```
    pub fn routers_zid(&self) -> RoutersZidBuilder<impl Iterator<Item = ZenohId>> {
        RoutersZidBuilder(ClosureResolve(Box::new(move || {
            self.session
                .runtime
                .manager()
                .get_transports()
                .into_iter()
                .filter_map(|s| {
                    s.get_whatami()
                        .ok()
                        .and_then(|what| (what == WhatAmI::Router).then_some(()))
                        .and_then(|_| s.get_zid().ok())
                })
        })))
    }

    /// Return the [`ZenohId`] of the zenoh peers this process is currently connected to.
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::*;
    /// use r#async::AsyncResolve;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap();
    /// let mut peers_zid = session.info().peers_zid().res().await;
    /// while let Some(peer_zid) = peers_zid.next() {}
    /// # })
    /// ```
    pub fn peers_zid(&self) -> PeersZidBuilder<impl Iterator<Item = ZenohId>> {
        PeersZidBuilder(ClosureResolve(Box::new(move || {
            self.session
                .runtime
                .manager()
                .get_transports()
                .into_iter()
                .filter_map(|s| {
                    s.get_whatami()
                        .ok()
                        .and_then(|what| (what == WhatAmI::Peer).then_some(()))
                        .and_then(|_| s.get_zid().ok())
                })
        })))
    }
}
