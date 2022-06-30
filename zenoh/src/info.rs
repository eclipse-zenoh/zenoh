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
use crate::utils::ClosureResolve;
use crate::SessionRef;
use zenoh_config::{WhatAmI, ZenohId};
use zenoh_core::{AsyncResolve, Resolvable, SyncResolve};

pub struct ZidBuilder<'a> {
    pub(crate) session: SessionRef<'a>,
}
impl<'a> Resolvable for ZidBuilder<'a> {
    type Output = ZenohId;
}
impl<'a> SyncResolve for ZidBuilder<'a> {
    #[inline]
    fn res_sync(self) -> Self::Output {
        self.session.runtime.pid
    }
}
impl<'a> AsyncResolve for ZidBuilder<'a> {
    type Future = futures::future::Ready<Self::Output>;
    fn res_async(self) -> Self::Future {
        futures::future::ready(self.res_sync())
    }
}

macro_rules! closure_builder {
    ($name:ident) => {
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

closure_builder!(RoutersZidBuilder);
closure_builder!(PeersZidBuilder);

pub struct SessionInfos<'a> {
    pub(crate) session: SessionRef<'a>,
}

impl SessionInfos<'_> {
    pub fn zid(&self) -> ZidBuilder {
        ZidBuilder {
            session: self.session.clone(),
        }
    }

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
                        .and_then(|what| (what == WhatAmI::Router).then(|| ()))
                        .and_then(|_| s.get_pid().ok())
                })
        })))
    }

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
                        .and_then(|what| (what == WhatAmI::Peer).then(|| ()))
                        .and_then(|_| s.get_pid().ok())
                })
        })))
    }
}
