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
use std::future::Ready;
use zenoh_core::{zread, AsyncResolve, Resolvable, Resolve, Result as ZResult, SyncResolve};
use zenoh_protocol::{core::Channel, zenoh::DataInfo};

/// The kind of congestion control.
pub use zenoh_protocol::core::CongestionControl;

/// A builder for initializing a [`delete`](crate::Session::delete) operation.
///
/// # Examples
/// ```
/// # async_std::task::block_on(async {
/// use zenoh::prelude::r#async::*;
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
/// use zenoh::prelude::r#async::*;
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

    /// Restrict the matching subscribers that will receive the published data
    /// to the ones that have the given [`Locality`](crate::prelude::Locality).
    #[zenoh_core::unstable]
    #[inline]
    pub fn allowed_destination(mut self, destination: Locality) -> Self {
        self.publisher = self.publisher.allowed_destination(destination);
        self
    }

    pub fn kind(mut self, kind: SampleKind) -> Self {
        self.kind = kind;
        self
    }
}

impl Resolvable for PutBuilder<'_, '_> {
    type To = ZResult<()>;
}

impl SyncResolve for PutBuilder<'_, '_> {
    #[inline]
    fn res_sync(self) -> <Self as Resolvable>::To {
        let PutBuilder {
            publisher,
            value,
            kind,
        } = self;
        let key_expr = publisher.key_expr?;
        log::trace!("write({:?}, [...])", &key_expr);
        let primitives = zread!(publisher.session.state)
            .primitives
            .as_ref()
            .unwrap()
            .clone();

        let info = DataInfo {
            kind,
            encoding: if value.encoding != Encoding::default() {
                Some(value.encoding)
            } else {
                None
            },
            timestamp: publisher.session.runtime.new_timestamp(),
            ..Default::default()
        };
        let data_info = if info != DataInfo::default() {
            Some(info)
        } else {
            None
        };

        if publisher.destination != Locality::SessionLocal {
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
        }
        if publisher.destination != Locality::Remote {
            publisher.session.handle_data(
                true,
                &key_expr.to_wire(&publisher.session),
                data_info,
                value.payload,
            );
        }
        Ok(())
    }
}

impl AsyncResolve for PutBuilder<'_, '_> {
    type Future = Ready<Self::To>;

    fn res_async(self) -> Self::Future {
        std::future::ready(self.res_sync())
    }
}

use futures::Sink;
use std::convert::TryFrom;
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
/// use futures::StreamExt;
/// use zenoh::prelude::r#async::*;
///
/// let session = zenoh::open(config::peer()).res().await.unwrap().into_arc();
/// let mut subscriber = session.declare_subscriber("key/expression").res().await.unwrap();
/// let publisher = session.declare_publisher("another/key/expression").res().await.unwrap();
/// subscriber.stream().map(Ok).forward(publisher).await.unwrap();
/// # })
/// ```
#[derive(Debug, Clone)]
pub struct Publisher<'a> {
    pub(crate) session: SessionRef<'a>,
    pub(crate) key_expr: KeyExpr<'a>,
    pub(crate) congestion_control: CongestionControl,
    pub(crate) priority: Priority,
    pub(crate) destination: Locality,
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

    /// Restrict the matching subscribers that will receive the published data
    /// to the ones that have the given [`Locality`](crate::prelude::Locality).
    #[zenoh_core::unstable]
    #[inline]
    pub fn allowed_destination(mut self, destination: Locality) -> Self {
        self.destination = destination;
        self
    }

    fn _write(&self, kind: SampleKind, value: Value) -> Publication {
        Publication {
            publisher: self,
            value,
            kind,
        }
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
        self._write(kind, value.into())
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
        self._write(SampleKind::Put, value.into())
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
        self._write(SampleKind::Delete, Value::empty())
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
        Undeclarable::undeclare_inner(self, ())
    }
}

impl<'a> Undeclarable<(), PublisherUndeclaration<'a>> for Publisher<'a> {
    fn undeclare_inner(self, _: ()) -> PublisherUndeclaration<'a> {
        PublisherUndeclaration { publisher: self }
    }
}

/// A [`Resolvable`] returned when undeclaring a publisher.
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
pub struct PublisherUndeclaration<'a> {
    publisher: Publisher<'a>,
}

impl Resolvable for PublisherUndeclaration<'_> {
    type To = ZResult<()>;
}

impl SyncResolve for PublisherUndeclaration<'_> {
    fn res_sync(mut self) -> <Self as Resolvable>::To {
        let Publisher {
            session, key_expr, ..
        } = &self.publisher;
        session
            .undeclare_publication_intent(key_expr.clone())
            .res_sync()?;
        self.publisher.key_expr = unsafe { keyexpr::from_str_unchecked("") }.into();
        Ok(())
    }
}

impl AsyncResolve for PublisherUndeclaration<'_> {
    type Future = Ready<Self::To>;

    fn res_async(self) -> Self::Future {
        std::future::ready(self.res_sync())
    }
}

impl Drop for Publisher<'_> {
    fn drop(&mut self) {
        if !self.key_expr.is_empty() {
            let _ = self
                .session
                .undeclare_publication_intent(self.key_expr.clone())
                .res_sync();
        }
    }
}

/// A [`Resolvable`] returned by [`Publisher::put()`](Publisher::put),
/// [`Publisher::delete()`](Publisher::delete) and [`Publisher::write()`](Publisher::write).
pub struct Publication<'a> {
    publisher: &'a Publisher<'a>,
    value: Value,
    kind: SampleKind,
}

impl Resolvable for Publication<'_> {
    type To = ZResult<()>;
}

impl SyncResolve for Publication<'_> {
    fn res_sync(self) -> <Self as Resolvable>::To {
        let Publication {
            publisher,
            value,
            kind,
        } = self;
        log::trace!("write({:?}, [...])", publisher.key_expr);
        let primitives = zread!(publisher.session.state)
            .primitives
            .as_ref()
            .unwrap()
            .clone();

        let info = DataInfo {
            kind,
            encoding: if value.encoding != Encoding::default() {
                Some(value.encoding)
            } else {
                None
            },
            timestamp: publisher.session.runtime.new_timestamp(),
            ..Default::default()
        };
        let data_info = if info != DataInfo::default() {
            Some(info)
        } else {
            None
        };

        if publisher.destination != Locality::SessionLocal {
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
        }
        if publisher.destination != Locality::Remote {
            publisher.session.handle_data(
                true,
                &publisher.key_expr.to_wire(&publisher.session),
                data_info,
                value.payload,
            );
        }
        Ok(())
    }
}

impl AsyncResolve for Publication<'_> {
    type Future = Ready<Self::To>;

    fn res_async(self) -> Self::Future {
        std::future::ready(self.res_sync())
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
/// use zenoh::prelude::r#async::*;
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
pub struct PublisherBuilder<'a, 'b: 'a> {
    pub(crate) session: SessionRef<'a>,
    pub(crate) key_expr: ZResult<KeyExpr<'b>>,
    pub(crate) congestion_control: CongestionControl,
    pub(crate) priority: Priority,
    pub(crate) destination: Locality,
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
            destination: self.destination,
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

    /// Restrict the matching subscribers that will receive the published data
    /// to the ones that have the given [`Locality`](crate::prelude::Locality).
    #[zenoh_core::unstable]
    #[inline]
    pub fn allowed_destination(mut self, destination: Locality) -> Self {
        self.destination = destination;
        self
    }
}

impl<'a, 'b> Resolvable for PublisherBuilder<'a, 'b> {
    type To = ZResult<Publisher<'a>>;
}

impl<'a, 'b> SyncResolve for PublisherBuilder<'a, 'b> {
    fn res_sync(self) -> <Self as Resolvable>::To {
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
        self.session
            .declare_publication_intent(key_expr.clone())
            .res_sync()?;
        let publisher = Publisher {
            session: self.session,
            key_expr,
            congestion_control: self.congestion_control,
            priority: self.priority,
            destination: self.destination,
        };
        log::trace!("publish({:?})", publisher.key_expr);
        Ok(publisher)
    }
}

impl<'a, 'b> AsyncResolve for PublisherBuilder<'a, 'b> {
    type Future = Ready<Self::To>;

    fn res_async(self) -> Self::Future {
        std::future::ready(self.res_sync())
    }
}

/// The Priority of zenoh messages.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(u8)]
pub enum Priority {
    RealTime = 1,
    InteractiveHigh = 2,
    InteractiveLow = 3,
    DataHigh = 4,
    Data = 5,
    DataLow = 6,
    Background = 7,
}

impl Priority {
    /// The lowest Priority
    pub const MIN: Self = Self::Background;
    /// The highest Priority
    pub const MAX: Self = Self::RealTime;
    /// The number of available priorities
    pub const NUM: usize = 1 + Self::MIN as usize - Self::MAX as usize;
}

impl Default for Priority {
    fn default() -> Priority {
        Priority::Data
    }
}

impl TryFrom<u8> for Priority {
    type Error = zenoh_core::Error;

    /// A Priority is identified by a numeric value.
    /// Lower the value, higher the priority.
    /// Higher the value, lower the priority.
    ///
    /// Admitted values are: 1-7.
    ///
    /// Highest priority: 1
    /// Lowest priority: 7
    fn try_from(priority: u8) -> Result<Self, Self::Error> {
        match priority {
            1 => Ok(Priority::RealTime),
            2 => Ok(Priority::InteractiveHigh),
            3 => Ok(Priority::InteractiveLow),
            4 => Ok(Priority::DataHigh),
            5 => Ok(Priority::Data),
            6 => Ok(Priority::DataLow),
            7 => Ok(Priority::Background),
            unknown => bail!(
                "{} is not a valid priority value. Admitted values are: [{}-{}].",
                unknown,
                Self::MAX as u8,
                Self::MIN as u8
            ),
        }
    }
}

impl From<Priority> for zenoh_protocol::core::Priority {
    fn from(prio: Priority) -> Self {
        // The Priority in the prelude differs from the Priority in the core protocol only from
        // the missing Control priority. The Control priority is reserved for zenoh internal use
        // and as such it is not exposed by the zenoh API. Nevertheless, the values of the
        // priorities which are common to the internal and public Priority enums are the same. Therefore,
        // it is possible to safely transmute from the public Priority enum toward the internal
        // Priority enum without risking to be in an invalid state.
        // For better robusteness, the correctness of the unsafe transmute operation is covered
        // by the unit test below.
        unsafe { std::mem::transmute::<Priority, zenoh_protocol::core::Priority>(prio) }
    }
}

mod tests {
    #[test]
    fn priority_from() {
        use super::Priority as APrio;
        use std::convert::TryInto;
        use zenoh_protocol::core::Priority as TPrio;

        for i in APrio::MAX as u8..=APrio::MIN as u8 {
            let p: APrio = i.try_into().unwrap();

            match p {
                APrio::RealTime => assert_eq!(p as u8, TPrio::RealTime as u8),
                APrio::InteractiveHigh => assert_eq!(p as u8, TPrio::InteractiveHigh as u8),
                APrio::InteractiveLow => assert_eq!(p as u8, TPrio::InteractiveLow as u8),
                APrio::DataHigh => assert_eq!(p as u8, TPrio::DataHigh as u8),
                APrio::Data => assert_eq!(p as u8, TPrio::Data as u8),
                APrio::DataLow => assert_eq!(p as u8, TPrio::DataLow as u8),
                APrio::Background => assert_eq!(p as u8, TPrio::Background as u8),
            }

            let t: TPrio = p.into();
            assert_eq!(p as u8, t as u8);
        }
    }
}
