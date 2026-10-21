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

use std::{
    future::{ready, IntoFuture},
    pin::Pin,
    time::Duration,
};

use futures::Future;
use zenoh_config::{wrappers::ZenohId, EndPoint};
use zenoh_core::{Resolvable, Wait};
use zenoh_result::ZResult;
use zenoh_runtime::ZRuntime;

use crate::api::session::{Session, SessionClosedError};

/// A builder returned by [`Session::connect_peer`](crate::Session::connect_peer).
///
/// Resolving the builder dials the endpoint once. It resolves to the [`ZenohId`] of the
/// peer the session is now connected to.
///
/// # Examples
/// ```no_run
/// # #[tokio::main]
/// # async fn main() {
/// use std::time::Duration;
///
/// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
/// let endpoint: zenoh::config::EndPoint = "tcp/192.168.1.10:7447".parse().unwrap();
/// let peer = session
///     .connect_peer(endpoint)
///     .timeout(Duration::from_millis(1500))
///     .await
///     .unwrap();
/// println!("connected to {peer}");
/// # }
/// ```
#[must_use = "Resolvables do nothing unless you resolve them using `.await` or `zenoh::Wait::wait`"]
#[derive(Debug)]
pub struct ConnectPeerBuilder<'a> {
    session: &'a Session,
    endpoint: EndPoint,
    expected_zid: Option<ZenohId>,
    timeout: Option<Duration>,
}

impl<'a> ConnectPeerBuilder<'a> {
    pub(crate) fn new(session: &'a Session, endpoint: EndPoint) -> Self {
        Self {
            session,
            endpoint,
            expected_zid: None,
            timeout: None,
        }
    }

    /// Require the remote peer to have the given [`ZenohId`].
    ///
    /// The handshake fails, and the link is closed, if the peer at the endpoint reports a
    /// different id. If the session already has a unicast transport to this peer, the call
    /// resolves to `Ok` without dialing.
    pub fn expected_zid(mut self, zid: ZenohId) -> Self {
        self.expected_zid = Some(zid);
        self
    }

    /// Bound this attempt (link setup plus the Init/Open handshake) by `timeout`.
    ///
    /// It replaces `transport/unicast/open_timeout` for this call only. By default the
    /// configured `transport/unicast/open_timeout` applies.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }
}

impl Resolvable for ConnectPeerBuilder<'_> {
    type To = ZResult<ZenohId>;
}

impl Wait for ConnectPeerBuilder<'_> {
    fn wait(self) -> Self::To {
        ZRuntime::Net.block_in_place(self.into_future())
    }
}

impl IntoFuture for ConnectPeerBuilder<'_> {
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Pin<Box<dyn Future<Output = Self::Output> + Send>>;

    fn into_future(self) -> Self::IntoFuture {
        if self.session.is_closed() {
            return Box::pin(ready(Err(SessionClosedError.into())));
        }
        let fut = self.session.runtime().connect_peer_endpoint(
            self.endpoint,
            self.expected_zid.map(Into::into),
            self.timeout,
        );
        Box::pin(async move { fut.await.map(Into::into) })
    }
}
