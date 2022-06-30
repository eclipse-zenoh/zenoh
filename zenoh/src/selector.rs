use zenoh_protocol_core::key_expr::{keyexpr, OwnedKeyExpr};

use crate::{prelude::KeyExpr, queryable::Query};

use std::{
    borrow::Cow,
    convert::{TryFrom, TryInto},
};

/// A selector is the
#[derive(Clone, Debug, PartialEq)]
pub struct Selector<'a> {
    /// The part of this selector identifying which keys should be part of the selection.
    /// I.e. all characters before `?`.
    pub key_expr: KeyExpr<'a>,
    /// the part of this selector identifying which values should be part of the selection.
    /// I.e. all characters starting from `?`.
    pub(crate) value_selector: Cow<'a, str>,
}

impl<'a> Selector<'a> {
    pub fn borrowing_clone(&'a self) -> Self {
        Selector {
            key_expr: self.key_expr.borrowing_clone(),
            value_selector: self.value_selector.as_ref().into(),
        }
    }
    pub fn into_owned(self) -> Selector<'static> {
        Selector {
            key_expr: self.key_expr.into_owned(),
            value_selector: self.value_selector.into_owned().into(),
        }
    }

    #[deprecated = "If you have ownership of this selector, prefer `Selector::into_owned`"]
    pub fn to_owned(&self) -> Selector<'static> {
        self.borrowing_clone().into_owned()
    }

    /// Returns this selectors components as a tuple.
    pub fn split(self) -> (KeyExpr<'a>, Cow<'a, str>) {
        (self.key_expr, self.value_selector)
    }

    /// Sets the `value_selector` part of this `Selector`.
    #[inline(always)]
    pub fn with_value_selector(mut self, value_selector: &'a str) -> Self {
        self.value_selector = value_selector.into();
        self
    }

    /// Gets the value selector as a borrowed string.
    pub fn value_selector(&self) -> &str {
        &self.value_selector
    }

    /// Returns the value selector as an iterator of key-value pairs, where any urlencoding has been decoded.
    pub fn decode_value_selector(
        &'a self,
    ) -> impl Iterator<Item = (Cow<'a, str>, Cow<'a, str>)> + Clone + 'a {
        self.value_selector().decode()
    }

    pub fn extend<'b, I, K, V>(&'b mut self, key_value_pairs: I)
    where
        I: IntoIterator,
        I::Item: std::borrow::Borrow<(K, V)>,
        K: AsRef<str> + 'b,
        V: AsRef<str> + 'b,
    {
        let it = key_value_pairs.into_iter();
        if let Cow::Borrowed(s) = self.value_selector {
            self.value_selector = Cow::Owned(s.to_owned())
        }
        let selector = if let Cow::Owned(s) = &mut self.value_selector {
            s
        } else {
            unsafe { std::hint::unreachable_unchecked() } // this is safe because we just replaced the borrowed variant
        };
        let mut encoder = form_urlencoded::Serializer::new(selector);
        encoder.extend_pairs(it).finish();
    }
}
pub trait ValueSelector<'a> {
    type Decoder: Iterator<Item = (Cow<'a, str>, Cow<'a, str>)> + Clone + 'a;
    fn decode(&'a self) -> Self::Decoder;
}
impl<'a> ValueSelector<'a> for str {
    type Decoder = form_urlencoded::Parse<'a>;
    fn decode(&'a self) -> Self::Decoder {
        form_urlencoded::parse(self.as_bytes())
    }
}

impl std::fmt::Display for Selector<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}{}", self.key_expr, self.value_selector)
    }
}

impl<'a> From<&Selector<'a>> for Selector<'a> {
    fn from(s: &Selector<'a>) -> Self {
        s.clone()
    }
}

impl TryFrom<String> for Selector<'_> {
    type Error = zenoh_core::Error;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        let (key_selector, value_selector) = s
            .find(|c| c == '?')
            .map_or((s.as_str(), ""), |i| s.split_at(i));
        Ok(Selector {
            key_expr: key_selector.to_string().try_into()?,
            value_selector: value_selector.to_string().into(),
        })
    }
}

impl<'a> TryFrom<&'a str> for Selector<'a> {
    type Error = zenoh_core::Error;
    fn try_from(s: &'a str) -> Result<Self, Self::Error> {
        let (key_selector, value_selector) =
            s.find(|c| c == '?').map_or((s, ""), |i| s.split_at(i));
        Ok(Selector {
            key_expr: key_selector.try_into()?,
            value_selector: value_selector.into(),
        })
    }
}

impl<'a> TryFrom<&'a String> for Selector<'a> {
    type Error = zenoh_core::Error;
    fn try_from(s: &'a String) -> Result<Self, Self::Error> {
        Self::try_from(s.as_str())
    }
}

impl<'a> From<&'a Query> for Selector<'a> {
    fn from(q: &'a Query) -> Self {
        Selector {
            key_expr: q.key_expr.borrowing_clone(),
            value_selector: (&q.value_selector).into(),
        }
    }
}

impl<'a> From<&KeyExpr<'a>> for Selector<'a> {
    fn from(key_selector: &KeyExpr<'a>) -> Self {
        Self {
            key_expr: key_selector.clone(),
            value_selector: "".into(),
        }
    }
}

impl<'a> From<&'a keyexpr> for Selector<'a> {
    fn from(key_selector: &'a keyexpr) -> Self {
        Self {
            key_expr: key_selector.into(),
            value_selector: "".into(),
        }
    }
}

impl<'a> From<&'a OwnedKeyExpr> for Selector<'a> {
    fn from(key_selector: &'a OwnedKeyExpr) -> Self {
        Self {
            key_expr: key_selector.into(),
            value_selector: "".into(),
        }
    }
}

impl From<OwnedKeyExpr> for Selector<'static> {
    fn from(key_selector: OwnedKeyExpr) -> Self {
        Self {
            key_expr: key_selector.into(),
            value_selector: "".into(),
        }
    }
}

impl<'a> From<KeyExpr<'a>> for Selector<'a> {
    fn from(key_selector: KeyExpr<'a>) -> Self {
        Self {
            key_expr: key_selector,
            value_selector: "".into(),
        }
    }
}
