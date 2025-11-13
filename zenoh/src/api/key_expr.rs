//
// Copyright (c) 2023 ZettaScale Technology
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
    convert::{TryFrom, TryInto},
    future::{IntoFuture, Ready},
    str::FromStr,
};

use zenoh_config::wrappers::EntityId;
use zenoh_core::{Resolvable, Wait};
use zenoh_keyexpr::{keyexpr, OwnedKeyExpr};
use zenoh_protocol::{
    core::{key_expr::canon::Canonize, ExprId, WireExpr},
    network::{declare, DeclareBody, Mapping, UndeclareKeyExpr},
};
use zenoh_result::ZResult;

use crate::api::session::{Session, SessionInner, UndeclarableSealed};

#[derive(Clone, Debug)]
pub(crate) enum KeyExprInner<'a> {
    Borrowed(&'a keyexpr),
    BorrowedWire {
        key_expr: &'a keyexpr,
        expr_id: ExprId,
        mapping: Mapping,
        prefix_len: u32,
        session_id: EntityId,
    },
    Owned(OwnedKeyExpr),
    Wire {
        key_expr: OwnedKeyExpr,
        expr_id: ExprId,
        mapping: Mapping,
        prefix_len: u32,
        session_id: EntityId,
    },
}

/// A possibly-owned version of [`keyexpr`] that may carry optimisations for use with a [`Session`] that may have declared it.
///
/// Check [`keyexpr`]'s documentation for detailed explanations of the Key Expression Language.
#[repr(transparent)]
#[derive(Clone, serde::Deserialize, serde::Serialize)]
#[serde(from = "OwnedKeyExpr")]
#[serde(into = "OwnedKeyExpr")]
pub struct KeyExpr<'a>(pub(crate) KeyExprInner<'a>);
impl std::ops::Deref for KeyExpr<'_> {
    type Target = keyexpr;
    fn deref(&self) -> &Self::Target {
        match &self.0 {
            KeyExprInner::Borrowed(s) => s,
            KeyExprInner::Owned(s) => s,
            KeyExprInner::Wire { key_expr, .. } => key_expr,
            KeyExprInner::BorrowedWire { key_expr, .. } => key_expr,
        }
    }
}

impl KeyExpr<'static> {
    /// Constructs a [`KeyExpr`] without checking [`keyexpr`]'s invariants
    /// # Safety
    /// Key Expressions must follow some rules to be accepted by a Zenoh network.
    /// Messages addressed with invalid key expressions will be dropped.
    pub unsafe fn from_string_unchecked(s: String) -> Self {
        Self(KeyExprInner::Owned(OwnedKeyExpr::from_string_unchecked(s)))
    }

    /// Constructs a [`KeyExpr`] without checking [`keyexpr`]'s invariants
    /// # Safety
    /// Key Expressions must follow some rules to be accepted by a Zenoh network.
    /// Messages addressed with invalid key expressions will be dropped.
    pub unsafe fn from_boxed_str_unchecked(s: Box<str>) -> Self {
        Self(KeyExprInner::Owned(OwnedKeyExpr::from_boxed_str_unchecked(
            s,
        )))
    }
}

#[zenoh_macros::internal]
static KEYEXPR_DUMMY: &keyexpr = unsafe { keyexpr::from_str_unchecked("dummy") };

impl<'a> KeyExpr<'a> {
    /// Equivalent to `<KeyExpr as TryFrom>::try_from(t)`.
    ///
    /// Will return an Err if `t` isn't a valid key expression.
    /// Note that to be considered a valid key expression, a string MUST be canon.
    ///
    /// [`KeyExpr::autocanonize`] is an alternative constructor that will canonize the passed expression before constructing it.
    pub fn new<T, E>(t: T) -> Result<Self, E>
    where
        Self: TryFrom<T, Error = E>,
    {
        Self::try_from(t)
    }

    /// Constructs a key expression object to be used as a dummy value
    /// for empty objects. This method is not supposed to be called in user code,
    /// but may be used in language bindings (zenoh-c)
    #[zenoh_macros::internal]
    pub fn dummy() -> Self {
        Self(KeyExprInner::Borrowed(KEYEXPR_DUMMY))
    }

    /// Checks if the key expression is the dummy one.
    /// This method is not supposed to be called in user code,
    /// but may be used in language bindings (zenoh-c)
    #[zenoh_macros::internal]
    pub fn is_dummy(&self) -> bool {
        let Self(inner) = self;
        let KeyExprInner::Borrowed(key_expr) = inner else {
            return false;
        };
        std::ptr::eq(*key_expr, KEYEXPR_DUMMY)
    }

    /// Constructs a new [`KeyExpr`] aliasing `self`.
    ///
    /// Note that [`KeyExpr`] (as well as [`OwnedKeyExpr`]) use reference counters internally, so you're probably better off using clone.
    pub fn borrowing_clone(&'a self) -> Self {
        let inner = match &self.0 {
            KeyExprInner::Borrowed(key_expr) => KeyExprInner::Borrowed(key_expr),
            KeyExprInner::BorrowedWire {
                key_expr,
                expr_id,
                mapping,
                prefix_len,
                session_id,
            } => KeyExprInner::BorrowedWire {
                key_expr,
                expr_id: *expr_id,
                mapping: *mapping,
                prefix_len: *prefix_len,
                session_id: *session_id,
            },
            KeyExprInner::Owned(key_expr) => KeyExprInner::Borrowed(key_expr),
            KeyExprInner::Wire {
                key_expr,
                expr_id,
                mapping,
                prefix_len,
                session_id,
            } => KeyExprInner::BorrowedWire {
                key_expr,
                expr_id: *expr_id,
                mapping: *mapping,
                prefix_len: *prefix_len,
                session_id: *session_id,
            },
        };
        Self(inner)
    }

    /// Canonizes the passed value before returning it as a `KeyExpr`.
    ///
    /// Will return Err if the passed value isn't a valid key expression despite canonization.
    pub fn autocanonize<T, E>(mut t: T) -> Result<Self, E>
    where
        Self: TryFrom<T, Error = E>,
        T: Canonize,
    {
        t.canonize();
        Self::new(t)
    }

    /// Constructs a [`KeyExpr`] without checking [`keyexpr`]'s invariants
    /// # Safety
    /// Key Expressions must follow some rules to be accepted by a Zenoh network.
    /// Messages addressed with invalid key expressions will be dropped.
    pub unsafe fn from_str_unchecked(s: &'a str) -> Self {
        keyexpr::from_str_unchecked(s).into()
    }

    /// Returns the borrowed version of `self`
    pub fn as_keyexpr(&self) -> &keyexpr {
        self
    }

    /// Ensures `self` owns all of its data, and informs rustc that it does.
    pub fn into_owned(self) -> KeyExpr<'static> {
        match self.0 {
            KeyExprInner::Borrowed(s) => KeyExpr(KeyExprInner::Owned(s.into())),
            KeyExprInner::Owned(s) => KeyExpr(KeyExprInner::Owned(s)),
            KeyExprInner::BorrowedWire {
                key_expr,
                expr_id,
                mapping,
                prefix_len,
                session_id,
            } => KeyExpr(KeyExprInner::Wire {
                key_expr: key_expr.into(),
                expr_id,
                mapping,
                prefix_len,
                session_id,
            }),
            KeyExprInner::Wire {
                key_expr,
                expr_id,
                mapping,
                prefix_len,
                session_id,
            } => KeyExpr(KeyExprInner::Wire {
                key_expr,
                expr_id,
                mapping,
                prefix_len,
                session_id,
            }),
        }
    }

    /// Joins both sides, inserting a `/` in between them.
    ///
    /// This should be your preferred method when concatenating path segments.
    ///
    /// # Examples
    /// ```
    /// # use std::convert::TryFrom;
    /// # use zenoh::key_expr::KeyExpr;
    /// let prefix = KeyExpr::try_from("some/prefix").unwrap();
    /// let suffix = KeyExpr::try_from("some/suffix").unwrap();
    /// let join = prefix.join(&suffix).unwrap();
    /// assert_eq!(join.as_str(), "some/prefix/some/suffix");
    /// ```
    pub fn join<S: AsRef<str> + ?Sized>(&self, s: &S) -> ZResult<KeyExpr<'static>> {
        let r = self.as_keyexpr().join(s)?;
        if let KeyExprInner::Wire {
            expr_id,
            mapping,
            prefix_len,
            session_id,
            ..
        } = &self.0
        {
            Ok(KeyExpr(KeyExprInner::Wire {
                key_expr: r,
                expr_id: *expr_id,
                mapping: *mapping,
                prefix_len: *prefix_len,
                session_id: *session_id,
            }))
        } else {
            Ok(r.into())
        }
    }

    /// Performs string concatenation and returns the result as a [`KeyExpr`] if possible.
    ///
    /// You should probably prefer [`KeyExpr::join`] as Zenoh may then take advantage of the hierarchical separation it inserts.
    pub fn concat<S: AsRef<str> + ?Sized>(&self, s: &S) -> ZResult<KeyExpr<'static>> {
        let s = s.as_ref();
        self._concat(s)
    }

    fn _concat(&self, s: &str) -> ZResult<KeyExpr<'static>> {
        if self.ends_with('*') && s.starts_with('*') {
            bail!("Tried to concatenate {} (ends with *) and {} (starts with *), which would likely have caused bugs. If you're sure you want to do this, concatenate these into a string and then try to convert.", self, s)
        }
        let r = OwnedKeyExpr::try_from(format!("{self}{s}"))?;
        if let KeyExprInner::Wire {
            expr_id,
            mapping,
            prefix_len,
            session_id,
            ..
        }
        | KeyExprInner::BorrowedWire {
            expr_id,
            mapping,
            prefix_len,
            session_id,
            ..
        } = &self.0
        {
            Ok(KeyExpr(KeyExprInner::Wire {
                key_expr: r,
                expr_id: *expr_id,
                mapping: *mapping,
                prefix_len: *prefix_len,
                session_id: *session_id,
            }))
        } else {
            Ok(r.into())
        }
    }

    /// Will return false and log an error in case of a `TryInto` failure.
    #[inline]
    pub(crate) fn keyexpr_include<'b, L, R>(left: L, right: R) -> bool
    where
        L: TryInto<KeyExpr<'a>>,
        R: TryInto<KeyExpr<'b>>,
        L::Error: std::fmt::Display,
        R::Error: std::fmt::Display,
    {
        match left.try_into() {
            Ok(l) => match right.try_into() {
                Ok(r) => {
                    return l.includes(&r);
                }
                Err(e) => {
                    tracing::error!("{e}");
                }
            },
            Err(e) => {
                tracing::error!("{e}");
            }
        }
        false
    }
}

impl FromStr for KeyExpr<'static> {
    type Err = zenoh_result::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(KeyExprInner::Owned(s.parse()?)))
    }
}
impl<'a> From<KeyExpr<'a>> for OwnedKeyExpr {
    fn from(val: KeyExpr<'a>) -> Self {
        match val.0 {
            KeyExprInner::Borrowed(key_expr) | KeyExprInner::BorrowedWire { key_expr, .. } => {
                key_expr.into()
            }
            KeyExprInner::Owned(key_expr) | KeyExprInner::Wire { key_expr, .. } => key_expr,
        }
    }
}
impl AsRef<keyexpr> for KeyExpr<'_> {
    fn as_ref(&self) -> &keyexpr {
        self
    }
}
impl AsRef<str> for KeyExpr<'_> {
    fn as_ref(&self) -> &str {
        self
    }
}
impl<'a> From<&'a keyexpr> for KeyExpr<'a> {
    fn from(ke: &'a keyexpr) -> Self {
        Self(KeyExprInner::Borrowed(ke))
    }
}
impl From<OwnedKeyExpr> for KeyExpr<'_> {
    fn from(v: OwnedKeyExpr) -> Self {
        Self(KeyExprInner::Owned(v))
    }
}
impl<'a> From<&'a OwnedKeyExpr> for KeyExpr<'a> {
    fn from(v: &'a OwnedKeyExpr) -> Self {
        Self(KeyExprInner::Borrowed(v))
    }
}
impl<'a> From<&'a KeyExpr<'a>> for KeyExpr<'a> {
    fn from(val: &'a KeyExpr<'a>) -> Self {
        match &val.0 {
            KeyExprInner::Borrowed(key_expr) => Self(KeyExprInner::Borrowed(key_expr)),
            KeyExprInner::BorrowedWire {
                key_expr,
                expr_id,
                mapping,
                prefix_len,
                session_id,
            } => Self(KeyExprInner::BorrowedWire {
                key_expr,
                expr_id: *expr_id,
                mapping: *mapping,
                prefix_len: *prefix_len,
                session_id: *session_id,
            }),
            KeyExprInner::Owned(key_expr) => Self(KeyExprInner::Borrowed(key_expr)),
            KeyExprInner::Wire {
                key_expr,
                expr_id,
                mapping,
                prefix_len,
                session_id,
            } => Self(KeyExprInner::BorrowedWire {
                key_expr,
                expr_id: *expr_id,
                mapping: *mapping,
                prefix_len: *prefix_len,
                session_id: *session_id,
            }),
        }
    }
}
impl From<KeyExpr<'_>> for String {
    fn from(ke: KeyExpr) -> Self {
        match ke.0 {
            KeyExprInner::Borrowed(key_expr) | KeyExprInner::BorrowedWire { key_expr, .. } => {
                key_expr.as_str().to_owned()
            }
            KeyExprInner::Owned(key_expr) | KeyExprInner::Wire { key_expr, .. } => key_expr.into(),
        }
    }
}

impl TryFrom<String> for KeyExpr<'_> {
    type Error = zenoh_result::Error;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Ok(Self(KeyExprInner::Owned(value.try_into()?)))
    }
}
impl<'a> TryFrom<&'a String> for KeyExpr<'a> {
    type Error = zenoh_result::Error;
    fn try_from(value: &'a String) -> Result<Self, Self::Error> {
        Self::try_from(value.as_str())
    }
}
impl<'a> TryFrom<&'a mut String> for KeyExpr<'a> {
    type Error = zenoh_result::Error;
    fn try_from(value: &'a mut String) -> Result<Self, Self::Error> {
        Ok(Self::from(keyexpr::new(value)?))
    }
}
impl<'a> TryFrom<&'a str> for KeyExpr<'a> {
    type Error = zenoh_result::Error;
    fn try_from(value: &'a str) -> Result<Self, Self::Error> {
        Ok(Self(KeyExprInner::Borrowed(value.try_into()?)))
    }
}
impl<'a> TryFrom<&'a mut str> for KeyExpr<'a> {
    type Error = zenoh_result::Error;
    fn try_from(value: &'a mut str) -> Result<Self, Self::Error> {
        Ok(Self(KeyExprInner::Borrowed(value.try_into()?)))
    }
}
impl std::fmt::Debug for KeyExpr<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self.as_keyexpr(), f)
    }
}
impl std::fmt::Display for KeyExpr<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self.as_keyexpr(), f)
    }
}
impl PartialEq for KeyExpr<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.as_keyexpr() == other.as_keyexpr()
    }
}
impl<T: PartialEq<keyexpr>> PartialEq<T> for KeyExpr<'_> {
    fn eq(&self, other: &T) -> bool {
        other == self.as_keyexpr()
    }
}
impl Eq for KeyExpr<'_> {}
impl std::hash::Hash for KeyExpr<'_> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.as_keyexpr().hash(state);
    }
}

impl std::ops::Div<&keyexpr> for KeyExpr<'_> {
    type Output = KeyExpr<'static>;

    fn div(self, rhs: &keyexpr) -> Self::Output {
        match self.0 {
            KeyExprInner::Borrowed(key_expr) => (key_expr / rhs).into(),
            KeyExprInner::BorrowedWire {
                key_expr,
                expr_id,
                mapping,
                prefix_len,
                session_id,
            } => KeyExpr(KeyExprInner::Wire {
                key_expr: key_expr / rhs,
                expr_id,
                mapping,
                prefix_len,
                session_id,
            }),
            KeyExprInner::Owned(key_expr) => (key_expr / rhs).into(),
            KeyExprInner::Wire {
                key_expr,
                expr_id,
                mapping,
                prefix_len,
                session_id,
            } => KeyExpr(KeyExprInner::Wire {
                key_expr: key_expr / rhs,
                expr_id,
                mapping,
                prefix_len,
                session_id,
            }),
        }
    }
}
impl std::ops::Div<&keyexpr> for &KeyExpr<'_> {
    type Output = KeyExpr<'static>;

    fn div(self, rhs: &keyexpr) -> Self::Output {
        match &self.0 {
            KeyExprInner::Borrowed(key_expr) => (*key_expr / rhs).into(),
            KeyExprInner::BorrowedWire {
                key_expr,
                expr_id,
                mapping,
                prefix_len,
                session_id,
            } => KeyExpr(KeyExprInner::Wire {
                key_expr: *key_expr / rhs,
                expr_id: *expr_id,
                mapping: *mapping,
                prefix_len: *prefix_len,
                session_id: *session_id,
            }),
            KeyExprInner::Owned(key_expr) => (key_expr / rhs).into(),
            KeyExprInner::Wire {
                key_expr,
                expr_id,
                mapping,
                prefix_len,
                session_id,
            } => KeyExpr(KeyExprInner::Wire {
                key_expr: key_expr / rhs,
                expr_id: *expr_id,
                mapping: *mapping,
                prefix_len: *prefix_len,
                session_id: *session_id,
            }),
        }
    }
}

impl<'a> KeyExpr<'a> {
    //pub(crate) fn is_optimized(&self, session: &Session) -> bool {
    //    matches!(&self.0, KeyExprInner::Wire { expr_id, session_id, .. } | KeyExprInner::BorrowedWire { expr_id, session_id, .. } if *expr_id != 0 && session.id == *session_id)
    //}
    pub(crate) fn is_fully_optimized(&self, session: &SessionInner) -> bool {
        match &self.0 {
            KeyExprInner::Wire {
                key_expr,
                session_id,
                prefix_len,
                ..
            } if session.id == *session_id && key_expr.len() as u32 == *prefix_len => true,
            KeyExprInner::BorrowedWire {
                key_expr,
                session_id,
                prefix_len,
                ..
            } if session.id == *session_id && key_expr.len() as u32 == *prefix_len => true,
            _ => false,
        }
    }
    pub(crate) fn to_wire(&'a self, session: &SessionInner) -> WireExpr<'a> {
        match &self.0 {
            KeyExprInner::Wire {
                key_expr,
                expr_id,
                mapping,
                prefix_len,
                session_id,
            } if session.id == *session_id => WireExpr {
                scope: *expr_id,
                suffix: std::borrow::Cow::Borrowed(&key_expr.as_str()[((*prefix_len) as usize)..]),
                mapping: *mapping,
            },
            KeyExprInner::BorrowedWire {
                key_expr,
                expr_id,
                mapping,
                prefix_len,
                session_id,
            } if session.id == *session_id => WireExpr {
                scope: *expr_id,
                suffix: std::borrow::Cow::Borrowed(&key_expr.as_str()[((*prefix_len) as usize)..]),
                mapping: *mapping,
            },
            KeyExprInner::Owned(key_expr) | KeyExprInner::Wire { key_expr, .. } => WireExpr {
                scope: 0,
                suffix: std::borrow::Cow::Borrowed(key_expr.as_str()),
                mapping: Mapping::Sender,
            },
            KeyExprInner::Borrowed(key_expr) | KeyExprInner::BorrowedWire { key_expr, .. } => {
                WireExpr {
                    scope: 0,
                    suffix: std::borrow::Cow::Borrowed(key_expr.as_str()),
                    mapping: Mapping::Sender,
                }
            }
        }
    }
}

impl<'a> UndeclarableSealed<&'a Session> for KeyExpr<'a> {
    type Undeclaration = KeyExprUndeclaration<'a>;

    fn undeclare_inner(self, session: &'a Session) -> Self::Undeclaration {
        KeyExprUndeclaration {
            session,
            expr: self,
        }
    }
}

/// A [`Resolvable`] returned by [`Session::undeclare`] when undeclaring a [`KeyExpr`]
///
/// # Examples
/// ```
/// # #[tokio::main]
/// # async fn main() {
///
/// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
/// let key_expr = session.declare_keyexpr("key/expression").await.unwrap();
/// session.undeclare(key_expr).await.unwrap();
/// # }
/// ```
#[must_use = "Resolvables do nothing unless you resolve them using `.await` or `zenoh::Wait::wait`"]
pub struct KeyExprUndeclaration<'a> {
    session: &'a Session,
    expr: KeyExpr<'a>,
}

impl Resolvable for KeyExprUndeclaration<'_> {
    type To = ZResult<()>;
}

impl Wait for KeyExprUndeclaration<'_> {
    fn wait(self) -> <Self as Resolvable>::To {
        let KeyExprUndeclaration { session, expr } = self;
        let expr_id = match &expr.0 {
            KeyExprInner::Wire {
                key_expr,
                expr_id,
                prefix_len,
                session_id,
                ..
            } if *prefix_len as usize == key_expr.len() => {
                if *session_id == session.0.id {
                    *expr_id
                } else {
                    return Err(zerror!("Failed to undeclare {}, as it was declared by another Session", expr).into())
                }
            }
            KeyExprInner::BorrowedWire {
                key_expr,
                expr_id,
                prefix_len,
                session_id,
                ..
            } if *prefix_len as usize == key_expr.len() => {
                if *session_id == session.0.id {
                    *expr_id
                } else {
                    return Err(zerror!("Failed to undeclare {}, as it was declared by another Session", expr).into())
                }
            }
            _ => return Err(zerror!("Failed to undeclare {}, make sure you use the result of `Session::declare_keyexpr` to call `Session::undeclare`", expr).into()),
        };
        tracing::trace!("undeclare_keyexpr({:?})", expr_id);
        let mut state = zwrite!(session.0.state);
        state.local_resources.remove(&expr_id);

        let primitives = state.primitives()?;
        drop(state);
        primitives.send_declare(&mut zenoh_protocol::network::Declare {
            interest_id: None,
            ext_qos: declare::ext::QoSType::DECLARE,
            ext_tstamp: None,
            ext_nodeid: declare::ext::NodeIdType::DEFAULT,
            body: DeclareBody::UndeclareKeyExpr(UndeclareKeyExpr { id: expr_id }),
        });

        Ok(())
    }
}

impl IntoFuture for KeyExprUndeclaration<'_> {
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}

#[cfg(target_pointer_width = "64")]
#[allow(non_snake_case)]
#[test]
fn size_of_KeyExpr() {
    assert_eq!(
        std::mem::size_of::<KeyExpr>(),
        4 * std::mem::size_of::<usize>()
    );
    assert_eq!(
        std::mem::size_of::<Option<KeyExpr>>(),
        4 * std::mem::size_of::<usize>()
    );
}
