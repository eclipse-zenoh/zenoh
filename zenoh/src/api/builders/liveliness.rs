use std::future::{IntoFuture, Ready};

use zenoh_core::{Resolvable, Result as ZResult, Wait};

use crate::{Session, key_expr::KeyExpr, liveliness::LivelinessToken};

/// A builder for initializing a [`LivelinessToken`](LivelinessToken)
/// returned by the [`Liveliness::declare_token`] method.
///
/// # Examples
/// ```
/// # #[tokio::main]
/// # async fn main() {
///
/// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
/// let liveliness = session
///     .liveliness()
///     .declare_token("key/expression")
///     .await
///     .unwrap();
/// # }
/// ```
#[must_use = "Resolvables do nothing unless you resolve them using `.await` or `zenoh::Wait::wait`"]
#[derive(Debug)]
pub struct LivelinessTokenBuilder<'a, 'b> {
    pub(crate) session: &'a Session,
    pub(crate) key_expr: ZResult<KeyExpr<'b>>,
}

impl Resolvable for LivelinessTokenBuilder<'_, '_> {
    type To = ZResult<LivelinessToken>;
}

impl Wait for LivelinessTokenBuilder<'_, '_> {
    #[inline]
    fn wait(self) -> <Self as Resolvable>::To {
        let session = self.session;
        let key_expr = self.key_expr?.into_owned();
        session
            .0
            .declare_liveliness_inner(&key_expr)
            .map(|id| LivelinessToken {
                session: self.session.downgrade(),
                id,
                undeclare_on_drop: true,
            })
    }
}

impl IntoFuture for LivelinessTokenBuilder<'_, '_> {
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}

