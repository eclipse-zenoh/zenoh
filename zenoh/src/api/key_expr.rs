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
    sync::Arc,
};

use zenoh_core::{Resolvable, Wait};
use zenoh_keyexpr::{keyexpr, OwnedKeyExpr};
use zenoh_protocol::{
    core::{key_expr::canon::Canonize, ExprId, WireExpr},
    network::Mapping,
};
use zenoh_result::ZResult;

use crate::api::session::{Session, UndeclarableSealed, WeakSession};

#[derive(Debug)]
pub(crate) struct KeyExprWireDeclaration {
    expr_id: ExprId,
    prefix_len: u32,
    session: WeakSession,
    undeclared: bool,
}

impl KeyExprWireDeclaration {
    pub(crate) fn new(prefix: &str, session: &Session, force: bool) -> ZResult<Option<Self>> {
        let prefix_len = prefix.len() as u32;
        Ok(session
            .declare_prefix(prefix, force)
            .wait()?
            .map(|expr_id| Self {
                expr_id,
                prefix_len,
                session: session.downgrade(),
                undeclared: false,
            }))
    }

    pub(crate) fn undeclare(&mut self) -> ZResult<()> {
        if self.undeclared {
            Ok(())
        } else {
            self.undeclared = true;
            self.session.undeclare_prefix(self.expr_id)
        }
    }
}

impl Drop for KeyExprWireDeclaration {
    fn drop(&mut self) {
        let _ = self.undeclare();
    }
}

#[derive(Clone, Debug)]
pub(crate) enum KeyExprInner<'a> {
    Borrowed {
        key_expr: &'a keyexpr,
        declaration: Option<Arc<KeyExprWireDeclaration>>,
    },
    Owned {
        key_expr: OwnedKeyExpr,
        declaration: Option<Arc<KeyExprWireDeclaration>>,
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

// Implement RefUnwindSafe for compatibility purposes.
// Given that all key expression public methods are immutable, they
// do not break any key expression invariants.
impl<'a> std::panic::RefUnwindSafe for KeyExpr<'a> {}
impl<'a> std::panic::UnwindSafe for KeyExpr<'a> {}

impl std::ops::Deref for KeyExpr<'_> {
    type Target = keyexpr;
    fn deref(&self) -> &Self::Target {
        match &self.0 {
            KeyExprInner::Borrowed { key_expr, .. } => key_expr,
            KeyExprInner::Owned { key_expr, .. } => key_expr,
        }
    }
}

impl KeyExpr<'static> {
    /// Constructs a [`KeyExpr`] without checking [`keyexpr`]'s invariants
    /// # Safety
    /// Key Expressions must follow some rules to be accepted by a Zenoh network.
    /// Messages addressed with invalid key expressions will be dropped.
    pub unsafe fn from_string_unchecked(s: String) -> Self {
        Self(KeyExprInner::Owned {
            key_expr: OwnedKeyExpr::from_string_unchecked(s),
            declaration: None,
        })
    }

    /// Constructs a [`KeyExpr`] without checking [`keyexpr`]'s invariants
    /// # Safety
    /// Key Expressions must follow some rules to be accepted by a Zenoh network.
    /// Messages addressed with invalid key expressions will be dropped.
    pub unsafe fn from_boxed_str_unchecked(s: Box<str>) -> Self {
        Self(KeyExprInner::Owned {
            key_expr: OwnedKeyExpr::from_boxed_str_unchecked(s),
            declaration: None,
        })
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
        Self(KeyExprInner::Borrowed {
            key_expr: KEYEXPR_DUMMY,
            declaration: None,
        })
    }

    /// Checks if the key expression is the dummy one.
    /// This method is not supposed to be called in user code,
    /// but may be used in language bindings (zenoh-c)
    #[zenoh_macros::internal]
    pub fn is_dummy(&self) -> bool {
        let Self(inner) = self;
        let KeyExprInner::Borrowed { key_expr, .. } = inner else {
            return false;
        };
        std::ptr::eq(*key_expr, KEYEXPR_DUMMY)
    }

    /// Constructs a new [`KeyExpr`] aliasing `self`.
    ///
    /// Note that [`KeyExpr`] (as well as [`OwnedKeyExpr`]) use reference counters internally, so you're probably better off using clone.
    pub fn borrowing_clone(&'a self) -> Self {
        let inner = match &self.0 {
            KeyExprInner::Borrowed {
                key_expr,
                declaration,
            } => KeyExprInner::Borrowed {
                key_expr,
                declaration: declaration.clone(),
            },
            KeyExprInner::Owned {
                key_expr,
                declaration,
            } => KeyExprInner::Borrowed {
                key_expr,
                declaration: declaration.clone(),
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
        let inner = match self.0 {
            KeyExprInner::Borrowed {
                key_expr,
                declaration,
            } => KeyExprInner::Owned {
                key_expr: key_expr.into(),
                declaration: declaration.clone(),
            },
            KeyExprInner::Owned {
                key_expr,
                declaration,
            } => KeyExprInner::Owned {
                key_expr,
                declaration,
            },
        };
        KeyExpr(inner)
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
        Ok(KeyExpr(KeyExprInner::Owned {
            key_expr: self.as_keyexpr().join(s)?,
            declaration: self.declaration().clone(),
        }))
    }

    /// Performs string concatenation and returns the result as a [`KeyExpr`] if possible.
    ///
    /// You should probably prefer [`KeyExpr::join`] as Zenoh may then take advantage of the hierarchical separation it inserts.
    pub fn concat<S: AsRef<str> + ?Sized>(&self, s: &S) -> ZResult<KeyExpr<'static>> {
        let s = s.as_ref();
        if self.ends_with('*') && s.starts_with('*') {
            bail!("Tried to concatenate {} (ends with *) and {} (starts with *), which would likely have caused bugs. If you're sure you want to do this, concatenate these into a string and then try to convert.", self, s)
        }
        Ok(KeyExpr(KeyExprInner::Owned {
            key_expr: OwnedKeyExpr::try_from(format!("{self}{s}"))?,
            declaration: self.declaration().clone(),
        }))
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
        Ok(Self(KeyExprInner::Owned {
            key_expr: s.parse()?,
            declaration: None,
        }))
    }
}

impl<'a> From<KeyExpr<'a>> for OwnedKeyExpr {
    fn from(val: KeyExpr<'a>) -> Self {
        match val.0 {
            KeyExprInner::Borrowed { key_expr, .. } => key_expr.into(),
            KeyExprInner::Owned { key_expr, .. } => key_expr,
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
    fn from(key_expr: &'a keyexpr) -> Self {
        Self(KeyExprInner::Borrowed {
            key_expr,
            declaration: None,
        })
    }
}
impl From<OwnedKeyExpr> for KeyExpr<'_> {
    fn from(key_expr: OwnedKeyExpr) -> Self {
        Self(KeyExprInner::Owned {
            key_expr,
            declaration: None,
        })
    }
}
impl<'a> From<&'a OwnedKeyExpr> for KeyExpr<'a> {
    fn from(key_expr: &'a OwnedKeyExpr) -> Self {
        Self(KeyExprInner::Borrowed {
            key_expr,
            declaration: None,
        })
    }
}
impl<'a> From<&'a KeyExpr<'a>> for KeyExpr<'a> {
    fn from(val: &'a KeyExpr<'a>) -> Self {
        Self(KeyExprInner::Borrowed {
            key_expr: val.key_expr(),
            declaration: val.declaration().clone(),
        })
    }
}
impl From<KeyExpr<'_>> for String {
    fn from(ke: KeyExpr) -> Self {
        match ke.0 {
            KeyExprInner::Borrowed { key_expr, .. } => key_expr.as_str().to_owned(),
            KeyExprInner::Owned { key_expr, .. } => key_expr.into(),
        }
    }
}

impl TryFrom<String> for KeyExpr<'_> {
    type Error = zenoh_result::Error;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Ok(Self(KeyExprInner::Owned {
            key_expr: value.try_into()?,
            declaration: None,
        }))
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
        Ok(Self(KeyExprInner::Borrowed {
            key_expr: value.try_into()?,
            declaration: None,
        }))
    }
}
impl<'a> TryFrom<&'a mut str> for KeyExpr<'a> {
    type Error = zenoh_result::Error;
    fn try_from(value: &'a mut str) -> Result<Self, Self::Error> {
        Ok(Self(KeyExprInner::Borrowed {
            key_expr: value.try_into()?,
            declaration: None,
        }))
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
        KeyExpr(KeyExprInner::Owned {
            key_expr: self.key_expr() / rhs,
            declaration: self.into_declaration(),
        })
    }
}
impl std::ops::Div<&keyexpr> for &KeyExpr<'_> {
    type Output = KeyExpr<'static>;

    fn div(self, rhs: &keyexpr) -> Self::Output {
        KeyExpr(KeyExprInner::Owned {
            key_expr: self.key_expr() / rhs,
            declaration: self.declaration().clone(),
        })
    }
}

impl<'a> KeyExpr<'a> {
    fn declaration(&self) -> &Option<Arc<KeyExprWireDeclaration>> {
        match &self.0 {
            KeyExprInner::Borrowed { declaration, .. } => declaration,
            KeyExprInner::Owned { declaration, .. } => declaration,
        }
    }

    fn declaration_mut(&mut self) -> &mut Option<Arc<KeyExprWireDeclaration>> {
        match &mut self.0 {
            KeyExprInner::Borrowed { declaration, .. } => declaration,
            KeyExprInner::Owned { declaration, .. } => declaration,
        }
    }

    pub(crate) fn declare(mut self, session: &Session, force: bool) -> ZResult<Self> {
        if !self.is_fully_optimized(session) {
            *self.declaration_mut() =
                KeyExprWireDeclaration::new(self.as_str(), session, force)?.map(Arc::new);
        }
        Ok(self)
    }

    pub(crate) fn declare_nonwild_prefix(
        mut self,
        session: &Session,
        force: bool,
    ) -> ZResult<Self> {
        if !self.is_non_wild_prefix_optimized(session) {
            let ke = self.as_keyexpr();
            *self.declaration_mut() = match ke.get_nonwild_prefix() {
                Some(prefix) => {
                    KeyExprWireDeclaration::new(prefix.as_str(), session, force)?.map(Arc::new)
                }
                None => None,
            }
        }
        Ok(self)
    }

    fn into_declaration(self) -> Option<Arc<KeyExprWireDeclaration>> {
        match self.0 {
            KeyExprInner::Borrowed { declaration, .. } => declaration,
            KeyExprInner::Owned { declaration, .. } => declaration,
        }
    }

    pub(crate) fn key_expr(&self) -> &keyexpr {
        match &self.0 {
            KeyExprInner::Borrowed { key_expr, .. } => key_expr,
            KeyExprInner::Owned { key_expr, .. } => key_expr,
        }
    }

    pub(crate) fn is_fully_optimized(&self, session: &Session) -> bool {
        self.declaration()
            .as_ref()
            .map(|d| d.session == *session && d.prefix_len as usize == self.key_expr().len())
            .unwrap_or(false)
    }

    pub(crate) fn is_non_wild_prefix_optimized(&self, session: &Session) -> bool {
        self.declaration()
            .as_ref()
            .map(|d| {
                d.session == *session
                    && self
                        .key_expr()
                        .get_nonwild_prefix()
                        .map(|p| p.len() == d.prefix_len as usize)
                        .unwrap_or(d.prefix_len == 0)
            })
            .unwrap_or(false)
    }

    fn to_wire_inner(&'a self, session: &Session, mapping: Mapping) -> WireExpr<'a> {
        match self.declaration() {
            Some(d) if d.session == *session => WireExpr {
                scope: d.expr_id,
                suffix: std::borrow::Cow::Borrowed(
                    &self.key_expr().as_str()[(d.prefix_len as usize)..],
                ),
                mapping,
            },
            _ => WireExpr {
                scope: 0,
                suffix: std::borrow::Cow::Borrowed(self.key_expr().as_str()),
                mapping,
            },
        }
    }

    pub(crate) fn to_wire(&'a self, session: &Session) -> WireExpr<'a> {
        self.to_wire_inner(session, Mapping::Sender)
    }

    pub(crate) fn to_wire_local(&'a self, session: &Session) -> WireExpr<'a> {
        self.to_wire_inner(session, Mapping::Receiver)
    }

    fn undeclare_with_session_check(&mut self, parent_session: Option<&Session>) -> ZResult<()> {
        match self.declaration_mut().take() {
            Some(mut d) if self.key_expr().len() == d.prefix_len as usize => {
                if parent_session.is_some_and(|s| d.session == *s) {
                    Arc::get_mut(&mut d).map(|d| d.undeclare()).unwrap_or(Ok(()))
                } else {
                    let expr_id = self.declaration_mut().insert(d).expr_id;
                        Err(zerror!(
                            "Failed to undeclare expr with id {}, as it was declared by another Session",
                            expr_id
                        )
                        .into())
                }
            },
            _ => Err(zerror!("Failed to undeclare {}, make sure you use the result of `Session::declare_keyexpr` to call `Session::undeclare`, and that key_expression was not undeclared previously", self).into()),
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
        let KeyExprUndeclaration { session, mut expr } = self;
        expr.undeclare_with_session_check(Some(session))
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
