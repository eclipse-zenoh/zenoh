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

//! Some useful functions for Backend/Storage implementations.

use std::convert::TryInto;
use zenoh::prelude::keyexpr;
use zenoh_core::Result as ZResult;

/// Returns the longest prefix in a key selector that doesn't contain any '*' character.  
/// This would be the common prefix of all keys stored in a storage using this key selector.
///
/// Use this operation at creation of a Storage to get the keys prefix, and in [`Storage::on_sample()`](crate::Storage::on_sample())
/// strip this prefix from all received [`Sample::key_expr`](zenoh::prelude::Sample::key_expr) to retrieve the corrsponding key.
///
/// # Examples:
/// ```
/// # use zenoh_backend_traits::utils::get_keys_prefix;
/// assert_eq!("demo/example/", get_keys_prefix("demo/example/**"));
/// assert_eq!("demo/", get_keys_prefix("demo/**/test/**"));
/// assert_eq!("", get_keys_prefix("**"));
/// assert_eq!("demo/example/test", get_keys_prefix("demo/example/test"));
/// ```
pub fn get_keys_prefix(key_selector: &str) -> &str {
    // keys_prefix is the longest prefix in key_selector without a '*'
    key_selector
        .find('*')
        .map(|i| &key_selector[..i])
        .unwrap_or(key_selector)
}

/// Given a key selector and a prefix (usually returned from [`get_keys_prefix()`]) that is stripped from the keys to store,
/// this operation returns a list of key selectors allowing to match all the keys corresponding to the full keys that would have match
/// the given key selector.
///
/// Use this operation in [`Storage::on_query()`](crate::Storage::on_query()) implementation to transform the received
/// [`Query::selector()`](zenoh::queryable::Query::selector)`.`[`key_expr`](zenoh::prelude::Selector::key_expr) in a list of key selectors
/// that will match all the relevant stored keys (that correspond to keys stripped from the prefix).
///
/// # See also
/// [`get_keys_prefix()`]
///
/// # Examples:
/// ```
/// # use std::convert::TryInto;
/// # use zenoh_backend_traits::utils::get_sub_key_selectors;
/// assert_eq!(
///     ["**"],
///     get_sub_key_selectors("demo/example/test/**".try_into().unwrap(), "demo/example/test/").unwrap().as_slice()
/// );
/// assert_eq!(
///     ["**"],
///     get_sub_key_selectors("demo/example/**".try_into().unwrap(), "demo/example/test/").unwrap().as_slice()
/// );
/// assert_eq!(
///     ["**"],
///     get_sub_key_selectors("**".try_into().unwrap(), "demo/example/test/").unwrap().as_slice()
/// );
/// assert_eq!(
///     ["**/xyz"],
///     get_sub_key_selectors("demo/**/xyz".try_into().unwrap(), "demo/example/test/").unwrap().as_slice()
/// );
/// assert_eq!(
///     ["**"],
///     get_sub_key_selectors("demo/**/test/**".try_into().unwrap(), "demo/example/test/").unwrap().as_slice()
/// );
/// assert_eq!(
///     ["xyz", "**/ex$*/*/xyz"],
///     get_sub_key_selectors("demo/**/ex$*/*/xyz".try_into().unwrap(), "demo/example/test/").unwrap().as_slice()
/// );
/// ```
pub fn get_sub_key_selectors<'a>(
    key_selector: &'a keyexpr,
    prefix: &str,
) -> ZResult<Vec<&'a keyexpr>> {
    let prefix = prefix.strip_prefix('/').unwrap_or(prefix);
    let prefix = keyexpr::new(prefix.strip_suffix('/').unwrap_or(prefix))?;
    let mut result = vec![];
    'chunks: for i in (0..=key_selector.len()).rev() {
        if if i == key_selector.len() {
            key_selector.ends_with("**")
        } else {
            key_selector.as_bytes()[i] == b'/'
        } {
            let sub_part = keyexpr::new(&key_selector[..i]).unwrap();
            if sub_part.intersects(prefix) {
                // if sub_part ends with "**", keep those in remaining part
                let remaining = if sub_part.ends_with("**") {
                    &key_selector[i - 2..]
                } else {
                    &key_selector[i + 1..]
                };
                let remaining: &keyexpr = if remaining.is_empty() {
                    continue 'chunks;
                } else {
                    remaining
                }
                .try_into()
                .unwrap();
                // if remaining is "**" return only this since it covers all
                if remaining.as_bytes() == b"**" {
                    result.clear();
                    result.push(unsafe { keyexpr::from_str_unchecked(remaining) });
                    return Ok(result);
                }
                for i in (0..(result.len())).rev() {
                    if result[i].includes(remaining) {
                        continue 'chunks;
                    }
                    if remaining.includes(result[i]) {
                        result.swap_remove(i);
                    }
                }
                result.push(remaining);
            }
        }
    }
    Ok(result)
}

#[test]
fn test_get_sub_key_exprs() {
    let expectations = [
        (("demo/example/test/**", "demo/example/test/"), &["**"][..]),
        (("demo/example/**", "demo/example/test/"), &["**"]),
        (("**", "demo/example/test/"), &["**"]),
        (
            ("demo/example/test/**/x$*/**", "demo/example/test/"),
            &["**/x$*/**"],
        ),
        (("demo/**/xyz", "demo/example/test/"), &["**/xyz"]),
        (("demo/**/test/**", "demo/example/test/"), &["**"]),
        (
            ("demo/**/ex$*/*/xyz", "demo/example/test/"),
            ["xyz", "**/ex$*/*/xyz"].as_ref(),
        ),
        (
            ("demo/**/ex$*/t$*/xyz", "demo/example/test/"),
            ["xyz", "**/ex$*/t$*/xyz"].as_ref(),
        ),
        (
            ("demo/**/te$*/*/xyz", "demo/example/test/"),
            ["*/xyz", "**/te$*/*/xyz"].as_ref(),
        ),
        (("demo/example/test", "demo/example/test/"), [].as_ref()),
    ]
    .map(|((a, b), expected)| {
        (
            (keyexpr::new(a).unwrap(), b),
            expected
                .iter()
                .map(|s| keyexpr::new(*s).unwrap())
                .collect::<Vec<_>>(),
        )
    });
    for ((ke, prefix), expected) in expectations {
        dbg!(ke, prefix);
        assert_eq!(get_sub_key_selectors(ke, prefix).unwrap(), expected)
    }
}
