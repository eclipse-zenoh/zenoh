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
use super::net::protocol::message::{data_kind, DataInfo, Options};
use super::net::transport::Primitives;
use crate::prelude::*;
use crate::subscriber::Reliability;
use crate::Encoding;
use crate::Session;
use zenoh_util::sync::Runnable;

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
}

impl Runnable for Writer<'_> {
    type Output = zenoh_util::core::Result<()>;

    fn run(&mut self) -> Self::Output {
        log::trace!("write({:?}, [...])", self.key_expr);
        let state = zread!(self.session.state);
        let primitives = state.primitives.as_ref().unwrap().clone();
        drop(state);

        let value = self.value.take().unwrap();
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
