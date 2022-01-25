use crate::{
    locator,
    locators::{extend_with_props, split_once, split_once_mut, HasCanonForm},
    Locator,
};

use super::locators::{CONFIG_SEPARATOR, FIELD_SEPARATOR, LIST_SEPARATOR, METADATA_SEPARATOR};

use std::{
    borrow::Borrow,
    convert::TryFrom,
    ops::{Deref, DerefMut},
    str::FromStr,
};

/// A `String` that respects the [`EndPoint`] canon form: `<locator>#<config>`, such that `<locator>` is a valid [`Locator`] `<config>` is of the form `<key1>=<value1>;...;<keyN>=<valueN>` where keys are alphabetically sorted.
#[repr(transparent)]
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct EndPoint {
    inner: String,
}

impl Deref for EndPoint {
    type Target = endpoint;
    fn deref(&self) -> &Self::Target {
        unsafe { endpoint::new_unchecked(&self.inner) }
    }
}
impl DerefMut for EndPoint {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { endpoint::new_unchecked_mut(&mut self.inner) }
    }
}
impl Borrow<endpoint> for EndPoint {
    fn borrow(&self) -> &endpoint {
        &*self
    }
}
impl ToOwned for endpoint {
    type Owned = EndPoint;
    fn to_owned(&self) -> Self::Owned {
        EndPoint {
            inner: self.inner.to_owned(),
        }
    }
}

impl EndPoint {
    #[must_use = "returns true on success"]
    pub fn set_addr(&mut self, addr: &str) -> bool {
        unsafe { std::mem::transmute::<_, &mut Locator>(self) }.set_addr(addr)
    }
}

/// A `str` that respects the [`EndPoint`] canon form: `<locator>#<config>`, such that `<locator>` is a valid [`Locator`] `<config>` is of the form `<key1>=<value1>;...;<keyN>=<valueN>` where keys are alphabetically sorted.
#[allow(non_camel_case_types)]
#[repr(transparent)]
#[derive(Debug, Eq, PartialEq, Hash)]
pub struct endpoint {
    inner: str,
}
impl core::fmt::Display for endpoint {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.inner)
    }
}

impl TryFrom<String> for EndPoint {
    type Error = zenoh_core::Error;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        if endpoint::new(&value).is_none() {
            value.parse()
        } else {
            Ok(EndPoint { inner: value })
        }
    }
}
impl FromStr for EndPoint {
    type Err = zenoh_core::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (l, r) = split_once(s, CONFIG_SEPARATOR);
        let l: Locator = l.parse()?;
        let mut inner = l.into();
        let config = r
            .split(LIST_SEPARATOR)
            .map(|prop| split_once(prop, FIELD_SEPARATOR));
        if config.is_canon() {
            Ok(EndPoint { inner: inner + r })
        } else {
            extend_with_props(&mut inner, &config.canonicalize());
            Ok(EndPoint { inner })
        }
    }
}
impl AsRef<str> for endpoint {
    fn as_ref(&self) -> &str {
        &self.inner
    }
}
impl endpoint {
    /// # Safety
    /// This function doesn't check whether `s` is a valid locator or not.
    ///
    /// Use [`EndPointRef::new`] instead
    pub unsafe fn new_unchecked(s: &str) -> &Self {
        std::mem::transmute(s)
    }
    /// # Safety
    /// This function doesn't check whether `s` is a valid locator or not.
    ///
    /// Use [`EndPointRef::new`] instead
    pub unsafe fn new_unchecked_mut(s: &mut str) -> &mut Self {
        std::mem::transmute(s)
    }
    pub fn new(s: &str) -> Option<&Self> {
        let (l, r) = split_once(s, CONFIG_SEPARATOR);
        let invalid = r.contains(METADATA_SEPARATOR)
            || locator::new(l).is_none()
            || !r
                .split(LIST_SEPARATOR)
                .map(|prop| split_once(prop, FIELD_SEPARATOR))
                .is_canon();
        match invalid {
            true => None,
            false => Some(unsafe { endpoint::new_unchecked(s) }),
        }
    }
    pub fn split(
        &self,
    ) -> (
        &locator,
        impl Iterator<Item = (&str, &str)> + DoubleEndedIterator + Clone,
    ) {
        let (locator, config) = split_once(&self.inner, CONFIG_SEPARATOR);
        let locator = unsafe { locator::new_unchecked(locator) };
        (
            locator,
            config
                .split(LIST_SEPARATOR)
                .map(|prop| split_once(prop, FIELD_SEPARATOR)),
        )
    }
    pub fn locator(&self) -> &locator {
        let (locator, _) = split_once(&self.inner, CONFIG_SEPARATOR);
        unsafe { locator::new_unchecked(locator) }
    }
    pub fn locator_mut(&mut self) -> &mut locator {
        let (locator, _) = split_once_mut(&mut self.inner, CONFIG_SEPARATOR);
        unsafe { locator::new_unchecked_mut(locator) }
    }
}
