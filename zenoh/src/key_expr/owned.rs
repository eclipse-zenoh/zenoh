use super::{canon::Canonizable, keyexpr};
use std::convert::{TryFrom, TryInto};
use zenoh_core::Result as ZResult;

#[derive(Clone)]
pub(crate) enum KeyExprInner<'a> {
    Borrowed(&'a keyexpr),
    Owned(String),
}

#[repr(transparent)]
#[derive(Clone)]
pub struct KeyExpr<'a>(KeyExprInner<'a>);
impl std::ops::Deref for KeyExpr<'_> {
    type Target = keyexpr;
    fn deref(&self) -> &Self::Target {
        match &self.0 {
            KeyExprInner::Borrowed(s) => *s,
            KeyExprInner::Owned(s) => unsafe { keyexpr::from_str_unchecked(s) },
        }
    }
}
impl AsRef<keyexpr> for KeyExpr<'_> {
    fn as_ref(&self) -> &keyexpr {
        self
    }
}
impl<'a> From<&'a keyexpr> for KeyExpr<'a> {
    fn from(ke: &'a keyexpr) -> Self {
        Self(KeyExprInner::Borrowed(ke))
    }
}
impl TryFrom<String> for KeyExpr<'static> {
    type Error = zenoh_core::Error;
    fn try_from(mut value: String) -> Result<Self, Self::Error> {
        value.canonize();
        <&keyexpr as TryFrom<&str>>::try_from(value.as_str())?;
        Ok(Self(KeyExprInner::Owned(value)))
    }
}
impl<'a> TryFrom<&'a str> for KeyExpr<'a> {
    type Error = zenoh_core::Error;
    fn try_from(value: &'a str) -> Result<Self, Self::Error> {
        Ok(Self(KeyExprInner::Borrowed(value.try_into()?)))
    }
}
impl<'a> TryFrom<&'a mut str> for KeyExpr<'a> {
    type Error = zenoh_core::Error;
    fn try_from(value: &'a mut str) -> Result<Self, Self::Error> {
        Ok(Self(KeyExprInner::Borrowed(value.try_into()?)))
    }
}
impl std::fmt::Debug for KeyExpr<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self.keyexpr(), f)
    }
}
impl std::fmt::Display for KeyExpr<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self.keyexpr(), f)
    }
}
impl PartialEq for KeyExpr<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.keyexpr() == other.keyexpr()
    }
}
impl Eq for KeyExpr<'_> {}
impl std::hash::Hash for KeyExpr<'_> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.keyexpr().hash(state);
    }
}

impl<'a> KeyExpr<'a> {
    pub fn keyexpr(&self) -> &keyexpr {
        self
    }
    pub fn into_owned(self) -> KeyExpr<'static> {
        match self.0 {
            KeyExprInner::Borrowed(s) => KeyExpr(KeyExprInner::Owned(s.as_ref().to_owned())),
            KeyExprInner::Owned(s) => KeyExpr(KeyExprInner::Owned(s)),
        }
    }
    pub fn concat<S: AsRef<str>>(&self, s: &S) -> ZResult<KeyExpr<'static>> {
        let s = s.as_ref();
        if self.ends_with('*') && s.starts_with('*') {
            bail!("Tried to concatenate {} (ends with *) and {} (starts with *), which would likely have caused bugs. If you're sure you want to do this, concatenate these into a string and then try to convert.", self, s)
        }
        format!("{}{}", self, s).try_into()
    }
    pub fn join<S: AsRef<str>>(&self, s: &S) -> ZResult<KeyExpr<'static>> {
        format!("{}/{}", self, s.as_ref()).try_into()
    }
}
