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

use std::future::{IntoFuture, Ready};

use zenoh_config::wrappers::Hello;
use zenoh_core::{Resolvable, Wait};
use zenoh_protocol::core::WhatAmIMatcher;
use zenoh_result::ZResult;

use crate::api::{
    handlers::{locked, Callback, DefaultHandler, IntoHandler},
    scouting::{Scout, _scout},
};

/// A builder for initializing a [`Scout`], returned by the
/// [`zenoh::scout`](crate::scout) function.
///
/// # Examples
/// ```no_run
/// # #[tokio::main]
/// # async fn main() {
/// use zenoh::config::WhatAmI;
///
/// let receiver = zenoh::scout(WhatAmI::Peer | WhatAmI::Router, zenoh::Config::default())
///     .await
///     .unwrap();
/// while let Ok(hello) = receiver.recv_async().await {
///     println!("{}", hello);
/// }
/// # }
/// ```
#[must_use = "Resolvables do nothing unless you resolve them using `.await` or `zenoh::Wait::wait`"]
#[derive(Debug)]
pub struct ScoutBuilder<Handler> {
    pub(crate) what: WhatAmIMatcher,
    pub(crate) config: ZResult<crate::config::Config>,
    pub(crate) handler: Handler,
}

impl ScoutBuilder<DefaultHandler> {
    /// Receive the [`Hello`] messages from this scout with a callback.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh::config::WhatAmI;
    ///
    /// let scout = zenoh::scout(WhatAmI::Peer | WhatAmI::Router, zenoh::Config::default())
    ///     .callback(|hello| { println!("{}", hello); })
    ///     .await
    ///     .unwrap();
    /// # }
    /// ```
    #[inline]
    pub fn callback<F>(self, callback: F) -> ScoutBuilder<Callback<Hello>>
    where
        F: Fn(Hello) + Send + Sync + 'static,
    {
        self.with(Callback::from(callback))
    }

    /// Receive the [`Hello`] messages from this scout with a mutable callback.
    ///
    /// Using this guarantees that your callback will never be called concurrently.
    /// If your callback is also accepted by the [`callback`](ScoutBuilder::callback) method, we suggest you use it instead of `callback_mut`.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh::config::WhatAmI;
    ///
    /// let mut n = 0;
    /// let scout = zenoh::scout(WhatAmI::Peer | WhatAmI::Router, zenoh::Config::default())
    ///     .callback_mut(move |_hello| { n += 1; })
    ///     .await
    ///     .unwrap();
    /// # }
    /// ```
    #[inline]
    pub fn callback_mut<F>(self, callback: F) -> ScoutBuilder<Callback<Hello>>
    where
        F: FnMut(Hello) + Send + Sync + 'static,
    {
        self.callback(locked(callback))
    }

    /// Receive the [`Hello`] messages from this scout with a [`Handler`](crate::handlers::IntoHandler).
    ///
    /// # Examples
    /// ```no_run
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh::config::WhatAmI;
    ///
    /// let receiver = zenoh::scout(WhatAmI::Peer | WhatAmI::Router, zenoh::Config::default())
    ///     .with(flume::bounded(32))
    ///     .await
    ///     .unwrap();
    /// while let Ok(hello) = receiver.recv_async().await {
    ///     println!("{}", hello);
    /// }
    /// # }
    /// ```
    #[inline]
    pub fn with<Handler>(self, handler: Handler) -> ScoutBuilder<Handler>
    where
        Handler: IntoHandler<Hello>,
    {
        let ScoutBuilder {
            what,
            config,
            handler: _,
        } = self;
        ScoutBuilder {
            what,
            config,
            handler,
        }
    }
}

impl<Handler> Resolvable for ScoutBuilder<Handler>
where
    Handler: IntoHandler<Hello> + Send,
    Handler::Handler: Send,
{
    type To = ZResult<Scout<Handler::Handler>>;
}

impl<Handler> Wait for ScoutBuilder<Handler>
where
    Handler: IntoHandler<Hello> + Send,
    Handler::Handler: Send,
{
    fn wait(self) -> <Self as Resolvable>::To {
        let (callback, receiver) = self.handler.into_handler();
        _scout(self.what, self.config?, callback).map(|scout| Scout { scout, receiver })
    }
}

impl<Handler> IntoFuture for ScoutBuilder<Handler>
where
    Handler: IntoHandler<Hello> + Send,
    Handler::Handler: Send,
{
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}
