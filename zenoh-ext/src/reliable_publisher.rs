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
use std::future::Ready;
use std::sync::atomic::{AtomicU64, Ordering};
use zenoh::buffers::WBuf;
use zenoh::prelude::r#async::*;
use zenoh::publication::{Publication, Publisher};
use zenoh_core::{AsyncResolve, Resolvable, Result as ZResult, SyncResolve};
use zenoh_protocol::io::WBufCodec;
use zenoh_util::core::ResolveFuture;

use crate::{ReliabilityCache, SessionExt};

/// The builder of ReliablePublisher, allowing to configure it.
pub struct ReliablePublisherBuilder<'a, 'b> {
    session: &'a Session,
    pub_key_expr: ZResult<KeyExpr<'b>>,
    with_cache: bool,
    history: usize,
    resources_limit: Option<usize>,
}

impl<'a, 'b> ReliablePublisherBuilder<'a, 'b> {
    pub(crate) fn new(
        session: &'a Session,
        pub_key_expr: ZResult<KeyExpr<'b>>,
    ) -> ReliablePublisherBuilder<'a, 'b> {
        ReliablePublisherBuilder {
            session,
            pub_key_expr,
            with_cache: true,
            history: 1024,
            resources_limit: None,
        }
    }

    /// Change the limit number of cached resources.
    pub fn with_cache(mut self, with_cache: bool) -> Self {
        self.with_cache = with_cache;
        self
    }

    /// Change the history size for each resource.
    pub fn history(mut self, history: usize) -> Self {
        self.history = history;
        self
    }

    /// Change the limit number of cached resources.
    pub fn resources_limit(mut self, limit: usize) -> Self {
        self.resources_limit = Some(limit);
        self
    }
}

impl<'a> Resolvable for ReliablePublisherBuilder<'a, '_> {
    type To = ZResult<ReliablePublisher<'a>>;
}

impl SyncResolve for ReliablePublisherBuilder<'_, '_> {
    fn res_sync(self) -> <Self as Resolvable>::To {
        ReliablePublisher::new(self)
    }
}

impl<'a> AsyncResolve for ReliablePublisherBuilder<'a, '_> {
    type Future = Ready<Self::To>;

    fn res_async(self) -> Self::Future {
        std::future::ready(self.res_sync())
    }
}

pub struct ReliablePublisher<'a> {
    _id: ZenohId,
    _seqnum: AtomicU64,
    _publisher: Publisher<'a>,
    _cache: Option<ReliabilityCache<'a>>,
}

impl<'a> ReliablePublisher<'a> {
    fn new(conf: ReliablePublisherBuilder<'a, '_>) -> ZResult<Self> {
        let key_expr = conf.pub_key_expr?;
        let id = conf.session.info().zid().res_sync();

        let publisher = conf
            .session
            .declare_publisher(key_expr.clone().into_owned())
            .res_sync()?;

        let cache = if conf.with_cache {
            let prefix = id.into_keyexpr();
            let mut builder = conf
                .session
                .declare_reliability_cache(key_expr.into_owned())
                .subscriber_allowed_origin(Locality::SessionLocal)
                .history(conf.history)
                .queryable_prefix(&prefix);
            if let Some(resources_limit) = conf.resources_limit {
                builder = builder.resources_limit(resources_limit);
            }
            Some(builder.res_sync()?)
        } else {
            None
        };

        Ok(ReliablePublisher {
            _id: id,
            _seqnum: AtomicU64::new(0),
            _publisher: publisher,
            _cache: cache,
        })
    }

    pub fn key_expr(&self) -> &KeyExpr<'a> {
        self._publisher.key_expr()
    }

    /// Change the `congestion_control` to apply when routing the data.
    #[inline]
    pub fn congestion_control(mut self, congestion_control: CongestionControl) -> Self {
        self._publisher = self._publisher.congestion_control(congestion_control);
        self
    }

    /// Change the priority of the written data.
    #[inline]
    pub fn priority(mut self, priority: Priority) -> Self {
        self._publisher = self._publisher.priority(priority);
        self
    }

    /// Send data with [`kind`](SampleKind) (Put or Delete).
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::r#async::*;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap().into_arc();
    /// let publisher = session.declare_publisher("key/expression").res().await.unwrap();
    /// publisher.write(SampleKind::Put, "value").res().await.unwrap();
    /// # })
    /// ```
    pub fn write<IntoValue>(&self, kind: SampleKind, value: IntoValue) -> Publication
    where
        IntoValue: Into<Value>,
    {
        let v: Value = value.into();
        let mut buf = WBuf::new(128, false);
        buf.write_zid(&self._id);
        buf.write_zint(self._seqnum.fetch_add(1, Ordering::Relaxed));
        buf.write_zbuf_slices(&v.payload);
        self._publisher
            .write(kind, Value::new(buf.into()).encoding(v.encoding))
    }

    /// Put data.
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::r#async::*;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap().into_arc();
    /// let publisher = session.declare_publisher("key/expression").res().await.unwrap();
    /// publisher.put("value").res().await.unwrap();
    /// # })
    /// ```
    #[inline]
    pub fn put<IntoValue>(&self, value: IntoValue) -> Publication
    where
        IntoValue: Into<Value>,
    {
        self.write(SampleKind::Put, value)
    }

    /// Delete data.
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::r#async::*;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap().into_arc();
    /// let publisher = session.declare_publisher("key/expression").res().await.unwrap();
    /// publisher.delete().res().await.unwrap();
    /// # })
    /// ```
    pub fn delete(&self) -> Publication {
        self.write(SampleKind::Delete, Value::empty())
    }

    /// Undeclares the [`Publisher`], informing the network that it needn't optimize publications for its key expression anymore.
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::r#async::*;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap();
    /// let publisher = session.declare_publisher("key/expression").res().await.unwrap();
    /// publisher.undeclare().res().await.unwrap();
    /// # })
    /// ```
    pub fn undeclare(self) -> impl Resolve<ZResult<()>> + 'a {
        ResolveFuture::new(async move {
            let ReliablePublisher {
                _id,
                _seqnum,
                _publisher,
                _cache,
            } = self;
            if let Some(cache) = _cache {
                cache.close().res_async().await?;
            }
            _publisher.undeclare().res_async().await?;
            Ok(())
        })
    }
}
