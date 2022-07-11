//
// Copyright (c) 2022 ZettaScale Technology
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
    fmt::Display,
    ops::Add,
    str::FromStr,
    time::{Duration, SystemTime},
};
use zenoh_core::{bail, zerror, zresult::ZError};

const U_TO_SECS: f64 = 0.000001;
const MS_TO_SECS: f64 = 0.001;
const M_TO_SECS: f64 = 60.0;
const H_TO_SECS: f64 = M_TO_SECS * 60.0;
const D_TO_SECS: f64 = H_TO_SECS * 24.0;
const W_TO_SECS: f64 = D_TO_SECS * 7.0;

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct TimeRange(pub TimeBound, pub TimeBound);
impl Display for TimeRange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.0 {
            TimeBound::Inclusive(t) => write!(f, "[{}..", t)?,
            TimeBound::Exclusive(t) => write!(f, "]{}..", t)?,
            TimeBound::Unbounded => f.write_str("[..")?,
        }
        match &self.1 {
            TimeBound::Inclusive(t) => write!(f, "{}]", t),
            TimeBound::Exclusive(t) => write!(f, "{}[", t),
            TimeBound::Unbounded => f.write_str("]"),
        }
    }
}

impl FromStr for TimeRange {
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
            '[' => true,
            ']' => false,
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
                    r#"Invalid TimeRange (';' must separate a time and a duration)"): {}"#,
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

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum TimeBound {
    Inclusive(TimeExpr),
    Exclusive(TimeExpr),
    Unbounded,
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum TimeExpr {
    Fixed(SystemTime),
    Now { offset_secs: f64 },
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
                    write!(f, "now({}s)", offset_secs)
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

fn parse_time_bound(s: &str, inclusive: bool) -> Result<TimeBound, ZError> {
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
///  - 'u' or 'µ' => microseconds
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
    let mut it = s.char_indices().rev();
    match it.next().unwrap() {
        (i, 'u' | 'µ') => s[..i].parse::<f64>().map(|u| U_TO_SECS * u as f64),
        (_, 's') => match it.next().unwrap() {
            (i, 'm') => s[..i].parse::<f64>().map(|ms| MS_TO_SECS * ms as f64),
            (i, _) => s[..i + 1].parse::<f64>().map(|sec| sec as f64),
        },
        (i, 'm') => s[..i].parse::<f64>().map(|m| M_TO_SECS * m as f64),
        (i, 'h') => s[..i].parse::<f64>().map(|h| H_TO_SECS * h as f64),
        (i, 'd') => s[..i].parse::<f64>().map(|d| D_TO_SECS * d as f64),
        (i, 'w') => s[..i].parse::<f64>().map(|w| W_TO_SECS * w as f64),
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
        assert_eq!(parse_duration("2µ").unwrap(), 0.000002);
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
