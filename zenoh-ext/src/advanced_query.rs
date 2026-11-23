//
// Copyright (c) 2026 ZettaScale Technology
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
//! Shared range syntax for advanced cache queries and recovery requests.
//!
//! `_sn` selects publications by source sequence number; `_fn` selects fragments
//! within those publications. Both use inclusive `start..end` bounds, with an
//! empty bound meaning unbounded and a bare number selecting one value. Parsing
//! retains the cache's permissive fallback for malformed bounds.

use std::ops::{Bound, RangeBounds};

use crate::utils::WrappingSn;

/// Inclusive sequence-number or fragment-number bounds in an advanced query.
/// Both retain the wrapping comparisons used by the cache's range decoder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct QueryRange((Bound<WrappingSn>, Bound<WrappingSn>));

impl QueryRange {
    pub(crate) fn from_parameter(range: Option<&str>) -> Self {
        Self(
            range
                .map(decode_range)
                .unwrap_or((Bound::Unbounded, Bound::Unbounded)),
        )
    }

    pub(crate) fn is_unbounded(&self) -> bool {
        self.0 == (Bound::Unbounded, Bound::Unbounded)
    }

    pub(crate) fn contains(&self, number: u32) -> bool {
        self.0.contains(&number)
    }
}

#[zenoh_macros::unstable]
fn decode_range(range: &str) -> (Bound<WrappingSn>, Bound<WrappingSn>) {
    let mut split = range.split("..");
    let start = split
        .next()
        .and_then(|s| s.parse::<WrappingSn>().ok().map(Bound::Included))
        .unwrap_or(Bound::Unbounded);
    let end = split
        .next()
        .map(|s| {
            s.parse::<WrappingSn>()
                .ok()
                .map(Bound::Included)
                .unwrap_or(Bound::Unbounded)
        })
        .unwrap_or(start);
    (start, end)
}

pub(crate) fn format_range(
    name: &str,
    start: Option<WrappingSn>,
    end: Option<WrappingSn>,
) -> String {
    match (start, end) {
        (Some(start), Some(end)) => format!("{}={}..{}", name, start, end),
        (Some(start), None) => format!("{}={}..", name, start),
        (None, Some(end)) => format!("{}=..{}", name, end),
        (None, None) => format!("{}=..", name),
    }
}

#[cfg(test)]
mod tests {
    use std::ops::Bound;

    use super::{decode_range, format_range, QueryRange};
    use crate::utils::WrappingSn;

    #[test]
    fn decode_range_bounded() {
        assert_eq!(
            decode_range("2..3"),
            (
                Bound::Included(WrappingSn(2)),
                Bound::Included(WrappingSn(3))
            )
        );
    }

    #[test]
    fn decode_range_start_only() {
        assert_eq!(
            decode_range("2.."),
            (Bound::Included(WrappingSn(2)), Bound::Unbounded)
        );
    }

    #[test]
    fn decode_range_end_only() {
        assert_eq!(
            decode_range("..3"),
            (Bound::Unbounded, Bound::Included(WrappingSn(3)))
        );
    }

    #[test]
    fn decode_range_unbounded() {
        assert_eq!(decode_range(".."), (Bound::Unbounded, Bound::Unbounded));
    }

    #[test]
    fn decode_range_empty() {
        assert_eq!(decode_range(""), (Bound::Unbounded, Bound::Unbounded));
    }

    // A bare value with no `..` separator falls back to `unwrap_or(start)`,
    // yielding a singleton range `Included(v)..=Included(v)`.
    #[test]
    fn decode_range_single_value() {
        assert_eq!(
            decode_range("2"),
            (
                Bound::Included(WrappingSn(2)),
                Bound::Included(WrappingSn(2))
            )
        );
    }

    // A non-numeric end fails to parse and degrades to `Unbounded` rather than
    // panicking, so callers receive a half-open range instead of an error.
    #[test]
    fn decode_range_malformed_end() {
        assert_eq!(
            decode_range("2..abc"),
            (Bound::Included(WrappingSn(2)), Bound::Unbounded)
        );
    }

    // A non-numeric start fails to parse to `Unbounded`; the well-formed end is
    // still honoured.
    #[test]
    fn decode_range_malformed_start() {
        assert_eq!(
            decode_range("abc..3"),
            (Bound::Unbounded, Bound::Included(WrappingSn(3)))
        );
    }

    #[test]
    fn decode_range_ignores_extra_bounds() {
        assert_eq!(decode_range("2..3..4"), decode_range("2..3"));
        assert_eq!(decode_range("2....4"), decode_range("2.."));
    }

    #[test]
    fn query_range_unbounded_parameters() {
        for parameter in [
            None,
            Some(""),
            Some(".."),
            Some("abc"),
            Some("abc..def"),
            Some("4294967296"),
            Some("-1"),
        ] {
            let range = QueryRange::from_parameter(parameter);
            assert!(range.is_unbounded());
            assert!(range.contains(0));
            assert!(range.contains(u32::MAX));
        }
    }

    #[test]
    fn query_range_wraps_sequence_numbers() {
        let range = QueryRange::from_parameter(Some("4294967294..1"));
        for number in [u32::MAX - 1, u32::MAX, 0, 1] {
            assert!(range.contains(number));
        }
        assert!(!range.contains(u32::MAX - 2));
        assert!(!range.contains(2));
        assert!(QueryRange::from_parameter(Some("4294967295..")).contains(0));
        assert!(QueryRange::from_parameter(Some("..0")).contains(u32::MAX));
    }

    #[test]
    fn format_range_preserves_names_and_open_bounds() {
        assert_eq!(
            format_range("_sn", Some(WrappingSn(u32::MAX)), Some(WrappingSn(0))),
            "_sn=4294967295..0"
        );
        assert_eq!(
            format_range("_fn", Some(WrappingSn(2)), Some(WrappingSn(2))),
            "_fn=2..2"
        );
        assert_eq!(format_range("_fn", Some(WrappingSn(2)), None), "_fn=2..");
        assert_eq!(format_range("_fn", None, Some(WrappingSn(3))), "_fn=..3");
        assert_eq!(format_range("_sn", None, None), "_sn=..");
    }
}
