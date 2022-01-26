use std::{borrow::Borrow, convert::TryFrom, ops::Deref, str::FromStr};

use zenoh_core::bail;

// Parsing chars
pub const PROTO_SEPARATOR: char = '/';
pub const METADATA_SEPARATOR: char = '?';
pub const CONFIG_SEPARATOR: char = '#';
pub const LIST_SEPARATOR: char = ';';
pub const FIELD_SEPARATOR: char = '=';

/// A `String` that respects the [`Locator`] canon form: `<proto>/<address>?<metadata>`, such that `<metadata>` is of the form `<key1>=<value1>;...;<keyN>=<valueN>` where keys are alphabetically sorted.
#[repr(transparent)]
#[derive(Clone, Debug, Eq, PartialEq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(into = "String")]
#[serde(try_from = "String")]
pub struct Locator {
    pub(crate) inner: String,
}
impl From<Locator> for String {
    fn from(val: Locator) -> Self {
        val.inner
    }
}
/// A `str` that respects the [`Locator`] canon form: `<proto>/<address>?<metadata>`, such that `<metadata>` is of the form `<key1>=<value1>;...;<keyN>=<valueN>` where keys are alphabetically sorted.
#[allow(non_camel_case_types)]
#[repr(transparent)]
#[derive(Debug, PartialEq, Eq, Hash, serde::Serialize)]
#[serde(transparent)]
pub struct locator {
    pub(crate) inner: str,
}

impl core::fmt::Display for locator {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.inner)
    }
}

impl core::fmt::Display for Locator {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.inner)
    }
}
impl TryFrom<String> for Locator {
    type Error = zenoh_core::Error;
    fn try_from(inner: String) -> Result<Self, Self::Error> {
        if locator::new(&inner).is_some() {
            Ok(Locator { inner })
        } else {
            inner.parse()
        }
    }
}
impl FromStr for Locator {
    type Err = zenoh_core::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let lr = unsafe { locator::new_unchecked(s) };
        let (proto, addr, props) = lr.split();
        if proto.is_empty() {
            bail!(
                "{} doesn't respect the Locator format (empty protocol)",
                proto
            )
        }
        if addr.is_empty() {
            bail!(
                "{} doesn't respect the Locator format (empty protocol)",
                addr
            )
        }
        if props.is_canon() {
            Ok(lr.to_owned())
        } else {
            let mut inner = format!("{}/{}?", proto, addr);
            let canon = props.canonicalize();
            extend_with_props(&mut inner, &canon);
            Ok(Locator { inner })
        }
    }
}

impl Locator {
    #[inline(always)]
    pub fn new<Addr: std::fmt::Display>(protocol: &str, addr: &Addr) -> Self {
        Locator::try_from(format!("{}{}{}", protocol, PROTO_SEPARATOR, addr)).unwrap()
    }
    #[must_use = "returns true if successful"]
    pub fn set_addr(&mut self, addr: &str) -> bool {
        if addr.contains(&[METADATA_SEPARATOR, CONFIG_SEPARATOR]) {
            return false;
        }
        let addr_start = self.inner.find(PROTO_SEPARATOR).unwrap() + 1;
        let addr_end = self.inner.find(METADATA_SEPARATOR).unwrap();
        self.inner.replace_range(addr_start..addr_end, addr);
        true
    }
    /// # Safety
    /// This function doesn't check whether the metadata is sorted, which may lead to issues
    pub unsafe fn with_metadata_str_unchecked(&mut self, meta: &str) {
        self.inner.push(METADATA_SEPARATOR);
        let meta_start = self.inner.find(METADATA_SEPARATOR).unwrap() + 1;
        self.inner.replace_range(meta_start.., meta);
    }
}

impl AsRef<str> for locator {
    fn as_ref(&self) -> &str {
        &self.inner
    }
}
impl locator {
    /// # Safety
    /// This function doesn't check whether `s` is a valid locator or not.
    ///
    /// Use [`LocatorRef::new`] instead
    pub unsafe fn new_unchecked(s: &str) -> &Self {
        std::mem::transmute(s)
    }
    /// # Safety
    /// This function doesn't check whether `s` is a valid locator or not.
    ///
    /// Use [`LocatorRef::new`] instead
    pub unsafe fn new_unchecked_mut(s: &mut str) -> &mut Self {
        std::mem::transmute(s)
    }
    pub fn new(s: &str) -> Option<&Self> {
        let r = unsafe { locator::new_unchecked(s) };
        let (proto, addr, props) = r.split();
        let invalid = proto.is_empty() || addr.is_empty() || !props.is_canon();
        std::mem::drop(props);
        match invalid {
            true => None,
            false => Some(r),
        }
    }
    pub fn split(
        &self,
    ) -> (
        &str,
        &str,
        impl Iterator<Item = (&str, &str)> + DoubleEndedIterator + Clone,
    ) {
        let (protocol, rest) = split_once(&self.inner, PROTO_SEPARATOR);
        let (address, properties) = split_once(rest, METADATA_SEPARATOR);
        (
            protocol,
            address,
            properties
                .split(LIST_SEPARATOR)
                .map(|prop| split_once(prop, FIELD_SEPARATOR)),
        )
    }
    pub fn protocol(&self) -> &str {
        let index = self
            .inner
            .find(PROTO_SEPARATOR)
            .unwrap_or_else(|| self.inner.len());
        &self.inner[..index]
    }
    pub fn address(&self) -> &str {
        let index = self
            .inner
            .find(PROTO_SEPARATOR)
            .unwrap_or_else(|| self.inner.len());
        let rest = &self.inner[index + 1..];
        let index = rest
            .find(METADATA_SEPARATOR)
            .unwrap_or_else(|| self.inner.len());
        &rest[..index]
    }
    pub fn without_metadata(&self) -> &Self {
        let index = self
            .inner
            .find(METADATA_SEPARATOR)
            .unwrap_or_else(|| self.inner.len());
        unsafe { Self::new_unchecked(&self.as_ref()[..index]) }
    }
    pub fn metadata_str(&self) -> &str {
        let index = self
            .inner
            .find(METADATA_SEPARATOR)
            .unwrap_or_else(|| self.inner.len());
        &self.as_ref()[index + 1..]
    }
    pub fn metadata(&self) -> impl Iterator<Item = (&str, &str)> + DoubleEndedIterator + Clone {
        self.metadata_str()
            .split(LIST_SEPARATOR)
            .map(|prop| split_once(prop, FIELD_SEPARATOR))
    }
}
impl AsRef<locator> for Locator {
    fn as_ref(&self) -> &locator {
        &*self
    }
}
impl Deref for Locator {
    type Target = locator;
    fn deref(&self) -> &Self::Target {
        unsafe { locator::new_unchecked(&self.inner) }
    }
}
impl Borrow<locator> for Locator {
    fn borrow(&self) -> &locator {
        &*self
    }
}
impl ToOwned for locator {
    type Owned = Locator;
    fn to_owned(&self) -> Self::Owned {
        Locator {
            inner: self.inner.to_owned(),
        }
    }
}

pub(crate) fn split_once(s: &str, c: char) -> (&str, &str) {
    match s.find(c) {
        Some(index) => {
            let (l, r) = s.split_at(index);
            (l, &r[1..])
        }
        None => (s, ""),
    }
}

pub(crate) fn split_once_mut(s: &mut str, c: char) -> (&mut str, &mut str) {
    match s.find(c) {
        Some(index) => {
            let (l, r) = s.split_at_mut(index);
            (l, &mut r[1..])
        }
        None => (s, unsafe { std::str::from_utf8_unchecked_mut(&mut []) }),
    }
}

pub(crate) trait HasCanonForm {
    fn is_canon(&self) -> bool;
    type Output;
    fn canonicalize(self) -> Self::Output;
}
fn cmp(this: &str, than: &str) -> std::cmp::Ordering {
    let is_longer = this.len().cmp(&than.len());
    let this = this.chars();
    let than = than.chars();
    let zip = this.zip(than);
    for (this, than) in zip {
        match this.cmp(&than) {
            std::cmp::Ordering::Equal => {}
            o => return o,
        }
    }
    is_longer
}
impl<'a, T: Iterator<Item = (&'a str, V)> + Clone, V> HasCanonForm for T {
    fn is_canon(&self) -> bool {
        self.clone()
            .try_fold("", |acc, (key, _)| {
                if cmp(key, acc) == std::cmp::Ordering::Greater {
                    Ok(key)
                } else {
                    Err(())
                }
            })
            .is_ok()
    }
    type Output = Vec<(&'a str, V)>;
    fn canonicalize(mut self) -> Self::Output {
        let mut result = Vec::new();
        if let Some(v) = self.next() {
            result.push(v);
        }
        'outer: for (k, v) in self {
            for (i, (x, _)) in result.iter().enumerate() {
                match cmp(k, x) {
                    std::cmp::Ordering::Less => {
                        result.insert(i, (k, v));
                        continue 'outer;
                    }
                    std::cmp::Ordering::Equal => {
                        result[i].1 = v;
                        continue 'outer;
                    }
                    std::cmp::Ordering::Greater => {}
                }
            }
            result.push((k, v))
        }
        result
    }
}

#[test]
fn locators() {
    let locator = Locator::from_str("udp/127.0.0.1?hello=there;general=kenobi").unwrap();
    assert_eq!(
        (&*locator).as_ref(),
        "udp/127.0.0.1?general=kenobi;hello=there"
    );
    assert_eq!(locator.protocol(), "udp");
    assert_eq!(locator.address(), "127.0.0.1");
    assert_eq!(locator.metadata_str(), "general=kenobi;hello=there");
    assert_eq!(
        locator.metadata().collect::<Vec<_>>(),
        vec![("general", "kenobi"), ("hello", "there")]
    );
}

#[test]
fn canon_test() {
    fn it<'a, T: IntoIterator<Item = &'a str>>(
        it: T,
    ) -> std::iter::Zip<T::IntoIter, std::iter::Repeat<()>> {
        it.into_iter().zip(std::iter::repeat(()))
    }
    assert!(it([]).is_canon());
    assert!(it(["a", "b"]).is_canon());
    assert!(!it(["a", "a"]).is_canon());
    assert!(it(["a", "aa"]).is_canon());
    assert!(!it(["aa", "a"]).is_canon());
    assert_eq!(it(["b", "a"]).canonicalize(), vec![("a", ()), ("b", ())]);
    assert_eq!(
        [("a", 1), ("a", 2)].iter().copied().canonicalize(),
        vec![("a", 2)]
    );
}

pub(crate) fn extend_with_props(s: &mut String, props: &[(&str, &str)]) {
    for (k, v) in &props[..props.len() - 1] {
        *s += k;
        s.push(FIELD_SEPARATOR);
        *s += v;
        s.push(LIST_SEPARATOR);
    }
    let (k, v) = if let Some(x) = props.last() {
        x
    } else {
        unsafe { std::hint::unreachable_unchecked() }
    };
    *s += k;
    s.push(FIELD_SEPARATOR);
    *s += v;
}

pub type LocatorProtocol = str;
