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

use humantime::{format_rfc3339, parse_rfc3339_weak};
use std::{
    convert::{TryFrom, TryInto},
    fmt::Display,
    ops::Add,
    str::FromStr,
    time::{Duration, SystemTime},
};
use zenoh_result::{bail, zerror, ZError};

const U_TO_SECS: f64 = 0.000001;
const MS_TO_SECS: f64 = 0.001;
const M_TO_SECS: f64 = 60.0;
const H_TO_SECS: f64 = M_TO_SECS * 60.0;
const D_TO_SECS: f64 = H_TO_SECS * 24.0;
const W_TO_SECS: f64 = D_TO_SECS * 7.0;

/// The structural representation of the Zenoh Time DSL, which may adopt one of two syntax:
/// - the "range" syntax: `<ldel: '[' | ']'><start: TimeExpr?>..<end: TimeExpr?><rdel: '[' | ']'>`
/// - the "duration" syntax: `<ldel: '[' | ']'><start: TimeExpr>;<duration: Duration><rdel: '[' | ']'>`, which is
///   equivalent to `<ldel><start>..<start+duration><rdel>`
///
/// Durations follow the `<duration: float><unit: "u", "ms", "s", "m", "h", "d", "w">` syntax.
///
/// Where [`TimeExpr`] itself may adopt one of two syntaxes:
/// - the "instant" syntax, which must be a UTC [RFC3339](https://datatracker.ietf.org/doc/html/rfc3339) formatted timestamp.
/// - the "offset" syntax, which is written `now(<sign: '-'?><offset: Duration?>)`, and allows to specify a target instant as
///   an offset applied to an instant of evaluation. These offset are resolved at the evaluation site.
///
/// In range syntax, omiting `<start>` and/or `<end>` implies that the range is unbounded in that direction.
///
/// Exclusive bounds are represented by their respective delimiters pointing towards the exterior.
/// Interior bounds are represented by the opposite.
///
/// The comparison step for instants is the nanosecond, which makes exclusive and inclusive bounds extremely close.
/// The `[<start>..<end>[` pattern may however be useful to guarantee that a same timestamp never appears twice when
/// iteratively getting values for `[t0..t1[`, `[t1..t2[`, `[t2..t3[`...
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct TimeRange<T = TimeExpr>(pub TimeBound<T>, pub TimeBound<T>);
impl TimeRange<TimeExpr> {
    /// Resolves the offset bounds in the range using `now` as reference.
    pub fn resolve_at(self, now: SystemTime) -> TimeRange<SystemTime> {
        TimeRange(self.0.resolve_at(now), self.1.resolve_at(now))
    }

    /// Resolves the offset bounds in the range using [`SystemTime::now`] as reference.
    pub fn resolve(self) -> TimeRange<SystemTime> {
        self.resolve_at(SystemTime::now())
    }

    /// Returns `true` if the provided `instant` belongs to `self`.
    ///
    /// This method performs resolution with [`SystemTime::now`] if the bounds contain an "offset" time expression.
    /// If you intend on performing this check multiple times, it may be wiser to resolve `self` first, and use
    /// [`TimeRange::<SystemTime>::contains`] instead.
    pub fn contains(&self, instant: SystemTime) -> bool {
        let now = SystemTime::now();
        match &self.0 {
            TimeBound::Inclusive(t) if t.resolve_at(now) > instant => return false,
            TimeBound::Exclusive(t) if t.resolve_at(now) >= instant => return false,
            _ => {}
        }
        match &self.1 {
            TimeBound::Inclusive(t) => t.resolve_at(now) >= instant,
            TimeBound::Exclusive(t) => t.resolve_at(now) > instant,
            _ => true,
        }
    }
}
impl TimeRange<SystemTime> {
    /// Returns `true` if the provided `instant` belongs to `self`.
    pub fn contains(&self, instant: SystemTime) -> bool {
        match &self.0 {
            TimeBound::Inclusive(t) if *t > instant => return false,
            TimeBound::Exclusive(t) if *t >= instant => return false,
            _ => {}
        }
        match &self.1 {
            TimeBound::Inclusive(t) => *t >= instant,
            TimeBound::Exclusive(t) => *t > instant,
            _ => true,
        }
    }
}
impl From<TimeRange<SystemTime>> for TimeRange<TimeExpr> {
    fn from(value: TimeRange<SystemTime>) -> Self {
        TimeRange(value.0.into(), value.1.into())
    }
}
impl TryFrom<TimeRange<TimeExpr>> for TimeRange<SystemTime> {
    type Error = ();
    fn try_from(value: TimeRange<TimeExpr>) -> Result<Self, Self::Error> {
        Ok(TimeRange(value.0.try_into()?, value.1.try_into()?))
    }
}
impl Display for TimeRange<TimeExpr> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.0 {
            TimeBound::Inclusive(t) => write!(f, "[{t}..")?,
            TimeBound::Exclusive(t) => write!(f, "]{t}..")?,
            TimeBound::Unbounded => f.write_str("[..")?,
        }
        match &self.1 {
            TimeBound::Inclusive(t) => write!(f, "{t}]"),
            TimeBound::Exclusive(t) => write!(f, "{t}["),
            TimeBound::Unbounded => f.write_str("]"),
        }
    }
}
impl Display for TimeRange<SystemTime> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.0 {
            TimeBound::Inclusive(t) => write!(f, "[{}..", TimeExpr::Fixed(*t))?,
            TimeBound::Exclusive(t) => write!(f, "]{}..", TimeExpr::Fixed(*t))?,
            TimeBound::Unbounded => f.write_str("[..")?,
        }
        match &self.1 {
            TimeBound::Inclusive(t) => write!(f, "{}]", TimeExpr::Fixed(*t)),
            TimeBound::Exclusive(t) => write!(f, "{}[", TimeExpr::Fixed(*t)),
            TimeBound::Unbounded => f.write_str("]"),
        }
    }
}

impl FromStr for TimeRange<TimeExpr> {
    type Err = ZError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // minimum str size is 4: "[..]"
        let len = s.len();
        if len < 4 {
            bail!("Invalid TimeRange: {}", s);
        }

        let mut chars = s.chars();
        let inclusive_start = match chars.next().unwrap() {
            '[' => true,
            ']' => false,
            _ => bail!("Invalid TimeRange (must start with '[' or ']'): {}", s),
        };
        let inclusive_end = match chars.last().unwrap() {
            ']' => true,
            '[' => false,
            _ => bail!("Invalid TimeRange (must end with '[' or ']'): {}", s),
        };

        let s = &s[1..len - 1];
        if let Some((start, end)) = s.split_once("..") {
            Ok(TimeRange(
                parse_time_bound(start, inclusive_start)?,
                parse_time_bound(end, inclusive_end)?,
            ))
        } else if let Some((start, duration)) = s.split_once(';') {
            let start_bound = parse_time_bound(start, inclusive_start)?;
            let duration = parse_duration(duration)?;
            let end_bound = match &start_bound {
                TimeBound::Inclusive(time) | TimeBound::Exclusive(time) => {
                    if inclusive_end {
                        TimeBound::Inclusive(time + duration)
                    } else {
                        TimeBound::Exclusive(time + duration)
                    }
                }
                TimeBound::Unbounded => bail!(
                    r#"Invalid TimeRange (';' must contain a time and a duration)"): {}"#,
                    s
                ),
            };
            Ok(TimeRange(start_bound, end_bound))
        } else {
            bail!(
                r#"Invalid TimeRange (must contain ".." or ";" as separator)"): {}"#,
                s
            )
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum TimeBound<T> {
    Inclusive(T),
    Exclusive(T),
    Unbounded,
}
impl From<TimeBound<SystemTime>> for TimeBound<TimeExpr> {
    fn from(value: TimeBound<SystemTime>) -> Self {
        match value {
            TimeBound::Inclusive(t) => TimeBound::Inclusive(t.into()),
            TimeBound::Exclusive(t) => TimeBound::Exclusive(t.into()),
            TimeBound::Unbounded => TimeBound::Unbounded,
        }
    }
}
impl TryFrom<TimeBound<TimeExpr>> for TimeBound<SystemTime> {
    type Error = ();
    fn try_from(value: TimeBound<TimeExpr>) -> Result<Self, Self::Error> {
        Ok(match value {
            TimeBound::Inclusive(t) => TimeBound::Inclusive(t.try_into()?),
            TimeBound::Exclusive(t) => TimeBound::Exclusive(t.try_into()?),
            TimeBound::Unbounded => TimeBound::Unbounded,
        })
    }
}
impl TimeBound<TimeExpr> {
    pub fn resolve_at(self, now: SystemTime) -> TimeBound<SystemTime> {
        match self {
            TimeBound::Inclusive(t) => TimeBound::Inclusive(t.resolve_at(now)),
            TimeBound::Exclusive(t) => TimeBound::Exclusive(t.resolve_at(now)),
            TimeBound::Unbounded => TimeBound::Unbounded,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum TimeExpr {
    Fixed(SystemTime),
    Now { offset_secs: f64 },
}
impl From<SystemTime> for TimeExpr {
    fn from(t: SystemTime) -> Self {
        Self::Fixed(t)
    }
}
impl TryFrom<TimeExpr> for SystemTime {
    type Error = ();
    fn try_from(value: TimeExpr) -> Result<Self, Self::Error> {
        match value {
            TimeExpr::Fixed(t) => Ok(t),
            TimeExpr::Now { .. } => Err(()),
        }
    }
}
impl TimeExpr {
    /// Resolves `self` into a [`SystemTime`], using `now` as a reference for offset expressions.
    pub fn resolve_at(&self, now: SystemTime) -> SystemTime {
        match self {
            TimeExpr::Fixed(t) => *t,
            TimeExpr::Now { offset_secs } => now + std::time::Duration::from_secs_f64(*offset_secs),
        }
    }
}
impl Display for TimeExpr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TimeExpr::Fixed(time) => {
                write!(f, "{}", format_rfc3339(*time))
            }
            TimeExpr::Now { offset_secs } => {
                if *offset_secs == 0.0 {
                    f.write_str("now()")
                } else {
                    write!(f, "now({offset_secs}s)")
                }
            }
        }
    }
}

impl FromStr for TimeExpr {
    type Err = ZError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.starts_with("now(") && s.ends_with(')') {
            let s = &s[4..s.len() - 1];
            if s.is_empty() {
                Ok(TimeExpr::Now { offset_secs: 0.0 })
            } else {
                match s.chars().next().unwrap() {
                    '-' => parse_duration(&s[1..]).map(|f| TimeExpr::Now { offset_secs: -f }),
                    _ => parse_duration(s).map(|f| TimeExpr::Now { offset_secs: f }),
                }
            }
        } else {
            parse_rfc3339_weak(s)
                .map_err(|e| zerror!(e))
                .map(TimeExpr::Fixed)
        }
        .map_err(|e| zerror!(r#"Invalid time "{}" ({})"#, s, e))
    }
}

impl Add<f64> for TimeExpr {
    type Output = Self;
    fn add(self, duration: f64) -> Self {
        match self {
            Self::Fixed(time) => Self::Fixed(time + Duration::from_secs_f64(duration)),
            Self::Now { offset_secs } => Self::Now {
                offset_secs: offset_secs + duration,
            },
        }
    }
}

impl<'a> Add<f64> for &'a TimeExpr {
    type Output = TimeExpr;
    fn add(self, duration: f64) -> TimeExpr {
        match self {
            TimeExpr::Fixed(time) => TimeExpr::Fixed((*time) + Duration::from_secs_f64(duration)),
            TimeExpr::Now { offset_secs } => TimeExpr::Now {
                offset_secs: offset_secs + duration,
            },
        }
    }
}

fn parse_time_bound(s: &str, inclusive: bool) -> Result<TimeBound<TimeExpr>, ZError> {
    if s.is_empty() {
        Ok(TimeBound::Unbounded)
    } else if inclusive {
        Ok(TimeBound::Inclusive(s.parse()?))
    } else {
        Ok(TimeBound::Exclusive(s.parse()?))
    }
}

/// Parses a &str as a Duration.
/// Expected format is a f64 in seconds, or "<f64><unit>" where <unit> is:
///  - 'u'  => microseconds
///  - "ms" => milliseconds
///  - 's' => seconds
///  - 'm' => minutes
///  - 'h' => hours
///  - 'd' => days
///  - 'w' => weeks
fn parse_duration(s: &str) -> Result<f64, ZError> {
    if s.is_empty() {
        bail!(
            r#"Invalid duration: "" (expected format: <f64> (in seconds) or <f64><unit>. Accepted units: u, ms, s, m, h, d or w.)"#
        );
    }
    let mut it = s.bytes().enumerate().rev();
    match it.next().unwrap() {
        (i, b'u') => s[..i].parse::<f64>().map(|u| U_TO_SECS * u),
        (_, b's') => match it.next().unwrap() {
            (i, b'm') => s[..i].parse::<f64>().map(|ms| MS_TO_SECS * ms),
            (i, _) => s[..i + 1].parse::<f64>(),
        },
        (i, b'm') => s[..i].parse::<f64>().map(|m| M_TO_SECS * m),
        (i, b'h') => s[..i].parse::<f64>().map(|h| H_TO_SECS * h),
        (i, b'd') => s[..i].parse::<f64>().map(|d| D_TO_SECS * d),
        (i, b'w') => s[..i].parse::<f64>().map(|w| W_TO_SECS * w),
        _ => s.parse::<f64>(),
    }
    .map_err(|e| zerror!(r#"Invalid duration "{}" ({})"#, s, e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_time_range() {
        use TimeBound::*;
        assert_eq!(
            "[..]".parse::<TimeRange>().unwrap(),
            TimeRange(Unbounded, Unbounded)
        );

        assert!("".parse::<TimeExpr>().is_err());
        assert!("[;]".parse::<TimeExpr>().is_err());
        assert!("[;1h]".parse::<TimeExpr>().is_err());
    }

    #[test]
    fn test_parse_time_expr() {
        assert_eq!(
            "2022-06-30T01:02:03.226942997Z"
                .parse::<TimeExpr>()
                .unwrap(),
            TimeExpr::Fixed(humantime::parse_rfc3339("2022-06-30T01:02:03.226942997Z").unwrap())
        );
        assert_eq!(
            "2022-06-30T01:02:03Z".parse::<TimeExpr>().unwrap(),
            TimeExpr::Fixed(humantime::parse_rfc3339("2022-06-30T01:02:03Z").unwrap())
        );
        assert_eq!(
            "2022-06-30T01:02:03".parse::<TimeExpr>().unwrap(),
            TimeExpr::Fixed(humantime::parse_rfc3339("2022-06-30T01:02:03Z").unwrap())
        );
        assert_eq!(
            "2022-06-30 01:02:03Z".parse::<TimeExpr>().unwrap(),
            TimeExpr::Fixed(humantime::parse_rfc3339("2022-06-30T01:02:03Z").unwrap())
        );
        assert_eq!(
            "now()".parse::<TimeExpr>().unwrap(),
            TimeExpr::Now { offset_secs: 0.0 }
        );
        assert_eq!(
            "now(123.45)".parse::<TimeExpr>().unwrap(),
            TimeExpr::Now {
                offset_secs: 123.45
            }
        );
        assert_eq!(
            "now(1h)".parse::<TimeExpr>().unwrap(),
            TimeExpr::Now {
                offset_secs: 3600.0
            }
        );
        assert_eq!(
            "now(-1h)".parse::<TimeExpr>().unwrap(),
            TimeExpr::Now {
                offset_secs: -3600.0
            }
        );

        assert!("".parse::<TimeExpr>().is_err());
        assert!("1h".parse::<TimeExpr>().is_err());
        assert!("2020-11-05".parse::<TimeExpr>().is_err());
    }

    #[test]
    fn test_parse_duration() {
        assert_eq!(parse_duration("0").unwrap(), 0.0);
        assert_eq!(parse_duration("1.2").unwrap(), 1.2);
        assert_eq!(parse_duration("1u").unwrap(), 0.000001);
        assert_eq!(parse_duration("2u").unwrap(), 0.000002);
        assert_eq!(parse_duration("1.5ms").unwrap(), 0.0015);
        assert_eq!(parse_duration("100ms").unwrap(), 0.1);
        assert_eq!(parse_duration("10s").unwrap(), 10.0);
        assert_eq!(parse_duration("0.5s").unwrap(), 0.5);
        assert_eq!(parse_duration("1.1m").unwrap(), 66.0);
        assert_eq!(parse_duration("1.5h").unwrap(), 5400.0);
        assert_eq!(parse_duration("1d").unwrap(), 86400.0);
        assert_eq!(parse_duration("1w").unwrap(), 604800.0);

        assert!(parse_duration("").is_err());
        assert!(parse_duration("1x").is_err());
        assert!(parse_duration("abcd").is_err());
        assert!(parse_duration("4mm").is_err());
        assert!(parse_duration("1h4m").is_err());
    }
}
