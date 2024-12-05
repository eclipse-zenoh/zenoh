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

use core::fmt;
use std::{
    future::{IntoFuture, Ready},
    time::Duration,
};

use tracing::error;
use zenoh_core::{Resolvable, Resolve, Wait};
use zenoh_protocol::{
    core::{CongestionControl, Parameters},
    network::request::ext::QueryTarget,
};
use zenoh_result::ZResult;
#[cfg(feature = "unstable")]
use {
    crate::api::builders::matching_listener::MatchingListenerBuilder,
    crate::api::matching::{MatchingStatus, MatchingStatusType},
    crate::api::sample::SourceInfo,
    crate::query::ReplyKeyExpr,
    std::collections::HashSet,
    std::sync::{Arc, Mutex},
    zenoh_config::wrappers::EntityGlobalId,
    zenoh_protocol::core::EntityGlobalIdProto,
};

use super::{
    builders::querier::QuerierGetBuilder,
    key_expr::KeyExpr,
    query::QueryConsolidation,
    sample::{Locality, QoS},
    session::{UndeclarableSealed, WeakSession},
    Id,
};
use crate::{api::handlers::DefaultHandler, qos::Priority};

pub(crate) struct QuerierState {
    pub(crate) id: Id,
    pub(crate) remote_id: Id,
    pub(crate) key_expr: KeyExpr<'static>,
    pub(crate) destination: Locality,
}

/// A querier that allows to send queries to a queryable.
///
/// Queriers are automatically undeclared when dropped.
///
/// # Examples
/// ```
/// # #[tokio::main]
/// # async fn main() {
///
/// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
/// let querier = session.declare_querier("key/expression").await.unwrap();
/// let replies = querier.get().await.unwrap();
/// # }
/// ```
#[zenoh_macros::unstable]
#[derive(Debug)]
pub struct Querier<'a> {
    pub(crate) session: WeakSession,
    pub(crate) id: Id,
    pub(crate) key_expr: KeyExpr<'a>,
    pub(crate) qos: QoS,
    pub(crate) destination: Locality,
    pub(crate) target: QueryTarget,
    pub(crate) consolidation: QueryConsolidation,
    pub(crate) timeout: Duration,
    #[cfg(feature = "unstable")]
    pub(crate) accept_replies: ReplyKeyExpr,
    pub(crate) undeclare_on_drop: bool,
    #[cfg(feature = "unstable")]
    pub(crate) matching_listeners: Arc<Mutex<HashSet<Id>>>,
}

impl fmt::Debug for QuerierState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Querier")
            .field("id", &self.id)
            .field("key_expr", &self.key_expr)
            .finish()
    }
}

impl<'a> Querier<'a> {
    /// Returns the [`EntityGlobalId`] of this Querier.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let querier = session.declare_querier("key/expression")
    ///     .await
    ///     .unwrap();
    /// let querier_id = querier.id();
    /// # }
    /// ```
    #[zenoh_macros::unstable]
    pub fn id(&self) -> EntityGlobalId {
        EntityGlobalIdProto {
            zid: self.session.zid().into(),
            eid: self.id,
        }
        .into()
    }

    #[inline]
    #[zenoh_macros::unstable]
    pub fn key_expr(&self) -> &KeyExpr<'a> {
        &self.key_expr
    }

    /// Get the `congestion_control` applied when routing the data.
    #[inline]
    #[zenoh_macros::unstable]
    pub fn congestion_control(&self) -> CongestionControl {
        self.qos.congestion_control()
    }

    /// Get the priority of the written data.
    #[inline]
    #[zenoh_macros::unstable]
    pub fn priority(&self) -> Priority {
        self.qos.priority()
    }

    /// Get type of queryables that can reply to this querier
    #[inline]
    #[zenoh_macros::unstable]
    pub fn accept_replies(&self) -> ReplyKeyExpr {
        self.accept_replies
    }

    /// Send a query.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let querier = session.declare_querier("key/expression").await.unwrap();
    /// let replies = querier.get();
    /// # }
    /// ```
    #[inline]
    #[zenoh_macros::unstable]
    pub fn get(&self) -> QuerierGetBuilder<'_, '_, DefaultHandler> {
        QuerierGetBuilder {
            querier: self,
            #[cfg(feature = "unstable")]
            source_info: SourceInfo::empty(),
            value: None,
            attachment: None,
            parameters: Parameters::empty(),
            handler: DefaultHandler::default(),
        }
    }

    /// Undeclare the [`Querier`], informing the network that it needn't optimize queries for its key expression anymore.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let querier = session.declare_querier("key/expression").await.unwrap();
    /// querier.undeclare().await.unwrap();
    /// # }
    /// ```
    #[zenoh_macros::unstable]
    pub fn undeclare(self) -> impl Resolve<ZResult<()>> + 'a {
        UndeclarableSealed::undeclare_inner(self, ())
    }

    fn undeclare_impl(&mut self) -> ZResult<()> {
        // set the flag first to avoid double panic if this function panic
        self.undeclare_on_drop = false;
        #[cfg(feature = "unstable")]
        {
            let ids: Vec<Id> = zlock!(self.matching_listeners).drain().collect();
            for id in ids {
                self.session.undeclare_matches_listener_inner(id)?
            }
        }
        self.session.undeclare_querier_inner(self.id)
    }

    /// Return the [`MatchingStatus`] of the querier.
    ///
    /// [`MatchingStatus::matching`] will return true if there exist Queryables
    /// matching the Queriers's key expression and target and false otherwise.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let querier = session.declare_querier("key/expression").await.unwrap();
    /// let matching_queriers: bool = querier
    ///     .matching_status()
    ///     .await
    ///     .unwrap()
    ///     .matching();
    /// # }
    /// ```
    #[zenoh_macros::unstable]
    pub fn matching_status(&self) -> impl Resolve<ZResult<MatchingStatus>> + '_ {
        zenoh_core::ResolveFuture::new(async move {
            self.session.matching_status(
                self.key_expr(),
                self.destination,
                MatchingStatusType::Queryables(self.target == QueryTarget::AllComplete),
            )
        })
    }

    /// Return a [`MatchingListener`](crate::api::matching::MatchingListener) for this Querier.
    ///
    /// The [`MatchingListener`](crate::api::matching::MatchingListener) that will send a notification each time the [`MatchingStatus`](crate::api::matching::MatchingStatus) of
    /// the Querier changes.
    ///
    /// # Examples
    /// ```no_run
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let querier = session.declare_querier("key/expression").await.unwrap();
    /// let matching_listener = querier.matching_listener().await.unwrap();
    /// while let Ok(matching_status) = matching_listener.recv_async().await {
    ///     if matching_status.matching() {
    ///         println!("Querier has matching queryables.");
    ///     } else {
    ///         println!("Querier has NO MORE matching queryables.");
    ///     }
    /// }
    /// # }
    /// ```
    #[zenoh_macros::unstable]
    pub fn matching_listener(&self) -> MatchingListenerBuilder<'_, DefaultHandler> {
        MatchingListenerBuilder {
            session: &self.session,
            key_expr: &self.key_expr,
            destination: self.destination,
            matching_listeners: &self.matching_listeners,
            matching_status_type: MatchingStatusType::Queryables(
                self.target == QueryTarget::AllComplete,
            ),
            handler: DefaultHandler::default(),
        }
    }
}

impl<'a> UndeclarableSealed<()> for Querier<'a> {
    type Undeclaration = QuerierUndeclaration<'a>;

    fn undeclare_inner(self, _: ()) -> Self::Undeclaration {
        QuerierUndeclaration(self)
    }
}

/// A [`Resolvable`] returned when undeclaring a publisher.
///
/// # Examples
/// ```
/// # #[tokio::main]
/// # async fn main() {
///
/// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
/// let querier = session.declare_querier("key/expression").await.unwrap();
/// querier.undeclare().await.unwrap();
/// # }
/// ```
#[must_use = "Resolvables do nothing unless you resolve them using `.await` or `zenoh::Wait::wait`"]
pub struct QuerierUndeclaration<'a>(Querier<'a>);

impl Resolvable for QuerierUndeclaration<'_> {
    type To = ZResult<()>;
}

impl Wait for QuerierUndeclaration<'_> {
    fn wait(mut self) -> <Self as Resolvable>::To {
        self.0.undeclare_impl()
    }
}

impl IntoFuture for QuerierUndeclaration<'_> {
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}

impl Drop for Querier<'_> {
    fn drop(&mut self) {
        if self.undeclare_on_drop {
            if let Err(error) = self.undeclare_impl() {
                error!(error);
            }
        }
    }
}
