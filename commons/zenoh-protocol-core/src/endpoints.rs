use zenoh_core::bail;

use crate::{
    locators::{ArcProperties, Locator},
    split_once,
};

use super::locators::{CONFIG_SEPARATOR, FIELD_SEPARATOR, LIST_SEPARATOR, METADATA_SEPARATOR};

use std::{
    convert::{TryFrom, TryInto},
    iter::FromIterator,
    str::FromStr,
};

/// A `String` that respects the [`EndPoint`] canon form: `<locator>#<config>`, such that `<locator>` is a valid [`Locator`] `<config>` is of the form `<key1>=<value1>;...;<keyN>=<valueN>` where keys are alphabetically sorted.
#[derive(Clone, Debug, PartialEq)]
pub struct EndPoint {
    pub locator: Locator,
    pub config: Option<ArcProperties>,
}
impl core::fmt::Display for EndPoint {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", &self.locator)?;
        if let Some(meta) = &self.config {
            let mut iter = meta.iter();
            if let Some((k, v)) = iter.next() {
                write!(f, "{}{}{}{}", CONFIG_SEPARATOR, k, FIELD_SEPARATOR, v)?;
            }
            for (k, v) in iter {
                write!(f, "{}{}{}{}", LIST_SEPARATOR, k, FIELD_SEPARATOR, v)?;
            }
        }
        Ok(())
    }
}

impl EndPoint {
    #[must_use = "returns true on success"]
    pub fn set_addr(&mut self, addr: &str) -> bool {
        unsafe { std::mem::transmute::<_, &mut Locator>(self) }.set_addr(addr)
    }
    pub fn extend_configuration(&mut self, extension: impl IntoIterator<Item = (String, String)>) {
        match self.config.is_some() {
            true => match &mut self.config {
                Some(config) => config.extend(extension),
                None => unsafe { std::hint::unreachable_unchecked() },
            },
            false => {
                self.config =
                    Some(std::collections::HashMap::from_iter(extension.into_iter()).into())
            }
        }
    }
}
impl From<EndPoint> for Locator {
    fn from(val: EndPoint) -> Self {
        val.locator
    }
}
impl From<Locator> for EndPoint {
    fn from(val: Locator) -> Self {
        EndPoint {
            locator: val,
            config: None,
        }
    }
}

impl TryFrom<String> for EndPoint {
    type Error = zenoh_core::Error;
    fn try_from(mut value: String) -> Result<Self, Self::Error> {
        let (l, r) = split_once(&value, CONFIG_SEPARATOR);
        if r.contains(METADATA_SEPARATOR) {
            bail!(
                "{} is a forbidden character in endpoint metadata",
                METADATA_SEPARATOR
            );
        }
        let config = r.parse().ok();
        let locator_len = l.len();
        value.truncate(locator_len);
        Ok(EndPoint {
            locator: value.try_into()?,
            config,
        })
    }
}
impl FromStr for EndPoint {
    type Err = zenoh_core::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (l, r) = split_once(s, CONFIG_SEPARATOR);
        if r.contains(METADATA_SEPARATOR) {
            bail!(
                "{} is a forbidden character in endpoint metadata",
                METADATA_SEPARATOR
            );
        }
        Ok(EndPoint {
            locator: l.parse()?,
            config: r.parse().ok(),
        })
    }
}
