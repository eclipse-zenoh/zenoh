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

use zenoh_result::{bail, ZResult};

use super::{IKeFormatStorage, KeFormat, Segment};
use crate::key_expr::keyexpr;

pub struct Parsed<'s, Storage: IKeFormatStorage<'s>> {
    format: &'s KeFormat<'s, Storage>,
    results: Storage::ValuesStorage<Option<&'s keyexpr>>,
}

impl<'s, Storage: IKeFormatStorage<'s>> Parsed<'s, Storage> {
    /// Access the `id` element.
    ///
    /// The returned string is guaranteed to either be an empty string or a valid key expression.
    ///
    /// # Errors
    /// If `id` is not part of `self`'s specs.
    pub fn get(&self, id: &str) -> ZResult<&'s str> {
        let Some(i) = self
            .format
            .storage
            .segments()
            .iter()
            .position(|s| s.spec.id() == id)
        else {
            bail!("{} has no {id} field", self.format)
        };
        Ok(self.results.as_ref()[i].map_or("", keyexpr::as_str))
    }
    /// The raw values for each spec, in left-to-right order.
    pub fn values(&self) -> &[Option<&'s keyexpr>] {
        self.results.as_ref()
    }
    /// Iterates over id-value pairs.
    pub fn iter(&'s self) -> Iter<'s, Storage> {
        self.into_iter()
    }
}

impl<'s, Storage: IKeFormatStorage<'s>> IntoIterator for &'s Parsed<'s, Storage> {
    type Item = <Self::IntoIter as Iterator>::Item;
    type IntoIter = Iter<'s, Storage>;
    fn into_iter(self) -> Self::IntoIter {
        Iter {
            parsed: self,
            start: 0,
            end: self.format.storage.segments().len(),
        }
    }
}
pub struct Iter<'s, Storage: IKeFormatStorage<'s>> {
    parsed: &'s Parsed<'s, Storage>,
    start: usize,
    end: usize,
}
impl<'s, Storage: IKeFormatStorage<'s>> Iterator for Iter<'s, Storage> {
    type Item = (&'s str, Option<&'s keyexpr>);
    fn next(&mut self) -> Option<Self::Item> {
        if self.start < self.end {
            let id = self.parsed.format.storage.segments()[self.start].spec.id();
            let ke = self.parsed.results.as_ref()[self.start];
            self.start += 1;
            Some((id, ke))
        } else {
            None
        }
    }
    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        self.start += n;
        self.next()
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        let result = self.end - self.start;
        (result, Some(result))
    }
}
impl<'s, Storage: IKeFormatStorage<'s>> ExactSizeIterator for Iter<'s, Storage> {
    fn len(&self) -> usize {
        self.end - self.start
    }
}
impl<'s, Storage: IKeFormatStorage<'s>> DoubleEndedIterator for Iter<'s, Storage> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.start < self.end {
            self.end -= 1;
            let id = self.parsed.format.storage.segments()[self.end].spec.id();
            let ke = self.parsed.results.as_ref()[self.end];
            Some((id, ke))
        } else {
            None
        }
    }
}

impl<'s, Storage: IKeFormatStorage<'s> + 's> KeFormat<'s, Storage> {
    /// Parses `target` according to `self`. The returned [`Parsed`] object can be used to extract the values of the fields in `self` from `target`.
    ///
    /// Parsing is greedy and done left-to-right. Please refer to [`KeFormat`]'s documentation for more details.
    ///
    /// # Errors
    /// If `target` does not intersect with `self`, an error is returned.
    pub fn parse(&'s self, target: &'s keyexpr) -> ZResult<Parsed<'s, Storage>> {
        let segments = self.storage.segments();
        if segments.is_empty()
            && !target.intersects(unsafe { keyexpr::from_str_unchecked(self.suffix) })
        {
            bail!("{target} does not intersect with {self}")
        }
        let mut results = self.storage.values_storage(|_| None);
        let results_mut = results.as_mut();
        debug_assert_eq!(segments.len(), results_mut.len());
        let found = 'a: {
            match self.suffix.as_bytes() {
                [] => do_parse(Some(target), segments, results_mut),
                [b'/', suffix @ ..] => {
                    let suffix = unsafe { keyexpr::from_slice_unchecked(suffix) };
                    for (target, candidate) in target.iter_splits_rtl() {
                        if suffix.intersects(candidate)
                            && do_parse(Some(target), segments, results_mut)
                        {
                            break 'a true;
                        }
                    }
                    suffix.intersects(target) && do_parse(None, segments, results_mut)
                }
                _ => {
                    unreachable!();
                }
            }
        };
        if found {
            Ok(Parsed {
                format: self,
                results,
            })
        } else {
            bail!("{target} does not intersect with {self}")
        }
    }
}

fn do_parse<'a>(
    target: Option<&'a keyexpr>,
    segments: &[Segment],
    results: &mut [Option<&'a keyexpr>],
) -> bool {
    match (segments, results) {
        ([], []) => target.map_or(true, keyexpr::is_double_wild),
        ([segment, segments @ ..], [result, results @ ..]) => {
            let prefix = segment.prefix();
            let pattern = segment.pattern();
            // if target is empty
            let Some(target) = target else {
                // this segment only matches if the pattern is `**` and the prefix is empty (since it cannot be `**`)
                if prefix.is_none() && pattern.is_double_wild() {
                    *result = None;
                    // the next segments still have to be checked to respect the same condition
                    return !segments.iter().zip(results).any(|(segment, result)| {
                        *result = None;
                        segment.prefix().is_some() || !segment.pattern().is_double_wild()
                    });
                } else {
                    return false;
                }
            };
            macro_rules! try_intersect {
                ($pattern: expr, $result: expr, $target: expr, $segments: expr, $results: expr) => {{
                    let target = $target;
                    let segments = $segments;
                    if $pattern.intersects(target)
                        && do_parse(
                            target.is_double_wild().then_some(target),
                            segments,
                            $results,
                        )
                    {
                        *$result = Some(target);
                        return true;
                    }
                    for (candidate, target) in target.iter_splits_rtl() {
                        if $pattern.intersects(candidate)
                            && do_parse(Some(target), segments, $results)
                        {
                            *result = Some(candidate);
                            return true;
                        }
                    }
                    if $pattern.is_double_wild() && do_parse(Some(target), segments, $results) {
                        *$result = None;
                        return true;
                    }
                }};
            }
            //if the prefix can be compressed to empty,
            if prefix.is_none() {
                try_intersect!(pattern, result, target, segments, results);
            }
            // iterate through as many splits as `prefix` could possibly consume.
            for (candidate, target) in target.iter_splits_ltr().take(match prefix {
                None => 1,
                Some(prefix) => (prefix.bytes().filter(|&c| c == b'/').count() + 1) * 3,
            }) {
                if prefix.map_or(candidate.is_double_wild(), |prefix| {
                    prefix.intersects(candidate)
                }) {
                    try_intersect!(pattern, result, target, segments, results);
                }
            }
            pattern.is_double_wild()
                && prefix.is_some_and(|prefix| prefix.intersects(target))
                && do_parse(None, segments, results)
        }
        _ => unreachable!(),
    }
}

#[test]
fn parsing() {
    use core::convert::TryFrom;

    use crate::key_expr::OwnedKeyExpr;
    for a_spec in ["${a:*}", "a/${a:*}"] {
        for b_spec in ["b/${b:**}", "${b:**}"] {
            let specs = [a_spec, b_spec, "c"];
            for spec in [2, 3] {
                let spec = specs[..spec].join("/");
                let format: KeFormat<[Segment; 2]> = KeFormat::noalloc_new(&spec).unwrap();
                let mut formatter = format.formatter();
                for a_val in ["hi"] {
                    formatter.set("a", a_val).unwrap();
                    for b_val in ["hello", "hello/there", ""] {
                        formatter.set("b", b_val).unwrap();
                        let ke = OwnedKeyExpr::try_from(&formatter).unwrap();
                        let parsed = format.parse(&ke).unwrap();
                        assert_eq!(parsed.get("a").unwrap(), a_val);
                        assert_eq!(parsed.get("b").unwrap(), b_val);
                    }
                }
            }
        }
    }
    KeFormat::new("**/${a:**}/${b:**}/**").unwrap_err();
    let format = KeFormat::new("${a:**}/${b:**}").unwrap();
    assert_eq!(
        format
            .parse(keyexpr::new("a/b/c").unwrap())
            .unwrap()
            .get("a")
            .unwrap(),
        "a/b/c"
    );
    assert_eq!(
        format
            .parse(keyexpr::new("**").unwrap())
            .unwrap()
            .get("a")
            .unwrap(),
        "**"
    );
    assert_eq!(
        format
            .parse(keyexpr::new("**").unwrap())
            .unwrap()
            .get("b")
            .unwrap(),
        "**"
    );
    let format = KeFormat::new("hi/${a:there}/${b:**}").unwrap();
    assert_eq!(
        format
            .parse(keyexpr::new("hi/**").unwrap())
            .unwrap()
            .get("a")
            .unwrap(),
        "**"
    );
    assert_eq!(
        format
            .parse(keyexpr::new("hi/**").unwrap())
            .unwrap()
            .get("b")
            .unwrap(),
        "**"
    );
    let format = KeFormat::new("hi/${a:there}/@/${b:**}").unwrap();
    assert_eq!(
        format
            .parse(keyexpr::new("hi/**/@").unwrap())
            .unwrap()
            .get("a")
            .unwrap(),
        "**"
    );
    assert_eq!(
        format
            .parse(keyexpr::new("hi/**/@").unwrap())
            .unwrap()
            .get("b")
            .unwrap(),
        ""
    );
}
