//
// Copyright (c) 2017, 2020 ADLINK Technology Inc.
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ADLINK zenoh team, <zenoh@adlink-labs.tech>
//

//! Publishing primitives.

use super::net::protocol::core::Channel;
use super::net::protocol::proto::{data_kind, DataInfo, Options};
use crate::net::transport::Primitives;
use crate::prelude::*;
use crate::subscriber::Reliability;
use crate::Encoding;
use crate::Session;
use zenoh_core::zread;
use zenoh_sync::{derive_zfuture, Runnable};

/// The kind of congestion control.
pub use super::net::protocol::core::CongestionControl;

derive_zfuture! {
    /// A builder for initializing a `write` operation ([`put`](crate::Session::put) or [`delete`](crate::Session::delete)).
    ///
    /// The `write` operation can be run synchronously via [`wait()`](ZFuture::wait()) or asynchronously via `.await`.
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::*;
    /// use zenoh::publication::CongestionControl;
    ///
    /// let session = zenoh::open(config::peer()).await.unwrap();
    /// session
    ///     .put("/key/expression", "value")
    ///     .encoding(Encoding::TEXT_PLAIN)
    ///     .congestion_control(CongestionControl::Block)
    ///     .await
    ///     .unwrap();
    /// # })
    /// ```
    #[derive(Debug, Clone)]
    pub struct Writer<'a> {
        pub(crate) session: &'a Session,
        pub(crate) key_expr: KeyExpr<'a>,
        pub(crate) value: Option<Value>,
        pub(crate) kind: Option<ZInt>,
        pub(crate) congestion_control: CongestionControl,
        pub(crate) priority: Priority,
    }
}

impl<'a> Writer<'a> {
    /// Change the `congestion_control` to apply when routing the data.
    #[inline]
    pub fn congestion_control(mut self, congestion_control: CongestionControl) -> Writer<'a> {
        self.congestion_control = congestion_control;
        self
    }

    /// Change the kind of the written data.
    #[inline]
    pub fn kind(mut self, kind: SampleKind) -> Self {
        self.kind = Some(kind as ZInt);
        self
    }

    /// Change the encoding of the written data.
    #[inline]
    pub fn encoding<IntoEncoding>(mut self, encoding: IntoEncoding) -> Self
    where
        IntoEncoding: Into<Encoding>,
    {
        if let Some(mut payload) = self.value.as_mut() {
            payload.encoding = encoding.into();
        } else {
            self.value = Some(Value::empty().encoding(encoding.into()));
        }
        self
    }

    /// Change the priority of the written data.
    #[inline]
    pub fn priority(mut self, priority: Priority) -> Writer<'a> {
        self.priority = priority;
        self
    }

    fn write(&self, value: Value) -> zenoh_core::Result<()> {
        log::trace!("write({:?}, [...])", self.key_expr);
        let state = zread!(self.session.state);
        let primitives = state.primitives.as_ref().unwrap().clone();
        drop(state);

        let mut info = DataInfo::new();
        info.kind = match self.kind {
            Some(data_kind::DEFAULT) => None,
            kind => kind,
        };
        info.encoding = if value.encoding != Encoding::default() {
            Some(value.encoding)
        } else {
            None
        };
        info.timestamp = self.session.runtime.new_timestamp();
        let data_info = if info.has_options() { Some(info) } else { None };

        primitives.send_data(
            &self.key_expr,
            value.payload.clone(),
            Channel {
                priority: self.priority.into(),
                reliability: Reliability::Reliable, // @TODO: need to check subscriptions to determine the right reliability value
            },
            self.congestion_control,
            data_info.clone(),
            None,
        );
        self.session
            .handle_data(true, &self.key_expr, data_info, value.payload);
        Ok(())
    }
}

impl Runnable for Writer<'_> {
    type Output = zenoh_core::Result<()>;

    #[inline]
    fn run(&mut self) -> Self::Output {
        let value = self.value.take().unwrap();
        self.write(value)
    }
}

use futures::Sink;
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
/// use zenoh::prelude::*;
///
/// let session = zenoh::open(config::peer()).await.unwrap().into_arc();
/// let publisher = session.publish("/key/expression").await.unwrap();
/// publisher.send("value").unwrap();
/// # })
/// ```
///
///
/// `Publisher` implements the `Sink` trait which is useful to forward
/// streams to zenoh.
/// ```no_run
/// # async_std::task::block_on(async {
/// use zenoh::prelude::*;
///
/// let session = zenoh::open(config::peer()).await.unwrap().into_arc();
/// let mut subscriber = session.subscribe("/key/expression").await.unwrap();
/// let publisher = session.publish("/another/key/expression").await.unwrap();
/// subscriber.forward(publisher).await.unwrap();
/// # })
/// ```
pub type Publisher<'a> = Writer<'a>;

impl Publisher<'_> {
    /// Send a value.
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::*;
    ///
    /// let session = zenoh::open(config::peer()).await.unwrap().into_arc();
    /// let publisher = session.publish("/key/expression").await.unwrap();
    /// publisher.send("value").unwrap();
    /// # })
    /// ```
    #[inline]
    pub fn send<IntoValue>(&self, value: IntoValue) -> zenoh_core::Result<()>
    where
        IntoValue: Into<Value>,
    {
        self.write(value.into())
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
        self.write(item.into())
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
