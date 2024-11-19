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
    crate::api::sample::SourceInfo, crate::query::ReplyKeyExpr,
    zenoh_config::wrappers::EntityGlobalId, zenoh_protocol::core::EntityGlobalIdProto,
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
    pub fn key_expr(&self) -> &KeyExpr<'a> {
        &self.key_expr
    }

    /// Get the `congestion_control` applied when routing the data.
    #[inline]
    pub fn congestion_control(&self) -> CongestionControl {
        self.qos.congestion_control()
    }

    /// Get the priority of the written data.
    #[inline]
    pub fn priority(&self) -> Priority {
        self.qos.priority()
    }

    /// Get type of queryables that can reply to this querier
    #[zenoh_macros::unstable]
    #[inline]
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
    pub fn undeclare(self) -> impl Resolve<ZResult<()>> + 'a {
        UndeclarableSealed::undeclare_inner(self, ())
    }

    fn undeclare_impl(&mut self) -> ZResult<()> {
        // set the flag first to avoid double panic if this function panic
        self.undeclare_on_drop = false;
        self.session.undeclare_querier_inner(self.id)
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
