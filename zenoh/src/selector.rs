use zenoh_protocol_core::key_expr::{keyexpr, OwnedKeyExpr};
use zenoh_util::time_range::TimeRange;

use crate::{prelude::KeyExpr, queryable::Query};

use std::{
    borrow::{Borrow, Cow},
    collections::HashMap,
    convert::TryFrom,
    hash::Hash,
};

/// A selector is the combination of a [Key Expression](crate::prelude::KeyExpr), which defines the
/// set of keys that are relevant to an operation, and a `value_selector`, a set of key-value pairs
/// with a few uses:
/// - specifying arguments to a queryable, allowing the passing of Remote Procedure Call parameters
/// - filtering by value,
/// - filtering by metadata, such as the timestamp of a value,
///
/// When in string form, selectors look a lot like a URI, with similar semantics:
/// - the `key_expr` before the first `?` must be a valid key expression.
/// - the `selector` after the first `?` should be encoded like the query section of a URL:
///     - key-value pairs are separated by `&`,
///     - the key and value are separated by the first `=`,
///     - in the absence of `=`, the value is considered to be the empty string,
///     - both key and value should use percent-encoding to escape characters,
///     - defining a value for the same key twice is considered undefined behavior.
///
/// Zenoh intends to standardize the usage of a set of keys. To avoid conflicting with RPC parameters,
/// the Zenoh team has settled on reserving the set of keys that start with non-alphanumeric characters.
///
/// This document will summarize the standardized keys for which Zenoh provides helpers to facilitate
/// coherent behavior for some operations.
///
/// Queryable implementers are encouraged to prefer these standardized keys when implementing their
/// associated features, and to prefix their own keys to avoid having conflicting keys with other
/// queryables.
///
/// Here are the currently standardized keys for Zenoh:
/// - `_time`: used to express interest in only values dated within a certain time range, values for
///   this key must be readable by the [Zenoh Time DSL](zenoh_util::time_range::TimeRange) for the value to be considered valid.
/// - `_filter`: *TBD* Zenoh intends to provide helper tools to allow the value associated with
///   this key to be treated as a predicate that the value should fulfill before being returned.
///   A DSL will be designed by the Zenoh team to express these predicates.
#[non_exhaustive]
#[derive(Clone, PartialEq)]
pub struct Selector<'a> {
    /// The part of this selector identifying which keys should be part of the selection.
    pub key_expr: KeyExpr<'a>,
    /// the part of this selector identifying which values should be part of the selection.
    pub(crate) value_selector: Cow<'a, str>,
}

pub const TIME_RANGE_KEY: &str = "_time";
impl<'a> Selector<'a> {
    /// Gets the value selector as a raw string.
    pub fn value_selector(&self) -> &str {
        &self.value_selector
    }
    /// Gets a mutable reference to the value_selector as a String.
    ///
    /// Note that calling this function may cause an allocation and copy if the value selector wasn't
    /// already owned by `self`. `self` owns its value selector as soon as this function returns.
    pub fn value_selector_mut(&mut self) -> &mut String {
        if let Cow::Borrowed(s) = self.value_selector {
            self.value_selector = Cow::Owned(s.to_owned())
        }
        if let Cow::Owned(s) = &mut self.value_selector {
            s
        } else {
            unsafe { std::hint::unreachable_unchecked() } // this is safe because we just replaced the borrowed variant
        }
    }
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

    pub fn extend<'b, I, K, V>(&'b mut self, key_value_pairs: I)
    where
        I: IntoIterator,
        I::Item: std::borrow::Borrow<(K, V)>,
        K: AsRef<str> + 'b,
        V: AsRef<str> + 'b,
    {
        let it = key_value_pairs.into_iter();
        let selector = self.value_selector_mut();
        let mut encoder = form_urlencoded::Serializer::new(selector);
        encoder.extend_pairs(it).finish();
    }

    /// Sets the time range targeted by the selector.
    pub fn with_time_range(&mut self, time_range: TimeRange) {
        self.remove_time_range();
        let selector = self.value_selector_mut();
        if !selector.is_empty() {
            selector.push('&')
        }
        use std::fmt::Write;
        write!(selector, "{}={}", TIME_RANGE_KEY, time_range).unwrap(); // This unwrap is safe because `String: Write` should be infallibe.
    }

    pub fn remove_time_range(&mut self) {
        let selector = self.value_selector_mut();

        let mut splice_start = 0;
        let mut splice_end = 0;
        for argument in selector.split('&') {
            if argument.starts_with(TIME_RANGE_KEY)
                && matches!(
                    argument.as_bytes().get(TIME_RANGE_KEY.len()),
                    None | Some(b'=')
                )
            {
                splice_end = splice_start + argument.len();
                break;
            }
            splice_start += argument.len() + 1
        }
        if splice_end > 0 {
            selector.drain(splice_start..(splice_end + (splice_end != selector.len()) as usize));
        }
    }
}

#[test]
fn selector_accessors() {
    let time_range = "[now(-2s)..now(2s)]".parse().unwrap();
    for selector in [
        "hello/there?_timetrick",
        "hello/there?_timetrick&_time",
        "hello/there?_timetrick&_time&_filter",
        "hello/there?_timetrick&_time=[..]",
        "hello/there?_timetrick&_time=[..]&_filter",
    ] {
        let mut selector = Selector::try_from(selector).unwrap();
        selector.with_time_range(time_range);
        assert_eq!(selector.time_range().unwrap(), time_range);
        assert!(dbg!(selector.value_selector()).contains("_time=[now(-2s)..now(2s)]"));
        let map_selector = selector.decode().collect::<HashMap<_, _>>();
        assert_eq!(selector.time_range(), map_selector.time_range());
    }
}
pub trait ValueSelectorProperty {
    fn key(&self) -> &str;
    fn value(&self) -> &str;
}
impl ValueSelectorProperty for (&str, &str) {
    fn key(&self) -> &str {
        self.0
    }
    fn value(&self) -> &str {
        self.1
    }
}
impl ValueSelectorProperty for (Cow<'_, str>, Cow<'_, str>) {
    fn key(&self) -> &str {
        self.0.borrow()
    }
    fn value(&self) -> &str {
        self.1.borrow()
    }
}
impl ValueSelectorProperty for (&Cow<'_, str>, &Cow<'_, str>) {
    fn key(&self) -> &str {
        self.0.borrow()
    }
    fn value(&self) -> &str {
        self.1.borrow()
    }
}
/// A trait to help decode zenoh value selectors as properties.
pub trait ValueSelector<'a> {
    type Decoder: Iterator + 'a;

    /// Returns this value selector as properties.
    fn decode(&'a self) -> Self::Decoder
    where
        <Self::Decoder as Iterator>::Item: ValueSelectorProperty;
    ///
    fn time_range(&'a self) -> Option<TimeRange>
    where
        <Self::Decoder as Iterator>::Item: ValueSelectorProperty,
    {
        self.decode().find_map(|prop| {
            if prop.key() == TIME_RANGE_KEY {
                prop.value().parse().ok()
            } else {
                None
            }
        })
    }
}
impl<'a> ValueSelector<'a> for Selector<'a> {
    type Decoder = <str as ValueSelector<'a>>::Decoder;
    fn decode(&'a self) -> Self::Decoder {
        self.value_selector().decode()
    }
}
impl<'a> ValueSelector<'a> for str {
    type Decoder = form_urlencoded::Parse<'a>;
    fn decode(&'a self) -> Self::Decoder {
        form_urlencoded::parse(self.as_bytes())
    }
}

impl<'a, K: Borrow<str> + Hash + Eq + 'a, V: Borrow<str> + 'a> ValueSelector<'a> for HashMap<K, V> {
    type Decoder = std::collections::hash_map::Iter<'a, K, V>;
    fn decode(&'a self) -> Self::Decoder {
        self.iter()
    }
    fn time_range(&'a self) -> Option<TimeRange>
    where
        <Self::Decoder as Iterator>::Item: ValueSelectorProperty,
    {
        self.get::<str>(TIME_RANGE_KEY)
            .and_then(|v| v.borrow().parse().ok())
    }
}

impl std::fmt::Debug for Selector<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "sel\"{}\"", self)
    }
}

impl std::fmt::Display for Selector<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}?{}", self.key_expr, self.value_selector)
    }
}

impl<'a> From<&Selector<'a>> for Selector<'a> {
    fn from(s: &Selector<'a>) -> Self {
        s.clone()
    }
}

impl TryFrom<String> for Selector<'_> {
    type Error = zenoh_core::Error;
    fn try_from(mut s: String) -> Result<Self, Self::Error> {
        match s.find('?') {
            Some(qmark_position) => {
                let value_selector = s[qmark_position + 1..].to_owned();
                s.truncate(qmark_position);
                Ok(KeyExpr::try_from(s)?.with_owned_value_selector(value_selector))
            }
            None => Ok(KeyExpr::try_from(s)?.into()),
        }
    }
}

impl<'a> TryFrom<&'a str> for Selector<'a> {
    type Error = zenoh_core::Error;
    fn try_from(s: &'a str) -> Result<Self, Self::Error> {
        match s.find('?') {
            Some(qmark_position) => {
                let value_selector = &s[qmark_position + 1..];
                Ok(KeyExpr::try_from(&s[..qmark_position])?.with_value_selector(value_selector))
            }
            None => Ok(KeyExpr::try_from(s)?.into()),
        }
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
