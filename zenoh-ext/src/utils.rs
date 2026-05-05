use std::{
    cmp, fmt,
    ops::{Add, AddAssign, Sub},
    str::FromStr,
};

use zenoh::sample::SourceSn;

use crate::{Deserialize, ZDeserializeError, ZDeserializer};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct WrappingSn(pub(crate) SourceSn);

impl PartialOrd for WrappingSn {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for WrappingSn {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        match self.0.wrapping_sub(other.0) {
            0 => cmp::Ordering::Equal,
            sub if sub <= SourceSn::MAX / 2 => cmp::Ordering::Greater,
            _ => cmp::Ordering::Less,
        }
    }
}

impl fmt::Display for WrappingSn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for WrappingSn {
    type Err = <u32 as FromStr>::Err;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        u32::from_str(s).map(Self)
    }
}

impl Deserialize for WrappingSn {
    fn deserialize(deserializer: &mut ZDeserializer) -> Result<Self, ZDeserializeError> {
        u32::deserialize(deserializer).map(Self)
    }
}

impl PartialEq<SourceSn> for WrappingSn {
    fn eq(&self, other: &SourceSn) -> bool {
        self.0 == *other
    }
}

impl PartialEq<WrappingSn> for SourceSn {
    fn eq(&self, other: &WrappingSn) -> bool {
        *self == other.0
    }
}

impl PartialOrd<SourceSn> for WrappingSn {
    fn partial_cmp(&self, other: &SourceSn) -> Option<cmp::Ordering> {
        self.partial_cmp(&WrappingSn(*other))
    }
}

impl PartialOrd<WrappingSn> for SourceSn {
    fn partial_cmp(&self, other: &WrappingSn) -> Option<cmp::Ordering> {
        WrappingSn(*self).partial_cmp(other)
    }
}

impl Add<u32> for WrappingSn {
    type Output = Self;
    fn add(self, rhs: u32) -> Self::Output {
        Self(self.0.wrapping_add(rhs))
    }
}

impl AddAssign<u32> for WrappingSn {
    fn add_assign(&mut self, rhs: u32) {
        *self = *self + rhs;
    }
}

impl Sub for WrappingSn {
    type Output = u32;
    fn sub(self, rhs: Self) -> Self::Output {
        self.0.wrapping_sub(rhs.0)
    }
}

impl Sub<WrappingSn> for SourceSn {
    type Output = u32;
    fn sub(self, rhs: WrappingSn) -> Self::Output {
        self.wrapping_sub(rhs.0)
    }
}

impl From<SourceSn> for WrappingSn {
    fn from(value: SourceSn) -> Self {
        Self(value)
    }
}

impl From<WrappingSn> for SourceSn {
    fn from(value: WrappingSn) -> Self {
        value.0
    }
}

#[cfg(test)]
mod tests {
    use crate::utils::WrappingSn;

    #[test]
    fn wrapping_sn_ord() {
        assert!(WrappingSn(0) <= WrappingSn(0));
        assert!(WrappingSn(0) >= WrappingSn(0));
        assert!(WrappingSn(1) > WrappingSn(0));
        assert!(WrappingSn(0) < WrappingSn(1));
        assert!(WrappingSn(u32::MAX) < WrappingSn(0));
        assert!(WrappingSn(0) > WrappingSn(u32::MAX));
        assert!(WrappingSn(u32::MAX / 2 - 1) > WrappingSn(0));
        assert!(WrappingSn(u32::MAX / 2 - 1) > WrappingSn(u32::MAX));
        assert!(WrappingSn(u32::MAX / 2) > WrappingSn(0));
        assert!(WrappingSn(u32::MAX / 2) < WrappingSn(u32::MAX));
        assert!(WrappingSn(u32::MAX / 2 + 1) < WrappingSn(0));
        assert!(WrappingSn(u32::MAX / 2 + 1) < WrappingSn(u32::MAX));
        assert!((WrappingSn(u32::MAX)..WrappingSn(1)).contains(&0));
    }
}
