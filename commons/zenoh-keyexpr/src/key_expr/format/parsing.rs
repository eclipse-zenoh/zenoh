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

use super::{support::trim_suffix_slash, IKeFormatStorage, KeFormat, Segment};
use crate::key_expr::{format::support::trim_prefix_slash, keyexpr};

pub struct Parsed<'s, Storage: IKeFormatStorage<'s>> {
    format: &'s KeFormat<'s, Storage>,
    results: Storage::ValuesStorage<Option<&'s keyexpr>>,
}

impl<'s, Storage: IKeFormatStorage<'s>> Parsed<'s, Storage> {
    pub fn get(&self, id: &str) -> ZResult<Option<&'s keyexpr>> {
        let Some(i) = self.format.storage.segments().iter().position(|s| s.spec.id() == id) else {bail!("{} has no {id} field", self.format)};
        Ok(self.results.as_ref()[i])
    }
    pub fn values(&self) -> &[Option<&'s keyexpr>] {
        self.results.as_ref()
    }
    pub fn iter(&'s self) -> Iter<'s, Storage> {
        self.into_iter()
    }
}

impl<'s, Storage: IKeFormatStorage<'s>> IntoIterator for &'s Parsed<'s, Storage> {
    type Item = <Self::IntoIter as Iterator>::Item;
    type IntoIter = Iter<'s, Storage>;
    fn into_iter(self) -> Self::IntoIter {
        todo!()
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
    pub fn parse(&'s self, target: &'s keyexpr) -> ZResult<Parsed<'s, Storage>> {
        let segments = self.storage.segments();
        let mut results = self.storage.values_storage(|_| None);
        let Some(target) = target.strip_suffix(self.suffix) else {
            if !segments.is_empty()
            && segments.iter().all(|s| s.spec.pattern() == "**")
            && self.suffix.as_bytes()[0] == b'/'
            && target == &self.suffix[1..] {
                return Ok(Parsed { format: self, results });
            }
            bail!("{target} is not included in {self}")
        };
        assert_eq!(segments.len(), results.as_mut().len());
        if do_parse(target, segments, results.as_mut()) {
            Ok(Parsed {
                format: self,
                results,
            })
        } else {
            bail!("{target} is not included in {self}")
        }
    }
}

fn do_parse<'s>(
    input: &'s str,
    segments: &[Segment<'s>],
    results: &mut [Option<&'s keyexpr>],
) -> bool {
    debug_assert!(!input.starts_with('/'));
    // Parsing is finished if there are no more segments to process AND the input is now empty.
    let [segment, segments @ ..] = segments else {return input.is_empty()};
    let [result, results @ ..] = results else {unreachable!()};
    // reset result to None in case of backtracking
    *result = None;
    // Inspect the pattern: we want to know how many chunks we need to have a chance of inclusion, as well as if we need to worry about double wilds
    let pattern = segment.spec.pattern();
    let mut has_double_wilds = false;
    let min_chunks = pattern
        .split('/')
        .filter(|s| {
            if *s == "**" {
                has_double_wilds = true;
                false
            } else {
                true
            }
        })
        .count();
    // Since input is /-stripped, we need to strip it from the prefix too.
    let prefix = trim_prefix_slash(segment.prefix);
    // We handle double-wild segments that may branch in a different function, to keep this one tail-recursive
    if has_double_wilds {
        return do_parse_doublewild(
            input, segments, results, result, pattern, prefix, min_chunks,
        );
    }
    // Strip the prefix (including the end-/ if the prefix is non-empty)
    let Some(input) = input.strip_prefix(prefix) else {return false};
    let mut chunks = 0;
    for i in (0..input.len()).filter(|i| input.as_bytes()[*i] == b'/') {
        chunks += 1;
        if chunks < min_chunks {
            continue;
        }
        let r = keyexpr::new(&input[..i]).expect("any subsection of a keyexpr is a keyexpr");
        if pattern.includes(r) {
            *result = Some(r);
            return do_parse(trim_prefix_slash(&input[(i + 1)..]), segments, results);
        } else {
            return false;
        }
    }
    chunks += 1;
    if chunks < min_chunks {
        return false;
    }
    let r = keyexpr::new(input).expect("any subsection of a keyexpr is a keyexpr");
    if pattern.includes(r) {
        *result = Some(r);
        do_parse("", segments, results)
    } else {
        false
    }
}
fn do_parse_doublewild<'s>(
    input: &'s str,
    segments: &[Segment<'s>],
    results: &mut [Option<&'s keyexpr>],
    result: &mut Option<&'s keyexpr>,
    pattern: &keyexpr,
    prefix: &str,
    min_chunks: usize,
) -> bool {
    if min_chunks == 0 {
        if let Some(input) = input.strip_prefix(trim_suffix_slash(prefix)) {
            if do_parse(trim_prefix_slash(input), segments, results) {
                return true;
            }
        } else {
            return false;
        }
    }
    let Some(input) = input.strip_prefix(prefix) else {return false};
    let input = trim_prefix_slash(input);
    let mut chunks = 0;
    for i in (0..input.len()).filter(|i| input.as_bytes()[*i] == b'/') {
        chunks += 1;
        if chunks < min_chunks {
            continue;
        }
        let r = keyexpr::new(&input[..i]).expect("any subsection of a keyexpr is a keyexpr");
        if pattern.includes(r) {
            *result = Some(r);
            if do_parse(trim_prefix_slash(&input[(i + 1)..]), segments, results) {
                return true;
            }
        }
    }
    chunks += 1;
    if chunks < min_chunks {
        return false;
    }
    let r = keyexpr::new(input).expect("any subsection of a keyexpr is a keyexpr");
    if pattern.includes(r) {
        *result = Some(r);
        do_parse("", segments, results)
    } else {
        false
    }
}

#[test]
fn parsing() {
    use crate::key_expr::OwnedKeyExpr;
    use core::convert::TryFrom;
    for a_spec in ["${a:*}", "a/${a:*}", "a/${a:*/**}"] {
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
                        assert_eq!(parsed.get("a").unwrap().unwrap().as_str(), a_val);
                        assert_eq!(parsed.get("b").unwrap().map_or("", |s| s.as_str()), b_val);
                    }
                }
            }
        }
    }
}
