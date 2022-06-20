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

//! Publishing primitives.

use crate::net::transport::Primitives;
use crate::prelude::*;
use crate::subscriber::Reliability;
use crate::Encoding;
use crate::SessionRef;
use crate::Undeclarable;
use zenoh_core::zresult::ZResult;
use zenoh_core::Resolve;
use zenoh_core::{zread, AsyncResolve, Resolvable, SyncResolve};
use zenoh_protocol::proto::{DataInfo, Options};
use zenoh_protocol_core::Channel;

/// The kind of congestion control.
pub use zenoh_protocol_core::CongestionControl;

/// A builder for initializing a [`delete`](crate::Session::delete) operation.
///
/// # Examples
/// ```
/// # async_std::task::block_on(async {
/// use zenoh::prelude::*;
/// use r#async::AsyncResolve;
/// use zenoh::publication::CongestionControl;
///
/// let session = zenoh::open(config::peer()).res().await.unwrap();
/// session
///     .delete("key/expression")
///     .res()
///     .await
///     .unwrap();
/// # })
/// ```
pub type DeleteBuilder<'a, 'b> = PutBuilder<'a, 'b>;

/// A builder for initializing a [`put`](crate::Session::put) operation.
///
/// # Examples
/// ```
/// # async_std::task::block_on(async {
/// use zenoh::prelude::*;
/// use r#async::AsyncResolve;
/// use zenoh::publication::CongestionControl;
///
/// let session = zenoh::open(config::peer()).res().await.unwrap();
/// session
///     .put("key/expression", "value")
///     .encoding(KnownEncoding::TextPlain)
///     .congestion_control(CongestionControl::Block)
///     .res()
///     .await
///     .unwrap();
/// # })
/// ```
#[derive(Debug, Clone)]
pub struct PutBuilder<'a, 'b> {
    pub(crate) publisher: PublisherBuilder<'a, 'b>,
    pub(crate) value: Value,
    pub(crate) kind: SampleKind,
}

impl PutBuilder<'_, '_> {
    /// Change the encoding of the written data.
    #[inline]
    pub fn encoding<IntoEncoding>(mut self, encoding: IntoEncoding) -> Self
    where
        IntoEncoding: Into<Encoding>,
    {
        self.value.encoding = encoding.into();
        self
    }
    /// Change the `congestion_control` to apply when routing the data.
    #[inline]
    pub fn congestion_control(mut self, congestion_control: CongestionControl) -> Self {
        self.publisher = self.publisher.congestion_control(congestion_control);
        self
    }

    /// Change the priority of the written data.
    #[inline]
    pub fn priority(mut self, priority: Priority) -> Self {
        self.publisher = self.publisher.priority(priority);
        self
    }

    /// Enable or disable local routing.
    #[inline]
    pub fn local_routing(mut self, local_routing: bool) -> Self {
        self.publisher = self.publisher.local_routing(local_routing);
        self
    }
    pub fn kind(mut self, kind: SampleKind) -> Self {
        self.kind = kind;
        self
    }
}

impl Resolvable for PutBuilder<'_, '_> {
    type Output = zenoh_core::Result<()>;
}
impl SyncResolve for PutBuilder<'_, '_> {
    #[inline]
    fn res_sync(self) -> Self::Output {
        let PutBuilder {
            publisher,
            value,
            kind,
        } = self;
        let key_expr = publisher.key_expr?;
        log::trace!("write({:?}, [...])", &key_expr);
        let state = zread!(publisher.session.state);
        let primitives = state.primitives.as_ref().unwrap().clone();
        drop(state);

        let mut info = DataInfo::new();
        info.kind = kind;
        info.encoding = if value.encoding != Encoding::default() {
            Some(value.encoding)
        } else {
            None
        };
        info.timestamp = publisher.session.runtime.new_timestamp();
        let data_info = if info.has_options() { Some(info) } else { None };

        primitives.send_data(
            &key_expr.to_wire(&publisher.session),
            value.payload.clone(),
            Channel {
                priority: publisher.priority.into(),
                reliability: Reliability::Reliable, // @TODO: need to check subscriptions to determine the right reliability value
            },
            publisher.congestion_control,
            data_info.clone(),
            None,
        );
        publisher.session.handle_data(
            true,
            &key_expr.to_wire(&publisher.session),
            data_info,
            value.payload,
            publisher.local_routing,
        );
        Ok(())
    }
}
impl AsyncResolve for PutBuilder<'_, '_> {
    type Future = futures::future::Ready<Self::Output>;
    fn res_async(self) -> Self::Future {
        futures::future::ready(self.res_sync())
    }
}

use futures::Sink;
use std::convert::TryInto;
use std::pin::Pin;
use std::task::{Context, Poll};
use zenoh_core::zresult::Error;

/// A publisher that allows to send data through a stream.
///
/// Publishers are automatically undeclared when dropped.
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
///
///
/// `Publisher` implements the `Sink` trait which is useful to forward
/// streams to zenoh.
/// ```no_run
/// # async_std::task::block_on(async {
/// use zenoh::prelude::*;
/// use r#async::AsyncResolve;
///
/// let session = zenoh::open(config::peer()).res().await.unwrap().into_arc();
/// let mut subscriber = session.declare_subscriber("key/expression").res().await.unwrap();
/// let publisher = session.declare_publisher("another/key/expression").res().await.unwrap();
/// subscriber.forward(publisher).await.unwrap();
/// # })
/// ```
#[derive(Debug, Clone)]
pub struct Publisher<'a> {
    pub(crate) session: SessionRef<'a>,
    pub(crate) key_expr: KeyExpr<'a>,
    pub(crate) congestion_control: CongestionControl,
    pub(crate) priority: Priority,
    pub(crate) local_routing: Option<bool>,
}

impl<'a> Publisher<'a> {
    pub fn key_expr(&self) -> &KeyExpr<'a> {
        &self.key_expr
    }
    /// Change the `congestion_control` to apply when routing the data.
    #[inline]
    pub fn congestion_control(mut self, congestion_control: CongestionControl) -> Self {
        self.congestion_control = congestion_control;
        self
    }

    /// Change the priority of the written data.
    #[inline]
    pub fn priority(mut self, priority: Priority) -> Self {
        self.priority = priority;
        self
    }

    /// Enable or disable local routing.
    #[inline]
    pub fn local_routing(mut self, local_routing: bool) -> Self {
        self.local_routing = Some(local_routing);
        self
    }

    pub fn write(&self, kind: SampleKind, value: Value) -> Publication {
        Publication {
            publisher: self,
            value,
            kind,
        }
    }
    /// Send a value.
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
        self.write(SampleKind::Put, value.into())
    }
    pub fn delete(&self) -> Publication {
        self.write(SampleKind::Delete, Value::empty())
    }
    /// Undeclares the [`Publisher`], informing the network that it needn't optimize publications for its key expression anymore.
    pub fn undeclare(self) -> impl Resolve<ZResult<()>> + 'a {
        Undeclarable::undeclare(self, ())
    }
}
impl<'a> Undeclarable<()> for Publisher<'a> {
    type Output = ZResult<()>;
    type Undeclaration = PublisherUndeclare<'a>;
    fn undeclare(self, _: ()) -> Self::Undeclaration {
        PublisherUndeclare { publisher: self }
    }
}
pub struct PublisherUndeclare<'a> {
    publisher: Publisher<'a>,
}
impl Resolvable for PublisherUndeclare<'_> {
    type Output = ZResult<()>;
}
impl SyncResolve for PublisherUndeclare<'_> {
    fn res_sync(mut self) -> Self::Output {
        let Publisher {
            session, key_expr, ..
        } = &self.publisher;
        session.undeclare_publication_intent(key_expr).res_sync()?;
        self.publisher.key_expr = unsafe { keyexpr::from_str_unchecked("") }.into();
        Ok(())
    }
}
impl AsyncResolve for PublisherUndeclare<'_> {
    type Future = futures::future::Ready<Self::Output>;
    fn res_async(self) -> Self::Future {
        futures::future::ready(self.res_sync())
    }
}
impl Drop for Publisher<'_> {
    fn drop(&mut self) {
        if !self.key_expr.is_empty() {
            let _ = self
                .session
                .undeclare_publication_intent(&self.key_expr)
                .res_sync();
        }
    }
}

pub struct Publication<'a> {
    publisher: &'a Publisher<'a>,
    value: Value,
    kind: SampleKind,
}
impl Resolvable for Publication<'_> {
    type Output = ZResult<()>;
}
impl AsyncResolve for Publication<'_> {
    type Future = futures::future::Ready<Self::Output>;
    fn res_async(self) -> Self::Future {
        futures::future::ready(self.res_sync())
    }
}
impl SyncResolve for Publication<'_> {
    fn res_sync(self) -> Self::Output {
        let Publication {
            publisher,
            value,
            kind,
        } = self;
        log::trace!("write({:?}, [...])", publisher.key_expr);
        let state = zread!(publisher.session.state);
        let primitives = state.primitives.as_ref().unwrap().clone();
        drop(state);

        let mut info = DataInfo::new();
        info.kind = kind;
        info.encoding = if value.encoding != Encoding::default() {
            Some(value.encoding)
        } else {
            None
        };
        info.timestamp = publisher.session.runtime.new_timestamp();
        let data_info = if info.has_options() { Some(info) } else { None };

        primitives.send_data(
            &publisher.key_expr.to_wire(&publisher.session),
            value.payload.clone(),
            Channel {
                priority: publisher.priority.into(),
                reliability: Reliability::Reliable, // @TODO: need to check subscriptions to determine the right reliability value
            },
            publisher.congestion_control,
            data_info.clone(),
            None,
        );
        publisher.session.handle_data(
            true,
            &publisher.key_expr.to_wire(&publisher.session),
            data_info,
            value.payload,
            publisher.local_routing,
        );
        Ok(())
    }
}

impl<'a, IntoValue> Sink<IntoValue> for Publisher<'a>
where
    IntoValue: Into<Value>,
{
    type Error = Error;

    #[inline]
    fn poll_ready(self: Pin<&mut Self>, _cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    #[inline]
    fn start_send(self: Pin<&mut Self>, item: IntoValue) -> Result<(), Self::Error> {
        self.put(item.into()).res_sync()
    }

    #[inline]
    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    #[inline]
    fn poll_close(self: Pin<&mut Self>, _cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }
}

/// A builder for initializing a [`Publisher`](Publisher).
///
/// # Examples
/// ```
/// # async_std::task::block_on(async {
/// use zenoh::prelude::*;
/// use r#async::AsyncResolve;
/// use zenoh::publication::CongestionControl;
///
/// let session = zenoh::open(config::peer()).res().await.unwrap();
/// let publisher = session
///     .declare_publisher("key/expression")
///     .congestion_control(CongestionControl::Block)
///     .res()
///     .await
///     .unwrap();
/// # })
/// ```
#[derive(Debug)]
pub struct PublisherBuilder<'a, 'b> {
    pub(crate) session: SessionRef<'a>,
    pub(crate) key_expr: ZResult<KeyExpr<'b>>,
    pub(crate) congestion_control: CongestionControl,
    pub(crate) priority: Priority,
    pub(crate) local_routing: Option<bool>,
}
impl<'a, 'b> Clone for PublisherBuilder<'a, 'b> {
    fn clone(&self) -> Self {
        Self {
            session: self.session.clone(),
            key_expr: match &self.key_expr {
                Ok(k) => Ok(k.clone()),
                Err(e) => Err(zerror!("Cloned KE Error: {}", e).into()),
            },
            congestion_control: self.congestion_control,
            priority: self.priority,
            local_routing: self.local_routing,
        }
    }
}

impl<'a, 'b> PublisherBuilder<'a, 'b> {
    /// Change the `congestion_control` to apply when routing the data.
    #[inline]
    pub fn congestion_control(mut self, congestion_control: CongestionControl) -> Self {
        self.congestion_control = congestion_control;
        self
    }

    /// Change the priority of the written data.
    #[inline]
    pub fn priority(mut self, priority: Priority) -> Self {
        self.priority = priority;
        self
    }

    /// Enable or disable local routing.
    #[inline]
    pub fn local_routing(mut self, local_routing: bool) -> Self {
        self.local_routing = Some(local_routing);
        self
    }
}

impl<'a> Resolvable for PublisherBuilder<'a, 'a> {
    type Output = ZResult<Publisher<'a>>;
}
impl<'a> SyncResolve for PublisherBuilder<'a, 'a> {
    #[inline]
    fn res_sync(self) -> Self::Output {
        let mut key_expr = self.key_expr?;
        if !key_expr.is_fully_optimized(&self.session) {
            let session_id = self.session.id;
            let expr_id = self.session.declare_prefix(key_expr.as_str()).res_sync();
            let prefix_len = key_expr
                .len()
                .try_into()
                .expect("How did you get a key expression with a length over 2^32!?");
            key_expr = match key_expr.0 {
                crate::key_expr::KeyExprInner::Borrowed(key_expr)
                | crate::key_expr::KeyExprInner::BorrowedWire { key_expr, .. } => {
                    KeyExpr(crate::key_expr::KeyExprInner::BorrowedWire {
                        key_expr,
                        expr_id,
                        prefix_len,
                        session_id,
                    })
                }
                crate::key_expr::KeyExprInner::Owned(key_expr)
                | crate::key_expr::KeyExprInner::Wire { key_expr, .. } => {
                    KeyExpr(crate::key_expr::KeyExprInner::Wire {
                        key_expr,
                        expr_id,
                        prefix_len,
                        session_id,
                    })
                }
            }
        }
        self.session.declare_publication_intent(&key_expr);
        let publisher = Publisher {
            session: self.session,
            key_expr,
            congestion_control: self.congestion_control,
            priority: self.priority,
            local_routing: self.local_routing,
        };
        log::trace!("publish({:?})", publisher.key_expr);
        Ok(publisher)
    }
}
impl<'a> AsyncResolve for PublisherBuilder<'a, 'a> {
    type Future = futures::future::Ready<Self::Output>;

    fn res_async(self) -> Self::Future {
        futures::future::ready(self.res_sync())
    }
}
